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
use std::sync::Arc;

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
///
/// `Clone` is **O(1)**: the two heavy, immutable structures — the backing FST
/// and the folded index — sit behind [`Arc`], so cloning bumps two reference
/// counts rather than deep-copying the ~12k-entry index and the FST bytes. That
/// matters because the strip blend clones every active pack's dictionary *per
/// keystroke* (`rank_suggestions`); a deep copy there cost milliseconds each.
/// The lexicon stays read-only (no `&mut self` methods), so sharing is sound.
#[derive(Clone)]
pub struct Dictionary {
    set: Arc<Set<Vec<u8>>>,
    /// The distinct characters occurring in the lexicon, sorted. Used as the
    /// substitution/insertion alphabet for fuzzy lookup so candidate generation
    /// stays bounded by the language rather than all of Unicode. Small (one
    /// entry per distinct letter), so it is copied on clone rather than shared.
    alphabet: Vec<char>,
    /// `(folded, original)` pairs sorted by `folded`, mirroring the Kotlin
    /// `Vocabulary` folded/sortedWords arrays. This is the accent-insensitive
    /// index [`fold_prefix`](Dictionary::fold_prefix) binary-searches: a bare
    /// match key (`fold("café") == "cafe"`) paired back to the real spelling.
    /// Behind an [`Arc`] so a per-keystroke clone shares it instead of copying
    /// every entry.
    folded: Arc<Vec<(String, String)>>,
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
        let mut folded = Vec::new();
        for word in words {
            let word = word.as_ref();
            // `insert` is the sole failure point: it rejects any key that is
            // less than the last (going backwards), which is exactly the
            // sorted-order violation we surface. Equal keys are accepted.
            builder
                .insert(word)
                .map_err(|_| DictionaryError::Unsorted)?;
            alphabet.extend(word.chars());
            folded.push((featherkey_fold::fold(word), word.to_owned()));
        }
        // The folded key order differs from the FST's byte order (accents fold
        // away, uppercase lowercases), so sort the index by its own key. Sort by
        // `(folded, original)` for a deterministic order among words that share a
        // folded form (e.g. `re`/`ré`).
        folded.sort();
        // `into_set` on the in-memory builder is infallible (it hands back the
        // bytes it just wrote), so there is no second error path to leak.
        Ok(Self {
            set: Arc::new(builder.into_set()),
            alphabet: alphabet.into_iter().collect(),
            folded: Arc::new(folded),
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

    /// Original spellings whose **match-folded** form (lowercased, diacritics
    /// and apostrophes stripped — see [`featherkey_fold::fold`]) begins with
    /// `folded_prefix`, capped at [`MAX_COMPLETIONS`].
    ///
    /// This is the accent-insensitive companion to [`prefix`](Dictionary::prefix):
    /// `fold_prefix("cafe")` surfaces `"café"`, `fold_prefix("dont")` surfaces
    /// `"don't"`. `folded_prefix` is expected to be already folded by the caller
    /// (the prediction layer folds the user's keystrokes once); it is compared
    /// as-is against the stored folded keys. An empty prefix matches every word
    /// (still capped). Results are ordered by `(folded, original)`.
    #[must_use]
    pub fn fold_prefix(&self, folded_prefix: &str) -> Vec<String> {
        // The index is sorted by folded key, so all matches form one contiguous
        // run. Binary-search its lower bound (mirrors the Kotlin `lowerBound`),
        // then walk forward while the folded key still starts with the prefix.
        let start = self
            .folded
            .partition_point(|(folded, _)| folded.as_str() < folded_prefix);
        self.folded[start..]
            .iter()
            .take_while(|(folded, _)| folded.starts_with(folded_prefix))
            .take(MAX_COMPLETIONS)
            .map(|(_, original)| original.clone())
            .collect()
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
    fn fold_prefix_surfaces_accented_and_apostrophe_words() {
        // Real spellings in; base-letter prefix out. The fixture is inserted in
        // sorted BYTE order (the FST's set contract): uppercase `I` (0x49) sorts
        // before the lowercase words, and inside `he'll`/`hello` the apostrophe
        // (0x27) sorts before `l` (0x6c) so `he'll` precedes `hello`.
        let d = Dictionary::from_sorted_words(
            ["I've", "café", "don't", "he'll", "hello", "também", "você"].iter(),
        )
        .expect("fixture is sorted");
        assert!(d.fold_prefix("ive").contains(&"I've".to_string()));
        assert!(d.fold_prefix("cafe").contains(&"café".to_string()));
        assert!(d.fold_prefix("hell").contains(&"he'll".to_string()));
        assert!(d.fold_prefix("dont").contains(&"don't".to_string()));
        assert!(d.fold_prefix("tambe").contains(&"também".to_string()));
        assert!(d.fold_prefix("voce").contains(&"você".to_string()));
    }

    #[test]
    fn debug_reports_alphabet_size_without_dumping_words() {
        let d = dict(&["ab", "cd"]);
        // Distinct chars a,b,c,d => 4.
        assert_eq!(format!("{d:?}"), "Dictionary { alphabet_len: 4, .. }");
    }
}
