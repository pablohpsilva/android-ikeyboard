//! Learn user vocabulary/habits behind the `SecureStore` port; user dictionary
//! and whitelist (the sole writer of the lexical learned-data domain, ADR-14).
//!
//! This crate owns **one** thing: what words *this user* uses. It keeps an
//! in-memory user dictionary (`word -> frequency count`) plus a whitelist of
//! words the user has explicitly marked as correct, and it is the *only*
//! component that writes the lexical namespaces ([`Namespace::UserDict`] and
//! [`Namespace::PersonalLm`]). Persistence, encryption and I/O are **not** done
//! here — they belong to `secure-store`, reached only through the
//! [`SecureStore`] port (SEDD §5.4, ADR-12 Dependency Rule). This crate depends
//! on the *trait*, never the adapter, so the composition root injects the
//! concrete store.
//!
//! Closes (this crate's part of the MVP substrate) BR-7 (learn the user's
//! vocabulary) and BR-13 (on-device only): there is no network, no clock and no
//! global state — every byte of learned data is local and flows through the
//! injected store, so "on-device only" holds *structurally*.
//!
//! Note: sensitive-field gating (never learn from password/OTP fields, BR-26)
//! happens **upstream** at the composition root via the `SensitiveContextSource`
//! port (`sensitive-context`), which suppresses `observe` calls before they
//! reach this crate. This model performs no gating of its own — if it is told to
//! observe a word, it learns it.

use std::collections::{BTreeMap, BTreeSet};

use featherkey_contracts::{Namespace, SecureStore, StoreError};

mod codec;

/// Storage key for the single blob each lexical namespace holds. Versioned so a
/// future encoding change can be detected rather than silently mis-parsed.
const BLOB_KEY: &[u8] = b"v1";

/// A word is storable only if it is non-empty and free of the codec's line/field
/// separators (`\n`, `\t`). A word containing one would corrupt the encoded blob
/// (making the model unloadable) or silently split into two words on load. Typed
/// tokens never contain these; this guards the import path (BR-57) and misuse.
fn is_storable(word: &str) -> bool {
    !word.is_empty() && !word.contains(['\n', '\t'])
}

/// A per-user lexical model: a frequency-counted user dictionary plus a
/// whitelist of explicitly-accepted words.
///
/// * [`observe`](Personalization::observe) folds a typed word into the learned
///   counts.
/// * [`whitelist`](Personalization::whitelist) marks a word as always-correct
///   even if it has never been typed.
/// * [`is_known`](Personalization::is_known) answers whether a word should be
///   treated as valid vocabulary (learned *or* whitelisted).
///
/// The frequency map is persisted under [`Namespace::PersonalLm`] (personal
/// unigram counts) and the whitelist under [`Namespace::UserDict`] (the user's
/// explicit dictionary), through the injected [`SecureStore`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Personalization {
    /// Learned words and how often each has been observed. Counts are `>= 1`;
    /// a word is removed from consideration only by never being observed.
    frequencies: BTreeMap<String, u32>,
    /// Words the user has explicitly accepted, independent of frequency.
    whitelist: BTreeSet<String>,
}

impl Personalization {
    /// A fresh model that knows no words.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one observation of `word`, incrementing its learned frequency.
    ///
    /// Empty input is ignored: an empty string is not a word, and admitting it
    /// would make the persisted encoding ambiguous with "no words". The count
    /// saturates at [`u32::MAX`] so an unbounded typing stream can never
    /// overflow or panic (SEDD §5.5 r3).
    pub fn observe(&mut self, word: &str) {
        if !is_storable(word) {
            return;
        }
        let count = self.frequencies.entry(word.to_owned()).or_insert(0);
        *count = count.saturating_add(1);
    }

    /// How many times `word` has been observed. `0` if never seen (a
    /// whitelisted-but-untyped word still reports `0`).
    #[must_use]
    pub fn frequency(&self, word: &str) -> u32 {
        self.frequencies.get(word).copied().unwrap_or(0)
    }

    /// `true` if `word` is part of the user's vocabulary — either it has been
    /// observed at least once, or it is on the whitelist.
    #[must_use]
    pub fn is_known(&self, word: &str) -> bool {
        self.frequencies.contains_key(word) || self.whitelist.contains(word)
    }

    /// Mark `word` as always-correct, independent of how often it is typed.
    ///
    /// Empty input is ignored for the same encoding-unambiguity reason as
    /// [`observe`](Personalization::observe).
    pub fn whitelist(&mut self, word: &str) {
        if !is_storable(word) {
            return;
        }
        self.whitelist.insert(word.to_owned());
    }

    /// Encrypt-and-store the whole model through the injected store.
    ///
    /// The frequency dictionary is written under [`Namespace::PersonalLm`] and
    /// the whitelist under [`Namespace::UserDict`]. This crate is the sole
    /// writer of both (ADR-14).
    ///
    /// # Errors
    /// Propagates any [`StoreError`] from the underlying store; this crate adds
    /// no error of its own on the write path.
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), StoreError> {
        let dict = codec::encode_frequencies(&self.frequencies);
        store.put(Namespace::PersonalLm, BLOB_KEY, &dict)?;
        let whitelist = codec::encode_whitelist(&self.whitelist);
        store.put(Namespace::UserDict, BLOB_KEY, &whitelist)?;
        Ok(())
    }

    /// Load a model previously written by [`persist`](Personalization::persist).
    ///
    /// A namespace with no stored blob loads as empty (a first run), so this
    /// never fails merely because the user has not typed yet.
    ///
    /// # Errors
    /// Returns the store's [`StoreError`] on a backend/crypto failure, or
    /// [`StoreError::Backend`] if a stored blob is corrupt (not valid UTF-8 or
    /// not in the expected encoding) — corruption is a backend fault, not a
    /// value the caller can act on.
    pub fn load(store: &impl SecureStore) -> Result<Self, StoreError> {
        let frequencies = match store.get(Namespace::PersonalLm, BLOB_KEY)? {
            Some(bytes) => codec::decode_frequencies(&bytes)?,
            None => BTreeMap::new(),
        };
        let whitelist = match store.get(Namespace::UserDict, BLOB_KEY)? {
            Some(bytes) => codec::decode_whitelist(&bytes)?,
            None => BTreeSet::new(),
        };
        Ok(Self { frequencies, whitelist })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn new_model_knows_nothing() {
        let p = Personalization::new();
        assert_eq!(p.frequency("word"), 0);
        assert!(!p.is_known("word"));
        // `Default` and `new` agree.
        assert_eq!(p, Personalization::default());
    }

    #[test]
    fn observe_increments_frequency_each_time() {
        let mut p = Personalization::new();
        assert_eq!(p.frequency("cat"), 0);
        p.observe("cat");
        assert_eq!(p.frequency("cat"), 1);
        p.observe("cat");
        p.observe("cat");
        assert_eq!(p.frequency("cat"), 3);
    }

    #[test]
    fn observing_makes_a_word_known() {
        let mut p = Personalization::new();
        assert!(!p.is_known("dog"));
        p.observe("dog");
        assert!(p.is_known("dog"));
    }

    #[test]
    fn observe_ignores_the_empty_string() {
        let mut p = Personalization::new();
        p.observe("");
        assert_eq!(p.frequency(""), 0);
        assert!(!p.is_known(""));
    }

    #[test]
    fn separator_bearing_words_are_rejected() {
        let mut p = Personalization::new();
        // A word carrying the codec's separators must not enter the model, or a
        // persist/load round-trip would corrupt it or silently split it in two.
        p.observe("a\nb");
        p.observe("c\td");
        p.whitelist("e\nf");
        assert_eq!(p.frequency("a\nb"), 0);
        assert!(!p.is_known("e\nf"));
        // A clean word is still accepted.
        p.observe("ok");
        assert_eq!(p.frequency("ok"), 1);
    }

    #[test]
    fn whitelist_makes_a_word_known_without_frequency() {
        let mut p = Personalization::new();
        p.whitelist("acme");
        assert!(p.is_known("acme"));
        // Whitelisting is not an observation: frequency stays 0.
        assert_eq!(p.frequency("acme"), 0);
    }

    #[test]
    fn whitelist_ignores_the_empty_string() {
        let mut p = Personalization::new();
        p.whitelist("");
        assert!(!p.is_known(""));
    }

    #[test]
    fn distinct_words_are_counted_independently() {
        let mut p = Personalization::new();
        p.observe("a");
        p.observe("b");
        p.observe("b");
        assert_eq!(p.frequency("a"), 1);
        assert_eq!(p.frequency("b"), 2);
        assert_eq!(p.frequency("c"), 0);
    }
}
