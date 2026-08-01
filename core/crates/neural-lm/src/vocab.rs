//! Bounded per-user word ↔ index map.
//!
//! [`Vocab`] interns learnable words into stable small integer indices so a
//! later `NextWordLm` can embed them. It is deliberately dumb: it owns only
//! the string↔index map and per-word frequencies, nothing about the model
//! itself. Two indices are reserved and never assigned to a word: `UNK` (0,
//! also the answer for any out-of-vocabulary or non-learnable token) and
//! `BOS` (1, a padding/beginning-of-sequence marker a later task uses to
//! prime a fixed-width context window).
//!
//! Learnability reuses `featherkey_context::is_learnable` — the same rule
//! the bigram `Context` model uses to decide what is worth remembering —
//! so "is this token worth learning" has exactly one definition in the
//! workspace.
//!
//! Backed by `BTreeMap` at every level so iteration order (and therefore
//! any later encoding of this state) is deterministic.

use std::collections::BTreeMap;

use featherkey_context::is_learnable;

/// Reserved index for out-of-vocabulary / non-learnable tokens.
pub const UNK: usize = 0;
/// Reserved index for the beginning-of-sequence / padding marker.
pub const BOS: usize = 1;
/// The smallest index a learned word may ever be assigned.
const FIRST_LEARNED_INDEX: usize = 2;
/// The real ceiling on the number of *learned* words (excludes `UNK`/`BOS`).
pub const MAX_VOCAB: usize = 2000;

/// A bounded per-user word ↔ index map.
///
/// Interning is capacity-bounded: once `learned_ceiling` learned words are
/// registered, the next new word evicts the least-frequent existing one
/// (ties broken by smallest index) and reuses its freed index, so a
/// pathological typing stream can never grow this structure without bound.
#[derive(Debug, Clone)]
pub struct Vocab {
    /// word -> (index, frequency).
    by_word: BTreeMap<String, (usize, u32)>,
    /// index -> word, the reverse of `by_word` for O(log n) `word_of`.
    by_index: BTreeMap<usize, String>,
    /// Ceiling on the number of learned (non-reserved) entries.
    learned_ceiling: usize,
}

impl Default for Vocab {
    fn default() -> Self {
        Self::new()
    }
}

impl Vocab {
    /// A fresh, empty vocabulary with the real ceiling ([`MAX_VOCAB`]).
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_word: BTreeMap::new(),
            by_index: BTreeMap::new(),
            learned_ceiling: MAX_VOCAB,
        }
    }

    /// Test-only constructor with a small ceiling, so eviction can be
    /// exercised without registering thousands of words. Never public API.
    #[cfg(test)]
    #[must_use]
    pub fn with_capacity_for_test(learned_ceiling: usize) -> Self {
        Self {
            by_word: BTreeMap::new(),
            by_index: BTreeMap::new(),
            learned_ceiling,
        }
    }

    /// The number of currently learned (non-reserved) words.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_word.len()
    }

    /// `true` if no word has been learned yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_word.is_empty()
    }

    /// Look up `word`'s index without registering it. [`UNK`] if absent.
    #[must_use]
    pub fn index_of(&self, word: &str) -> usize {
        self.by_word.get(word).map_or(UNK, |&(index, _)| index)
    }

    /// The word registered at `index`, if any. The reserved [`UNK`]/[`BOS`]
    /// indices are never assigned a word by [`intern`](Vocab::intern), so
    /// this always answers `None` for them, made explicit here rather than
    /// left to `by_index` happening to stay empty at those slots.
    #[must_use]
    pub fn word_of(&self, index: usize) -> Option<&str> {
        if index == UNK || index == BOS {
            return None;
        }
        self.by_index.get(&index).map(String::as_str)
    }

    /// Intern `word`, returning its stable index and, when that index was
    /// just freed by an eviction, `Some(index)` — the same index, repeated,
    /// so a caller can tell "this index is reused, its previous occupant's
    /// learned state (e.g. an embedding row) is now stale" apart from "this
    /// index is either brand new or was already this word's, its state is
    /// still valid" without a separate lookup. `None` in every other case:
    ///
    /// * A non-learnable token (per [`is_learnable`]) is never registered and
    ///   always returns `(UNK, None)`.
    /// * An already-known word has its frequency bumped and its existing
    ///   index returned, unchanged (idempotent) — not an eviction.
    /// * A new word under the ceiling gets a fresh, never-before-used index —
    ///   not an eviction.
    /// * A new word past the ceiling evicts the least-frequent learned entry
    ///   (ties broken by smallest index) and reuses its freed index — an
    ///   eviction, so the second element is `Some` of that same index.
    pub fn intern(&mut self, word: &str) -> (usize, Option<usize>) {
        // A zero ceiling means "learn nothing": short-circuit here so
        // `evict_least_frequent` is only ever called once at least one
        // learned entry is guaranteed to exist (see its doc comment).
        if !is_learnable(word) || self.learned_ceiling == 0 {
            return (UNK, None);
        }
        if let Some(&(index, freq)) = self.by_word.get(word) {
            self.by_word
                .insert(word.to_owned(), (index, freq.saturating_add(1)));
            return (index, None);
        }
        let (index, evicted) = if self.by_word.len() >= self.learned_ceiling {
            let index = self.evict_least_frequent();
            (index, Some(index))
        } else {
            (FIRST_LEARNED_INDEX + self.by_word.len(), None)
        };
        self.by_word.insert(word.to_owned(), (index, 1));
        self.by_index.insert(index, word.to_owned());
        (index, evicted)
    }

    /// All learned entries as `(word, index, freq)`, in ascending word order
    /// (mirrors `by_word`'s `BTreeMap` iteration) — used by `persist`'s
    /// `vocab_codec` to encode a deterministic blob.
    pub(crate) fn entries(&self) -> impl Iterator<Item = (&str, usize, u32)> {
        self.by_word
            .iter()
            .map(|(word, &(index, freq))| (word.as_str(), index, freq))
    }

    /// Rebuild a `Vocab` (the real ceiling, [`MAX_VOCAB`]) from previously
    /// persisted `(word, index, freq)` entries. Used by `persist::load`.
    ///
    /// Rejects (`None`) rather than silently building an inconsistent
    /// `by_word`/`by_index` pair: an index outside the valid learned range
    /// (`< FIRST_LEARNED_INDEX` or `>= FIRST_LEARNED_INDEX + MAX_VOCAB`), or a
    /// duplicate word or index. The caller (`persist::load`) treats `None` as
    /// a corrupt blob and falls back to cold-start.
    pub(crate) fn from_entries(
        entries: impl IntoIterator<Item = (String, usize, u32)>,
    ) -> Option<Self> {
        let mut vocab = Self::new();
        for (word, index, freq) in entries {
            if !(FIRST_LEARNED_INDEX..FIRST_LEARNED_INDEX + MAX_VOCAB).contains(&index) {
                return None;
            }
            if vocab.by_word.contains_key(&word) || vocab.by_index.contains_key(&index) {
                return None;
            }
            vocab.by_word.insert(word.clone(), (index, freq));
            vocab.by_index.insert(index, word);
        }
        Some(vocab)
    }

    /// Remove the least-frequent learned entry (ties -> smallest index) and
    /// return its now-free index.
    ///
    /// Precondition, upheld by `intern`: only called when
    /// `by_word.len() >= self.learned_ceiling` and `learned_ceiling >= 1`
    /// (`intern` short-circuits `learned_ceiling == 0` before ever reaching
    /// this call), so `by_word` always has at least one entry here. The
    /// `UNK` default below is therefore unreachable in practice; it exists
    /// only so this stays panic-free if that invariant is ever violated.
    fn evict_least_frequent(&mut self) -> usize {
        let victim = self
            .by_word
            .iter()
            .map(|(word, &(index, freq))| (freq, index, word.clone()))
            .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        victim.map_or(UNK, |(_, index, word)| {
            self.by_word.remove(&word);
            self.by_index.remove(&index);
            index
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn intern_assigns_stable_indices_and_is_idempotent() {
        let mut v = Vocab::new();
        let (a, evicted) = v.intern("cat");
        assert_eq!(evicted, None); // a fresh index is not an eviction
        assert_eq!((a, None), v.intern("cat")); // idempotent, still not an eviction
        assert!(a >= 2); // past reserved
        assert_ne!(a, v.intern("dog").0);
    }

    #[test]
    fn oov_maps_to_unk_and_bos_pads() {
        let v = Vocab::new();
        assert_eq!(v.index_of("never-seen"), 0); // UNK
    }

    #[test]
    fn sub_two_char_and_separator_tokens_are_never_interned() {
        let mut v = Vocab::new();
        assert_eq!(v.intern("a"), (0, None)); // too short -> UNK, not registered
        assert_eq!(v.intern("bad\ttok"), (0, None)); // separator -> UNK
    }

    #[test]
    fn eviction_removes_least_frequent_deterministically() {
        let mut v = Vocab::with_capacity_for_test(2); // ceiling = 2 learned
        let (rare, _) = v.intern("aaa");
        let (common, _) = v.intern("bbb");
        v.intern("bbb"); // bump freq
        let (evicted, freed) = v.intern("ccc"); // must evict least-frequent "aaa"
        assert_eq!(v.index_of("aaa"), 0); // gone -> UNK
        assert_eq!(evicted, rare); // reused the freed index
        assert_eq!(freed, Some(rare)); // and reported it as an eviction
        assert!(v.index_of("bbb") >= 2 && v.index_of("ccc") >= 2);
        let _ = common;
    }

    #[test]
    fn a_fresh_assignment_under_the_ceiling_is_not_reported_as_an_eviction() {
        let mut v = Vocab::with_capacity_for_test(2);
        let (_, freed) = v.intern("aaa"); // first of 2 -> room to spare
        assert_eq!(freed, None);
    }

    #[test]
    fn reserved_indices_never_resolve_to_a_word() {
        let mut v = Vocab::new();
        v.intern("cat");
        assert_eq!(v.word_of(UNK), None);
        assert_eq!(v.word_of(BOS), None);
    }

    #[test]
    fn a_zero_ceiling_registers_nothing_and_always_returns_unk() {
        let mut v = Vocab::with_capacity_for_test(0);
        assert_eq!(v.intern("cat"), (0, None)); // UNK, never registered
        assert!(v.is_empty());
        assert_eq!(v.index_of("cat"), 0);
    }

    #[test]
    fn len_tracks_learned_entries_and_default_matches_new() {
        let mut v = Vocab::default();
        assert_eq!(v.len(), 0);
        v.intern("cat");
        v.intern("dog");
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn word_of_resolves_a_learned_index_back_to_its_word() {
        let mut v = Vocab::new();
        let (idx, _) = v.intern("cat");
        assert_eq!(v.word_of(idx), Some("cat"));
    }
}
