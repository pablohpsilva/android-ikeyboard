//! On-device next-word (bigram) learning: `prev -> {next -> count}`, persisted as
//! one atomic encrypted blob under [`Namespace::PersonalLm`] through the injected
//! `SecureStore` port (the sole writer of that namespace). Nothing leaves the
//! device (BR-13). Gating (consent BR-22, sensitivity E-2/BR-26) happens upstream.
//!
//! Implemented in task W1b of the Tier-1 plan
//! (`docs/superpowers/plans/2026-07-26-tier1-smarter-learning-plan.md`).
//!
//! This crate mirrors the Kotlin `ContextModel`: it remembers, for each word the
//! user commits, which words tend to follow it, as `prev -> {next -> count}`.
//! Tokens shorter than two characters are skipped (a weak signal), and counts
//! saturate so an unbounded typing stream can never overflow or panic
//! (SEDD §5.5 r3). It owns the byte **codec** for its own learned state
//! (`codec.rs`) and persists it as one atomic blob under
//! [`Namespace::PersonalLm`]; encryption and I/O live in `secure-store`, reached
//! only through the [`SecureStore`] port (ADR-12 Dependency Rule).

use std::collections::BTreeMap;

use featherkey_contracts::{Namespace, SecureStore, StoreError};

mod codec;

/// Storage key for the model's single blob under [`Namespace::PersonalLm`].
/// Versioned so a future encoding change is detected rather than mis-parsed.
const BLOB_KEY: &[u8] = b"v1";

/// The shortest token worth learning. Words with fewer than this many characters
/// are a weak signal (articles, stray letters) and are skipped, mirroring the
/// Kotlin `ContextModel`.
pub const MIN_TOKEN_CHARS: usize = 2;

/// A token is storable only if it is free of the codec's line/field separators
/// (`\n`, `\t`). A token containing one would corrupt the encoded blob (making
/// the model unloadable) or silently split into two tokens on load. Typed tokens
/// never contain these; this guards the import path and misuse, exactly like
/// `personalization`'s `is_storable`.
pub fn is_storable(token: &str) -> bool {
    !token.contains(['\n', '\t'])
}

/// `true` if a token is long enough and clean enough to learn.
/// A token is learnable if it contains at least [`MIN_TOKEN_CHARS`] characters
/// and is free of codec separators (`\n`, `\t`). This predicate ensures that
/// only storable, meaningful tokens are recorded in the context model.
pub fn is_learnable(token: &str) -> bool {
    token.chars().count() >= MIN_TOKEN_CHARS && is_storable(token)
}

/// A per-user next-word (bigram) model: for each previous word, how often each
/// word has followed it.
///
/// * [`record`](Context::record) folds one observed `prev -> next` transition.
/// * [`next_words`](Context::next_words) ranks the likeliest next words.
/// * [`next_counts`](Context::next_counts) exposes the raw counts after a word.
/// * [`import`](Context::import) bulk-loads pre-computed transitions (migration).
///
/// The whole model is persisted as a single atomic blob under
/// [`Namespace::PersonalLm`] through the injected [`SecureStore`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    /// `prev -> (next -> count)`. `BTreeMap` at both levels so the codec encodes
    /// deterministically (stable bytes for equal models). Counts are `>= 1`
    /// under [`record`](Context::record); a transition is forgotten only by
    /// never being recorded.
    frequencies: BTreeMap<String, BTreeMap<String, u32>>,
}

impl Context {
    /// A fresh model that has learned no transitions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` while nothing has been learned yet (a fresh or reset model).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frequencies.is_empty()
    }

    /// Record that `next` followed `prev`, incrementing that transition's count.
    ///
    /// Tokens shorter than two characters, or containing a codec separator, are
    /// skipped (a weak or unstorable signal), mirroring the Kotlin
    /// `ContextModel`. The count saturates at [`u32::MAX`] so an unbounded
    /// typing stream can never overflow or panic.
    pub fn record(&mut self, prev: &str, next: &str) {
        if !is_learnable(prev) || !is_learnable(next) {
            return;
        }
        let inner = self.frequencies.entry(prev.to_owned()).or_default();
        let count = inner.entry(next.to_owned()).or_insert(0);
        *count = count.saturating_add(1);
    }

    /// The words most often typed after `prev`, most-frequent first.
    ///
    /// Ties (equal counts) are broken by ascending word order, so the ranking is
    /// fully deterministic. At most `limit` words are returned; an unknown `prev`
    /// yields an empty vector.
    #[must_use]
    pub fn next_words(&self, prev: &str, limit: usize) -> Vec<String> {
        let Some(inner) = self.frequencies.get(prev) else {
            return Vec::new();
        };
        // `inner` iterates in ascending word order; a stable sort by descending
        // count therefore keeps ties in ascending word order.
        let mut entries: Vec<(&String, &u32)> = inner.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        entries
            .into_iter()
            .take(limit)
            .map(|(word, _)| word.clone())
            .collect()
    }

    /// The raw next-word counts after `prev` (empty if none / no context).
    #[must_use]
    pub fn next_counts(&self, prev: &str) -> BTreeMap<String, u32> {
        self.frequencies.get(prev).cloned().unwrap_or_default()
    }

    /// Bulk-load pre-computed `(prev, next, count)` transitions for migration
    /// (e.g. importing the Kotlin `context.tsv`).
    ///
    /// Counts are **set** (last-write-wins), not accumulated: a re-imported
    /// `(prev, next)` overwrites its prior count rather than adding to it. This
    /// gives the W6a migration idempotency — re-running it (a partial-failure
    /// retry, or input with duplicate `(prev, next)` rows) converges to the same
    /// model instead of inflating counts. Mirrors [`codec::decode`]'s insert.
    /// Transitions whose tokens contain a codec separator are skipped so the
    /// persisted blob can never be corrupted.
    pub fn import<I: IntoIterator<Item = (String, String, u32)>>(&mut self, transitions: I) {
        for (prev, next, count) in transitions {
            if !is_storable(&prev) || !is_storable(&next) {
                continue;
            }
            self.frequencies
                .entry(prev)
                .or_default()
                .insert(next, count);
        }
    }

    /// Encrypt-and-store the whole model as one atomic blob under
    /// [`Namespace::PersonalLm`] through the injected store. A single
    /// [`put`](SecureStore::put) means a failure can never leave a
    /// partially-written model. This crate is the sole writer of that namespace.
    ///
    /// # Errors
    /// Propagates any [`StoreError`] from the underlying store.
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), StoreError> {
        let blob = codec::encode(&self.frequencies);
        store.put(Namespace::PersonalLm, BLOB_KEY, &blob)
    }

    /// Load a model previously written by [`persist`](Context::persist). A
    /// namespace with no stored blob loads as an empty model (first run), so this
    /// never fails merely because the user has no history yet.
    ///
    /// # Errors
    /// The store's [`StoreError`] on a backend/crypto failure, or
    /// [`StoreError::Backend`] if the stored blob is corrupt (not valid UTF-8 or
    /// not in the expected encoding).
    pub fn load(store: &impl SecureStore) -> Result<Self, StoreError> {
        let frequencies = match store.get(Namespace::PersonalLm, BLOB_KEY)? {
            Some(bytes) => codec::decode(&bytes)?,
            None => BTreeMap::new(),
        };
        Ok(Self { frequencies })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn records_and_ranks_next_words() {
        let mut c = Context::new();
        c.record("the", "cat");
        c.record("the", "cat");
        c.record("the", "dog");
        assert_eq!(
            c.next_words("the", 2),
            vec!["cat".to_string(), "dog".to_string()]
        );
        assert_eq!(c.next_counts("the").get("cat"), Some(&2));
    }

    #[test]
    fn skips_short_tokens() {
        let mut c = Context::new();
        c.record("a", "cat"); // prev too short
        c.record("the", "x"); // next too short
        assert!(c.next_words("the", 3).is_empty());
        assert!(c.next_counts("a").is_empty());
    }

    #[test]
    fn counts_saturate_at_u32_max() {
        let mut c = Context::new();
        c.import([("aa".to_string(), "bb".to_string(), u32::MAX)]);
        c.record("aa", "bb"); // would overflow, must saturate
        assert_eq!(c.next_counts("aa").get("bb"), Some(&u32::MAX));
    }

    #[test]
    fn ties_break_by_ascending_word() {
        let mut c = Context::new();
        c.record("go", "north");
        c.record("go", "east");
        // Equal counts (1 each) -> ascending word order: east before north.
        assert_eq!(
            c.next_words("go", 2),
            vec!["east".to_string(), "north".to_string()]
        );
    }

    #[test]
    fn import_sets_counts_and_skips_unstorable() {
        let mut c = Context::new();
        c.import([
            ("hi".to_string(), "there".to_string(), 3),
            ("hi".to_string(), "there".to_string(), 2), // duplicate: last write wins
            ("bad\tprev".to_string(), "ok".to_string(), 9), // unstorable, skipped
        ]);
        // Set-semantics (not accumulate): a re-imported (prev, next) overwrites,
        // so a W6a migration re-run converges instead of inflating counts.
        assert_eq!(c.next_counts("hi").get("there"), Some(&2));
        assert!(c.next_counts("bad\tprev").is_empty());
    }

    #[test]
    fn import_is_idempotent_when_re_run() {
        // Re-running the same migration input must converge (crash-safe retry),
        // which requires set-semantics, not accumulation.
        let rows = || {
            [
                ("the".to_string(), "cat".to_string(), 7),
                ("go".to_string(), "north".to_string(), 4),
            ]
        };
        let mut once = Context::new();
        once.import(rows());
        let mut twice = Context::new();
        twice.import(rows());
        twice.import(rows());
        assert_eq!(once, twice);
        assert_eq!(twice.next_counts("the").get("cat"), Some(&7));
    }

    #[test]
    fn unknown_prev_yields_empty() {
        let c = Context::new();
        assert!(c.next_words("nope", 5).is_empty());
        assert!(c.next_counts("nope").is_empty());
        assert!(c.is_empty());
    }

    #[test]
    fn learnable_predicate_is_public_and_matches_record_rules() {
        assert!(is_learnable("cat"));
        assert!(!is_learnable("a")); // < MIN_TOKEN_CHARS
        assert!(!is_learnable("bad\ttok")); // separator
        assert_eq!(MIN_TOKEN_CHARS, 2);
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
    fn learned_transitions_survive_persist_then_load() {
        let store = MemStore::default();
        let mut c = Context::new();
        c.record("the", "cat");
        c.record("the", "cat");
        c.record("the", "dog");
        c.record("big", "dog");
        c.persist(&store).unwrap();

        let loaded = Context::load(&store).unwrap();
        assert_eq!(loaded, c);
        assert_eq!(loaded.next_counts("the").get("cat"), Some(&2));
        assert_eq!(
            loaded.next_words("the", 2),
            vec!["cat".to_string(), "dog".to_string()]
        );
    }

    #[test]
    fn an_absent_blob_loads_an_empty_model() {
        let store = MemStore::default();
        let loaded = Context::load(&store).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn a_corrupt_blob_loads_as_backend_error() {
        let store = MemStore::default();
        store.put(Namespace::PersonalLm, BLOB_KEY, &[0xff]).unwrap();
        assert_eq!(Context::load(&store).err(), Some(StoreError::Backend));
    }
}
