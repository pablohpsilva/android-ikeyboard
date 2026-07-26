//! On-device correction-signal learning: strip-pick preferences
//! (`prefix -> {picked -> count}`) and low-weight `unwanted` words, persisted as
//! one atomic encrypted blob under [`Namespace::Corrections`] through the injected
//! `SecureStore` port (the sole writer of that namespace). Nothing leaves the
//! device (BR-13). Gating (consent BR-22, sensitivity E-2/BR-26) happens upstream.
//!
//! This crate owns **one** thing: what the user's *corrections* reveal about
//! their intended vocabulary. It keeps two learned maps in memory —
//!
//! * `prefs`: for a typed `prefix`, how often each `picked` word was chosen from
//!   the suggestion strip (`prefix -> {picked -> count}`), so a lower-ranked but
//!   repeatedly-chosen completion can be promoted;
//! * `unwanted`: how often a word was reverted/deleted right after being
//!   offered, a low-weight demotion signal (`word -> count`).
//!
//! Both maps are serialized into **one** blob and written with a single atomic
//! [`put`](SecureStore::put) under [`Namespace::Corrections`], so a persist can
//! never leave the two halves out of step. Persistence, encryption and I/O are
//! **not** done here — they belong to `secure-store`, reached only through the
//! [`SecureStore`] port (SEDD §5.4, ADR-12 Dependency Rule).
//!
//! Note: sensitive-field gating (never learn from password/OTP fields, BR-26)
//! happens **upstream** at the composition root; this model performs no gating of
//! its own — if it is told to note a signal, it records it.
//!
//! Implemented in task W1c of the Tier-1 plan
//! (`docs/superpowers/plans/2026-07-26-tier1-smarter-learning-plan.md`).

use std::collections::BTreeMap;

use featherkey_contracts::{Namespace, SecureStore, StoreError};

mod codec;

/// Storage key for the model's single blob under [`Namespace::Corrections`].
/// Versioned so a future encoding change can be detected rather than silently
/// mis-parsed.
const BLOB_KEY: &[u8] = b"v1";

/// A token (prefix, picked word, or unwanted word) is storable only if it is
/// non-empty and free of the codec's line/field separators (`\n`, `\t`). A token
/// containing one would corrupt the encoded blob (making the model unloadable) or
/// silently split into two on load. Typed tokens never contain these; this guards
/// the import path (BR-57) and misuse.
fn is_storable(token: &str) -> bool {
    !token.is_empty() && !token.contains(['\n', '\t'])
}

/// A per-user correction-signal model.
///
/// * [`note_pick`](Corrections::note_pick) records that, for a typed `prefix`,
///   the user chose `picked` from the suggestion strip.
/// * [`note_unwanted`](Corrections::note_unwanted) records a low-weight demotion
///   signal for a word the user reverted/deleted after it was offered.
///
/// Both learned maps are persisted together as a single atomic blob under
/// [`Namespace::Corrections`] through the injected [`SecureStore`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Corrections {
    /// For each typed `prefix`, how often each `picked` word was chosen. Counts
    /// are `>= 1`. Both levels are `BTreeMap` for a deterministic codec.
    prefs: BTreeMap<String, BTreeMap<String, u32>>,
    /// How often each word was flagged unwanted (reverted/deleted after being
    /// offered). Counts are `>= 1`.
    unwanted: BTreeMap<String, u32>,
}

impl Corrections {
    /// A fresh model that knows no corrections.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that, for the typed `prefix`, the user picked `picked` from the
    /// strip, incrementing that preference's count.
    ///
    /// Either token being empty or separator-bearing is ignored (the encoding
    /// would otherwise be corrupted or ambiguous). The count saturates at
    /// [`u32::MAX`] so an unbounded stream can never overflow or panic
    /// (SEDD §5.5 r3).
    pub fn note_pick(&mut self, prefix: &str, picked: &str) {
        if !is_storable(prefix) || !is_storable(picked) {
            return;
        }
        let count = self
            .prefs
            .entry(prefix.to_owned())
            .or_default()
            .entry(picked.to_owned())
            .or_insert(0);
        *count = count.saturating_add(1);
    }

    /// Record one low-weight `unwanted` signal for `word`, incrementing its
    /// count. Empty or separator-bearing input is ignored; the count saturates.
    pub fn note_unwanted(&mut self, word: &str) {
        if !is_storable(word) {
            return;
        }
        let count = self.unwanted.entry(word.to_owned()).or_insert(0);
        *count = count.saturating_add(1);
    }

    /// How often `word` was picked from the strip for the typed `prefix`. `0` if
    /// never (an unknown prefix or an unknown pick both report `0`).
    #[must_use]
    pub fn pref_count(&self, prefix: &str, word: &str) -> u32 {
        self.prefs
            .get(prefix)
            .and_then(|picks| picks.get(word))
            .copied()
            .unwrap_or(0)
    }

    /// How often `word` was flagged unwanted. `0` if never.
    #[must_use]
    pub fn unwanted_count(&self, word: &str) -> u32 {
        self.unwanted.get(word).copied().unwrap_or(0)
    }

    /// Bulk-set strip-pick preference counts, for migrating legacy data.
    ///
    /// Counts are **set** (not incremented), so re-running an import is
    /// idempotent. Empty/separator-bearing tokens and zero counts are skipped
    /// (a zero count is indistinguishable from "never seen", and admitting it
    /// would bloat the blob with dead records).
    pub fn import_prefs<I>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (String, String, u32)>,
    {
        for (prefix, picked, count) in entries {
            if count == 0 || !is_storable(&prefix) || !is_storable(&picked) {
                continue;
            }
            self.prefs.entry(prefix).or_default().insert(picked, count);
        }
    }

    /// Bulk-set unwanted counts, for migrating legacy data. Counts are **set**
    /// (idempotent); empty/separator-bearing words and zero counts are skipped.
    pub fn import_unwanted<I>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (String, u32)>,
    {
        for (word, count) in entries {
            if count == 0 || !is_storable(&word) {
                continue;
            }
            self.unwanted.insert(word, count);
        }
    }

    /// Encrypt-and-store the whole model through the injected store.
    ///
    /// Both learned maps are serialized into one blob and written with a
    /// **single** [`put`](SecureStore::put) under [`Namespace::Corrections`], so
    /// a failure can never leave the two halves out of step; either the whole new
    /// model lands or none of it does. This crate is the sole writer of that
    /// namespace.
    ///
    /// # Errors
    /// Propagates any [`StoreError`] from the underlying store; this crate adds
    /// no error of its own on the write path.
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), StoreError> {
        let blob = codec::encode_model(&self.prefs, &self.unwanted);
        store.put(Namespace::Corrections, BLOB_KEY, &blob)
    }

    /// Load a model previously written by [`persist`](Corrections::persist).
    ///
    /// A namespace with no stored blob loads as an empty model (a first run), so
    /// this never fails merely because the user has corrected nothing yet.
    ///
    /// # Errors
    /// Returns the store's [`StoreError`] on a backend/crypto failure, or
    /// [`StoreError::Backend`] if the stored blob is corrupt (not valid UTF-8 or
    /// not in the expected encoding) — corruption is a backend fault, not a value
    /// the caller can act on.
    pub fn load(store: &impl SecureStore) -> Result<Self, StoreError> {
        let (prefs, unwanted) = match store.get(Namespace::Corrections, BLOB_KEY)? {
            Some(bytes) => codec::decode_model(&bytes)?,
            None => (BTreeMap::new(), BTreeMap::new()),
        };
        Ok(Self { prefs, unwanted })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn new_model_knows_nothing() {
        let c = Corrections::new();
        assert_eq!(c.pref_count("teh", "teh"), 0);
        assert_eq!(c.unwanted_count("ducking"), 0);
        assert_eq!(c, Corrections::default());
    }

    #[test]
    fn records_strip_pick_prefs_and_unwanted() {
        let mut c = Corrections::new();
        c.note_pick("teh", "teh");
        c.note_pick("teh", "teh");
        c.note_unwanted("ducking");
        assert_eq!(c.pref_count("teh", "teh"), 2);
        assert_eq!(c.pref_count("teh", "other"), 0);
        assert_eq!(c.unwanted_count("ducking"), 1);
    }

    #[test]
    fn distinct_prefixes_and_picks_are_counted_independently() {
        let mut c = Corrections::new();
        c.note_pick("te", "the");
        c.note_pick("te", "teh");
        c.note_pick("te", "teh");
        c.note_pick("ca", "cat");
        assert_eq!(c.pref_count("te", "the"), 1);
        assert_eq!(c.pref_count("te", "teh"), 2);
        assert_eq!(c.pref_count("ca", "cat"), 1);
        assert_eq!(c.pref_count("ca", "the"), 0);
    }

    #[test]
    fn note_pick_ignores_empty_or_separator_bearing_tokens() {
        let mut c = Corrections::new();
        c.note_pick("", "the");
        c.note_pick("te", "");
        c.note_pick("a\tb", "the");
        c.note_pick("te", "a\nb");
        assert_eq!(c.pref_count("", "the"), 0);
        assert_eq!(c.pref_count("te", ""), 0);
        assert_eq!(c.pref_count("a\tb", "the"), 0);
        assert_eq!(c.pref_count("te", "a\nb"), 0);
        // A clean pick is still accepted.
        c.note_pick("te", "the");
        assert_eq!(c.pref_count("te", "the"), 1);
    }

    #[test]
    fn note_unwanted_ignores_empty_or_separator_bearing_words() {
        let mut c = Corrections::new();
        c.note_unwanted("");
        c.note_unwanted("a\tb");
        c.note_unwanted("a\nb");
        assert_eq!(c.unwanted_count(""), 0);
        assert_eq!(c.unwanted_count("a\tb"), 0);
        c.note_unwanted("ok");
        assert_eq!(c.unwanted_count("ok"), 1);
    }

    #[test]
    fn import_prefs_sets_counts_idempotently() {
        let mut c = Corrections::new();
        let entries = [
            ("te".to_owned(), "teh".to_owned(), 5),
            ("ca".to_owned(), "cat".to_owned(), 2),
        ];
        c.import_prefs(entries.iter().cloned());
        assert_eq!(c.pref_count("te", "teh"), 5);
        assert_eq!(c.pref_count("ca", "cat"), 2);
        // Re-import sets (not increments) — idempotent.
        c.import_prefs(entries.iter().cloned());
        assert_eq!(c.pref_count("te", "teh"), 5);
    }

    #[test]
    fn import_unwanted_sets_counts_idempotently() {
        let mut c = Corrections::new();
        c.import_unwanted([("ducking".to_owned(), 4)]);
        assert_eq!(c.unwanted_count("ducking"), 4);
        c.import_unwanted([("ducking".to_owned(), 4)]);
        assert_eq!(c.unwanted_count("ducking"), 4);
    }

    #[test]
    fn import_skips_zero_counts_and_bad_tokens() {
        let mut c = Corrections::new();
        c.import_prefs([
            ("te".to_owned(), "teh".to_owned(), 0),
            ("".to_owned(), "x".to_owned(), 3),
            ("a\tb".to_owned(), "y".to_owned(), 3),
        ]);
        c.import_unwanted([("z".to_owned(), 0), ("a\nb".to_owned(), 3)]);
        assert_eq!(c.pref_count("te", "teh"), 0);
        assert_eq!(c.unwanted_count("z"), 0);
        assert_eq!(c, Corrections::new());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod persistence_tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap as Map;

    /// `(namespace, key) -> value` backing map for the test store.
    type StoreData = Map<(String, Vec<u8>), Vec<u8>>;

    /// A minimal in-memory `SecureStore` for exercising persist/load without the
    /// real encrypted redb adapter.
    #[derive(Default)]
    struct MemStore {
        data: RefCell<StoreData>,
    }
    impl SecureStore for MemStore {
        fn put(&self, ns: Namespace, key: &[u8], val: &[u8]) -> Result<(), StoreError> {
            self.data
                .borrow_mut()
                .insert((ns.as_str().to_owned(), key.to_vec()), val.to_vec());
            Ok(())
        }
        fn get(&self, ns: Namespace, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
            Ok(self
                .data
                .borrow()
                .get(&(ns.as_str().to_owned(), key.to_vec()))
                .cloned())
        }
    }

    #[test]
    fn signals_survive_persist_then_load() {
        let store = MemStore::default();
        let mut c = Corrections::new();
        c.note_pick("te", "teh");
        c.note_pick("te", "teh");
        c.note_pick("ca", "cat");
        c.note_unwanted("ducking");
        c.persist(&store).unwrap();

        let loaded = Corrections::load(&store).unwrap();
        assert_eq!(loaded, c);
        assert_eq!(loaded.pref_count("te", "teh"), 2);
        assert_eq!(loaded.pref_count("ca", "cat"), 1);
        assert_eq!(loaded.unwanted_count("ducking"), 1);
    }

    #[test]
    fn an_absent_blob_loads_an_empty_model() {
        let store = MemStore::default();
        let loaded = Corrections::load(&store).unwrap();
        assert_eq!(loaded, Corrections::new());
    }

    #[test]
    fn an_empty_model_round_trips_through_the_store() {
        let store = MemStore::default();
        let c = Corrections::new();
        c.persist(&store).unwrap();
        assert_eq!(Corrections::load(&store).unwrap(), c);
    }
}
