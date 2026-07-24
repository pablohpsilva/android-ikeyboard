//! Compact per-language lexicon with prefix and fuzzy lookup.
//!
//! A [`Dictionary`] is a read-only, memory-compact word set backed by a
//! finite-state transducer (the `fst` crate). It answers three questions and
//! nothing more (SEDD §5.2 single responsibility):
//!
//! * [`contains`](Dictionary::contains) — is this exactly a word?
//! * [`prefix`](Dictionary::prefix) — which words start with these letters?
//! * [`fuzzy`](Dictionary::fuzzy) — which words are one edit away?
//!
//! It carries **no policy**: it does not rank, learn, or decide whether to
//! autocorrect. Those belong to the `prediction` and `autocorrect` crates
//! downstream (BR-10, BR-12); this crate is the pure lexical substrate they
//! read. Errors are values, never panics on the lookup path (SEDD §5.5 r3):
//! construction returns a [`Result`], and every query returns plain data.

use std::collections::BTreeSet;
use std::fmt;

use fst::automaton::Str;
use fst::{Automaton, IntoStreamer, Set, SetBuilder, Streamer};

mod fuzzy;

/// The most completions [`Dictionary::prefix`] will return for one query.
///
/// A cap keeps the hot path bounded regardless of how many words share a short
/// prefix (e.g. `"s"` in English): the UI can only surface a handful of
/// suggestions, so streaming the whole subtree would be wasted work.
pub const MAX_COMPLETIONS: usize = 16;

/// Why a [`Dictionary`] could not be built.
///
/// Lookup never produces an error — only construction can, and only for input
/// that violates the FST's sorted-set contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DictionaryError {
    /// A word was lexicographically smaller than the one before it. An FST is
    /// an ordered set, so callers must supply words in non-decreasing byte
    /// order. Adjacent duplicates are tolerated (the set collapses them); only
    /// going *backwards* is an error.
    Unsorted,
}

impl fmt::Display for DictionaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DictionaryError::Unsorted => {
                f.write_str("word list must be in non-decreasing (sorted) order")
            }
        }
    }
}

/// A compact, read-only lexicon for a single language.
pub struct Dictionary {
    set: Set<Vec<u8>>,
    /// The distinct characters occurring in the lexicon, sorted. Used as the
    /// substitution/insertion alphabet for fuzzy lookup so candidate generation
    /// stays bounded by the language rather than all of Unicode.
    alphabet: Vec<char>,
}

impl Dictionary {
    /// Build a dictionary from a **sorted** word list.
    ///
    /// The words must be yielded in non-decreasing lexicographic (byte) order —
    /// real lexicon files already are. Adjacent duplicates are allowed and
    /// merged. This mirrors the FST's set contract exactly and lets
    /// construction stay allocation-cheap.
    ///
    /// # Errors
    /// Returns [`DictionaryError::Unsorted`] if any word is lexicographically
    /// smaller than the one before it.
    pub fn from_sorted_words<I, S>(words: I) -> Result<Self, DictionaryError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut builder = SetBuilder::memory();
        let mut alphabet = BTreeSet::new();
        for word in words {
            let word = word.as_ref();
            // `insert` is the sole failure point: it rejects any key that is
            // less than the last (going backwards), which is exactly the
            // sorted-order violation we surface. Equal keys are accepted.
            builder
                .insert(word)
                .map_err(|_| DictionaryError::Unsorted)?;
            alphabet.extend(word.chars());
        }
        // `into_set` on the in-memory builder is infallible (it hands back the
        // bytes it just wrote), so there is no second error path to leak.
        Ok(Self {
            set: builder.into_set(),
            alphabet: alphabet.into_iter().collect(),
        })
    }

    /// `true` if `word` is exactly present in the lexicon.
    #[must_use]
    pub fn contains(&self, word: &str) -> bool {
        self.set.contains(word)
    }

    /// Words that begin with `prefix`, in lexicographic order, capped at
    /// [`MAX_COMPLETIONS`].
    ///
    /// An empty prefix matches every word (still capped). The prefix itself is
    /// included when it is also a complete word.
    #[must_use]
    pub fn prefix(&self, prefix: &str) -> Vec<String> {
        let matcher = Str::new(prefix).starts_with();
        let mut stream = self.set.search(&matcher).into_stream();
        let mut out = Vec::new();
        while let Some(key) = stream.next() {
            if out.len() >= MAX_COMPLETIONS {
                break;
            }
            // Keys are the bytes we inserted from `&str`, so they are valid
            // UTF-8; `from_utf8_lossy` is exact here and spares us an
            // unreachable error branch on the hot path.
            out.push(String::from_utf8_lossy(key).into_owned());
        }
        out
    }

    /// Dictionary words exactly one edit (delete, transpose, substitute, or
    /// insert) away from `word`, in lexicographic order, de-duplicated.
    ///
    /// The exact word is excluded even if present — callers asking for fuzzy
    /// matches already know whether the word itself exists via
    /// [`contains`](Dictionary::contains). This is pure lookup: no ranking, no
    /// autocorrect policy (BR-12 lives in the `autocorrect` crate).
    #[must_use]
    pub fn fuzzy(&self, word: &str) -> Vec<String> {
        let mut matches = BTreeSet::new();
        for candidate in fuzzy::edits1(word, &self.alphabet) {
            if candidate != word && self.set.contains(&candidate) {
                matches.insert(candidate);
            }
        }
        matches.into_iter().collect()
    }
}

impl fmt::Debug for Dictionary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The FST has no cheap len(); report the alphabet size, which is the
        // useful, bounded fact about a lexicon and avoids dumping every word.
        f.debug_struct("Dictionary")
            .field("alphabet_len", &self.alphabet.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(words: &[&str]) -> Dictionary {
        // Test fixtures are written pre-sorted; `expect` is confined to tests,
        // never library code.
        Dictionary::from_sorted_words(words.iter().copied()).expect("fixture is sorted")
    }

    #[test]
    fn contains_finds_exact_words_and_rejects_others() {
        let d = dict(&["apple", "apply", "apt"]);
        assert!(d.contains("apple"));
        assert!(d.contains("apt"));
        assert!(!d.contains("app"));
        assert!(!d.contains("banana"));
        assert!(!d.contains(""));
    }

    #[test]
    fn from_sorted_words_rejects_unsorted_input() {
        let err = Dictionary::from_sorted_words(["b", "a"]);
        assert_eq!(err.err(), Some(DictionaryError::Unsorted));
    }

    #[test]
    fn from_sorted_words_tolerates_adjacent_duplicates() {
        // The FST is a set: repeated equal keys collapse rather than erroring.
        let d = Dictionary::from_sorted_words(["a", "a", "b"]).expect("non-decreasing");
        assert!(d.contains("a"));
        assert!(d.contains("b"));
    }

    #[test]
    fn empty_dictionary_is_valid_and_finds_nothing() {
        let d = dict(&[]);
        assert!(!d.contains("a"));
        assert!(d.prefix("a").is_empty());
        assert!(d.fuzzy("a").is_empty());
    }

    #[test]
    fn prefix_returns_completions_in_order() {
        let d = dict(&["apple", "apply", "apt", "banana"]);
        assert_eq!(d.prefix("app"), ["apple", "apply"]);
        assert_eq!(d.prefix("ba"), ["banana"]);
    }

    #[test]
    fn prefix_includes_the_prefix_when_it_is_a_word() {
        let d = dict(&["an", "and", "ant"]);
        assert_eq!(d.prefix("an"), ["an", "and", "ant"]);
    }

    #[test]
    fn prefix_with_no_matches_is_empty() {
        let d = dict(&["cat", "dog"]);
        assert!(d.prefix("z").is_empty());
    }

    #[test]
    fn empty_prefix_matches_every_word() {
        let d = dict(&["a", "b", "c"]);
        assert_eq!(d.prefix(""), ["a", "b", "c"]);
    }

    #[test]
    fn prefix_is_capped_at_max_completions() {
        // Build MAX_COMPLETIONS + 5 words sharing the prefix "x", pre-sorted.
        let owned: Vec<String> = (0..MAX_COMPLETIONS + 5)
            .map(|i| format!("x{i:03}"))
            .collect();
        let d = Dictionary::from_sorted_words(owned.iter()).expect("generated in order");
        let got = d.prefix("x");
        assert_eq!(got.len(), MAX_COMPLETIONS);
        // The cap takes a prefix of the ordered results, not a random subset.
        assert_eq!(got[0], "x000");
    }

    #[test]
    fn fuzzy_finds_each_edit_class() {
        // Sorted by bytes: "cast" < "cat" because 's' < 't'.
        let d = dict(&["at", "cast", "cat", "cats", "cot", "dog"]);
        let got = d.fuzzy("cat");
        // deletion -> "at", substitution -> "cot", insertion -> "cast"/"cats".
        assert!(got.contains(&"at".to_string()));
        assert!(got.contains(&"cot".to_string()));
        assert!(got.contains(&"cast".to_string()));
        assert!(got.contains(&"cats".to_string()));
        // "dog" is far away and must not appear.
        assert!(!got.contains(&"dog".to_string()));
    }

    #[test]
    fn fuzzy_finds_a_transposition() {
        let d = dict(&["act", "cat"]);
        assert_eq!(d.fuzzy("cat"), ["act"]);
    }

    #[test]
    fn fuzzy_excludes_the_exact_word_and_is_sorted_unique() {
        let d = dict(&["bat", "cat", "cot", "hat"]);
        let got = d.fuzzy("cat");
        assert!(!got.contains(&"cat".to_string()), "exact word excluded");
        // Sorted and de-duplicated.
        let mut sorted = got.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(got, sorted);
        assert_eq!(got, ["bat", "cot", "hat"]);
    }

    #[test]
    fn fuzzy_never_returns_the_query_even_when_a_transposition_reproduces_it() {
        // Transposing the two identical letters of "ee" yields "ee" again; the
        // `candidate != word` guard must drop it rather than offer the query as
        // its own fuzzy match.
        let d = dict(&["ee", "eve"]);
        let got = d.fuzzy("ee");
        assert!(!got.contains(&"ee".to_string()));
    }

    #[test]
    fn fuzzy_with_no_neighbours_is_empty() {
        let d = dict(&["cat"]);
        assert!(d.fuzzy("zzzz").is_empty());
    }

    #[test]
    fn dictionary_error_displays_a_human_message() {
        assert_eq!(
            format!("{}", DictionaryError::Unsorted),
            "word list must be in non-decreasing (sorted) order"
        );
    }

    #[test]
    fn debug_reports_alphabet_size_without_dumping_words() {
        let d = dict(&["ab", "cd"]);
        // Distinct chars a,b,c,d => 4.
        assert_eq!(format!("{d:?}"), "Dictionary { alphabet_len: 4, .. }");
    }
}
