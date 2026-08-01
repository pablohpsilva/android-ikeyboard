//! Learn user vocabulary/habits behind the `SecureStore` port; user dictionary
//! and whitelist (the sole writer of the lexical learned-data domain, ADR-14).
//!
//! This crate owns **one** thing: what words *this user* uses. It keeps an
//! in-memory user dictionary (`word -> frequency count`) plus a whitelist of
//! words the user has explicitly marked as correct, and it is the *only*
//! component that writes the lexical user-dictionary namespace
//! ([`Namespace::UserDict`]), where it persists the whole model as one atomic
//! blob. Persistence, encryption and I/O are **not** done
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

/// Storage key for the model's single blob under [`Namespace::UserDict`].
/// Versioned so a future encoding change can be detected rather than silently
/// mis-parsed.
const BLOB_KEY: &[u8] = b"v1";

/// Storage key for the personal proper-noun blob (BR-69), a *separate* value
/// under [`Namespace::UserDict`]. Kept apart from [`BLOB_KEY`] because the
/// proper-noun encoding (folded → canonical) would be ambiguous inside the
/// frequency/whitelist blob. An install without this key loads an empty set.
const PROPER_KEY: &[u8] = b"proper_v1";

/// Upper bound on learned personal proper nouns (BR-69). Once full, new keys are
/// ignored (existing keys still update), so the encrypted blob stays bounded.
const PROPER_NOUN_CAP: usize = 2000;

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
/// The whole model — frequency map and whitelist together — is persisted as a
/// single atomic blob under [`Namespace::UserDict`] (the user's dictionary)
/// through the injected [`SecureStore`], so a persist can never leave the two
/// halves out of step with each other.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Personalization {
    /// Learned words and how often each has been observed. Counts are `>= 1`;
    /// a word is removed from consideration only by never being observed.
    frequencies: BTreeMap<String, u32>,
    /// Words the user has explicitly accepted, independent of frequency.
    whitelist: BTreeSet<String>,
    /// Personal proper nouns (BR-69): folded key → canonical-cased spelling,
    /// learned from words the user habitually capitalizes mid-sentence. Bounded
    /// by [`PROPER_NOUN_CAP`]; persisted under [`PROPER_KEY`].
    proper_nouns: BTreeMap<String, String>,
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

    /// The whole learned frequency map, read-only, for consumers that must
    /// *enumerate* it rather than probe a single word: the predictor's ranking
    /// snapshot and the shell's swipe-bias export. Ordered (`BTreeMap`), so any
    /// derived iteration is deterministic.
    #[must_use]
    pub fn frequencies(&self) -> &BTreeMap<String, u32> {
        &self.frequencies
    }

    /// Bulk-set learned frequencies from a prior export (migrating the legacy
    /// Kotlin `usage.tsv`). **Set-semantics**: each count *replaces* any existing
    /// value rather than adding to it, so re-running the same import is idempotent
    /// — the crash-safety guarantee the one-time migration relies on. Unstorable
    /// words (control chars) and non-positive counts are skipped, preserving the
    /// frequency-map invariant (counts are `>= 1`).
    pub fn import<I: IntoIterator<Item = (String, u32)>>(&mut self, frequencies: I) {
        for (word, count) in frequencies {
            if count > 0 && is_storable(&word) {
                self.frequencies.insert(word, count);
            }
        }
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

    /// Record a personal proper noun as `folded` key → `canonical` spelling
    /// (BR-69). Bounded: once at [`PROPER_NOUN_CAP`], a *new* key is ignored (an
    /// existing key still updates its canonical form). Either string carrying the
    /// codec's separators is rejected, exactly like [`observe`](Self::observe).
    pub fn observe_proper_noun(&mut self, folded: &str, canonical: &str) {
        if !is_storable(folded) || !is_storable(canonical) {
            return;
        }
        if !self.proper_nouns.contains_key(folded) && self.proper_nouns.len() >= PROPER_NOUN_CAP {
            return;
        }
        self.proper_nouns
            .insert(folded.to_owned(), canonical.to_owned());
    }

    /// The learned personal proper-noun set (folded → canonical), read-only.
    /// Ordered (`BTreeMap`) so any derived iteration is deterministic.
    #[must_use]
    pub fn proper_nouns(&self) -> &BTreeMap<String, String> {
        &self.proper_nouns
    }

    /// Encrypt-and-store the whole model through the injected store.
    ///
    /// The entire model — frequency dictionary *and* whitelist — is serialized
    /// into one blob and written with a **single** [`put`](SecureStore::put)
    /// under [`Namespace::UserDict`], the user's dictionary. Persisting as one
    /// atomic write means a failure can never leave the store holding a new
    /// dictionary beside a stale whitelist (or vice versa); either the whole new
    /// model lands or none of it does. This crate is the sole writer of that
    /// namespace (ADR-14).
    ///
    /// # Errors
    /// Propagates any [`StoreError`] from the underlying store; this crate adds
    /// no error of its own on the write path.
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), StoreError> {
        let blob = codec::encode_model(&self.frequencies, &self.whitelist);
        store.put(Namespace::UserDict, BLOB_KEY, &blob)?;
        // Proper nouns ride in their own blob (BR-69) — see `PROPER_KEY`.
        let proper = codec::encode_proper(&self.proper_nouns);
        store.put(Namespace::UserDict, PROPER_KEY, &proper)
    }

    /// Load a model previously written by [`persist`](Personalization::persist).
    ///
    /// Reads the single blob under [`Namespace::UserDict`]. A namespace with no
    /// stored blob loads as an empty model (a first run), so this never fails
    /// merely because the user has not typed yet.
    ///
    /// # Errors
    /// Returns the store's [`StoreError`] on a backend/crypto failure, or
    /// [`StoreError::Backend`] if the stored blob is corrupt (not valid UTF-8 or
    /// not in the expected encoding) — corruption is a backend fault, not a
    /// value the caller can act on.
    pub fn load(store: &impl SecureStore) -> Result<Self, StoreError> {
        let (frequencies, whitelist) = match store.get(Namespace::UserDict, BLOB_KEY)? {
            Some(bytes) => codec::decode_model(&bytes)?,
            None => (BTreeMap::new(), BTreeSet::new()),
        };
        // A missing proper-noun blob is a clean pre-BR-69 install: empty set.
        let proper_nouns = match store.get(Namespace::UserDict, PROPER_KEY)? {
            Some(bytes) => codec::decode_proper(&bytes)?,
            None => BTreeMap::new(),
        };
        Ok(Self {
            frequencies,
            whitelist,
            proper_nouns,
        })
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

    // --- BR-69: personal proper nouns ----------------------------------------

    #[derive(Default)]
    struct MemStore {
        map: std::cell::RefCell<std::collections::HashMap<Vec<u8>, Vec<u8>>>,
    }
    impl SecureStore for MemStore {
        fn put(&self, _ns: Namespace, key: &[u8], val: &[u8]) -> Result<(), StoreError> {
            self.map.borrow_mut().insert(key.to_vec(), val.to_vec());
            Ok(())
        }
        fn get(&self, _ns: Namespace, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
            Ok(self.map.borrow().get(key).cloned())
        }
    }

    #[test]
    fn learns_and_reads_back_a_personal_proper_noun() {
        let mut p = Personalization::new();
        p.observe_proper_noun("zoe", "Zoë");
        assert_eq!(p.proper_nouns().get("zoe").map(String::as_str), Some("Zoë"));
    }

    #[test]
    fn proper_noun_rejects_separator_bearing_strings() {
        let mut p = Personalization::new();
        p.observe_proper_noun("a\tb", "X");
        p.observe_proper_noun("k", "A\nB");
        p.observe_proper_noun("", "X");
        assert!(p.proper_nouns().is_empty());
    }

    #[test]
    fn proper_noun_map_is_bounded_but_updates_existing_keys() {
        let mut p = Personalization::new();
        for i in 0..(PROPER_NOUN_CAP + 50) {
            p.observe_proper_noun(&format!("k{i}"), &format!("K{i}"));
        }
        assert_eq!(p.proper_nouns().len(), PROPER_NOUN_CAP);
        // An existing key still updates its canonical form even when full.
        p.observe_proper_noun("k0", "Updated");
        assert_eq!(
            p.proper_nouns().get("k0").map(String::as_str),
            Some("Updated")
        );
    }

    #[test]
    fn proper_nouns_survive_persist_and_load() {
        let store = MemStore::default();
        let mut p = Personalization::new();
        p.observe("cat"); // the frequency blob rides alongside, untouched
        p.observe_proper_noun("zoe", "Zoë");
        p.persist(&store).unwrap();
        let loaded = Personalization::load(&store).unwrap();
        assert_eq!(
            loaded.proper_nouns().get("zoe").map(String::as_str),
            Some("Zoë")
        );
        assert_eq!(loaded.frequency("cat"), 1);
    }

    #[test]
    fn load_without_a_proper_blob_yields_an_empty_set() {
        let store = MemStore::default();
        let mut p = Personalization::new();
        p.observe_proper_noun("zoe", "Zoë");
        p.persist(&store).unwrap();
        // Simulate a pre-BR-69 install that never wrote the proper-noun blob.
        store.map.borrow_mut().remove(&PROPER_KEY.to_vec());
        let loaded = Personalization::load(&store).unwrap();
        assert!(loaded.proper_nouns().is_empty());
    }

    #[test]
    fn frequencies_exposes_the_learned_map() {
        let mut p = Personalization::new();
        p.observe("cat");
        p.observe("cat");
        p.observe("dog");
        let f = p.frequencies();
        assert_eq!(f.get("cat"), Some(&2));
        assert_eq!(f.get("dog"), Some(&1));
        assert_eq!(f.get("bird"), None);
    }

    #[test]
    fn import_sets_counts_and_is_idempotent() {
        let mut p = Personalization::new();
        p.observe("cat"); // pre-existing count of 1
        p.import([("cat".to_owned(), 5), ("dog".to_owned(), 3)]);
        // set-semantics: "cat" is replaced (5), not incremented to 6.
        assert_eq!(p.frequency("cat"), 5);
        assert_eq!(p.frequency("dog"), 3);
        // re-running the same import changes nothing (crash-safe migration).
        p.import([("cat".to_owned(), 5), ("dog".to_owned(), 3)]);
        assert_eq!(p.frequency("cat"), 5);
        assert_eq!(p.frequency("dog"), 3);
    }

    #[test]
    fn import_skips_unstorable_words_and_zero_counts() {
        let mut p = Personalization::new();
        p.import([
            ("ok".to_owned(), 4),
            ("zero".to_owned(), 0),  // non-positive → skipped
            ("ta\tb".to_owned(), 2), // contains tab → skipped
            ("".to_owned(), 7),      // empty → skipped
        ]);
        assert_eq!(p.frequency("ok"), 4);
        assert_eq!(p.frequency("zero"), 0);
        assert_eq!(p.frequency("ta\tb"), 0);
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
