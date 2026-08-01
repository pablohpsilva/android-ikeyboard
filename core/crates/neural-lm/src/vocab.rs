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

    /// Intern `word`, returning its stable index.
    ///
    /// * A non-learnable token (per [`is_learnable`]) is never registered and
    ///   always returns [`UNK`].
    /// * An already-known word has its frequency bumped and its existing
    ///   index returned (idempotent).
    /// * A new word past the ceiling evicts the least-frequent learned entry
    ///   (ties broken by smallest index), reusing its freed index.
    pub fn intern(&mut self, word: &str) -> usize {
        if !is_learnable(word) {
            return UNK;
        }
        if let Some(&(index, freq)) = self.by_word.get(word) {
            self.by_word
                .insert(word.to_owned(), (index, freq.saturating_add(1)));
            return index;
        }
        let index = if self.by_word.len() >= self.learned_ceiling {
            self.evict_least_frequent()
        } else {
            FIRST_LEARNED_INDEX + self.by_word.len()
        };
        self.by_word.insert(word.to_owned(), (index, 1));
        self.by_index.insert(index, word.to_owned());
        index
    }

    /// Remove the least-frequent learned entry (ties -> smallest index) and
    /// return its now-free index. Only called when at the ceiling, which
    /// implies at least one learned entry exists.
    fn evict_least_frequent(&mut self) -> usize {
        let victim = self
            .by_word
            .iter()
            .map(|(word, &(index, freq))| (freq, index, word.clone()))
            .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let Some((_, index, word)) = victim else {
            // Only reachable with a degenerate zero ceiling (nothing to
            // evict). Errors are values: fall back to the smallest learned
            // index rather than panic.
            return FIRST_LEARNED_INDEX;
        };
        self.by_word.remove(&word);
        self.by_index.remove(&index);
        index
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn intern_assigns_stable_indices_and_is_idempotent() {
        let mut v = Vocab::new();
        let a = v.intern("cat");
        assert_eq!(a, v.intern("cat")); // idempotent
        assert!(a >= 2); // past reserved
        assert_ne!(a, v.intern("dog"));
    }

    #[test]
    fn oov_maps_to_unk_and_bos_pads() {
        let v = Vocab::new();
        assert_eq!(v.index_of("never-seen"), 0); // UNK
    }

    #[test]
    fn sub_two_char_and_separator_tokens_are_never_interned() {
        let mut v = Vocab::new();
        assert_eq!(v.intern("a"), 0); // too short -> UNK, not registered
        assert_eq!(v.intern("bad\ttok"), 0); // separator -> UNK
    }

    #[test]
    fn eviction_removes_least_frequent_deterministically() {
        let mut v = Vocab::with_capacity_for_test(2); // ceiling = 2 learned
        let rare = v.intern("aaa");
        let common = v.intern("bbb");
        v.intern("bbb"); // bump freq
        let evicted = v.intern("ccc"); // must evict least-frequent "aaa"
        assert_eq!(v.index_of("aaa"), 0); // gone -> UNK
        assert_eq!(evicted, rare); // reused the freed index
        assert!(v.index_of("bbb") >= 2 && v.index_of("ccc") >= 2);
        let _ = common;
    }

    #[test]
    fn reserved_indices_never_resolve_to_a_word() {
        let mut v = Vocab::new();
        v.intern("cat");
        assert_eq!(v.word_of(UNK), None);
        assert_eq!(v.word_of(BOS), None);
    }
}
