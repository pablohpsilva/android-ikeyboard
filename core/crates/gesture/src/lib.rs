//! Swipe/glide decoder: rank the active-language words whose ideal path (the
//! polyline through their letters' key centres) best matches a finger gesture.
//!
//! Pure and coordinate-space-agnostic — pass `path` and `centers` in the same
//! space and it needs no layout knowledge of its own. The Rust port of the Android
//! Kotlin `GestureDecoder`; see the crate README's "Deferred" note (bounded twin).

use std::collections::HashMap;

use featherkey_fold::fold_char;

mod score;
pub use score::{decode, Point};

/// The keys a swipe of `word` passes through: every character folded to its base
/// key (é→e, ç→c, case dropped) and any character with no key on the layout — an
/// apostrophe (I've, don't), a hyphen — simply skipped, because the finger never
/// crosses a key that isn't there. `has_key` answers whether a folded character is
/// a real key. Mirrors Kotlin `GestureDecoder.keyPath`.
#[must_use]
pub fn key_path(word: &str, has_key: impl Fn(char) -> bool) -> Vec<char> {
    let mut out = Vec::with_capacity(word.len());
    for ch in word.chars() {
        let k = fold_char(ch);
        if has_key(k) {
            out.push(k);
        }
    }
    out
}

/// One swipeable word: its first key is the bucket it lives in; `last` is its final
/// typeable key (for the end-of-gesture prune).
#[derive(Debug)]
struct Entry {
    word: String,
    last: char,
}

/// The precomputed, first-key-bucketed candidate set a decode scans — built once per
/// vocabulary and reused by every gesture, so a gesture is pruned to just the words
/// that begin near where the finger started without re-deriving every word's key
/// path per gesture. Mirrors Kotlin `GestureDecoder.Index`.
#[derive(Debug)]
pub struct GestureIndex {
    by_first_key: HashMap<char, Vec<Entry>>,
}

impl GestureIndex {
    /// Bucket `words` by first typeable key. A word's keys are its letters folded to
    /// their base key ('é'→'e') with non-key characters (an apostrophe) dropped,
    /// against a standard a–z letter layout. Words with fewer than two keys can't be
    /// a gesture and are skipped. Mirrors Kotlin `Index.build`.
    #[must_use]
    pub fn build(words: &[&str]) -> Self {
        let mut by_first_key: HashMap<char, Vec<Entry>> = HashMap::new();
        for w in words {
            if w.chars().count() < 2 {
                continue;
            }
            let keys = key_path(w, |c| c.is_ascii_lowercase());
            if keys.len() < 2 {
                continue;
            }
            let (first, last) = (keys[0], keys[keys.len() - 1]);
            by_first_key.entry(first).or_default().push(Entry {
                word: (*w).to_string(),
                last,
            });
        }
        Self { by_first_key }
    }

    /// True when no word is indexed (used until the real vocabulary is loaded).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_first_key.is_empty()
    }

    /// The candidate entries bucketed under `first_key` (empty if none). The decode
    /// hot path scans only the bucket for the gesture's start key.
    pub(crate) fn bucket(&self, first_key: char) -> &[Entry] {
        self.by_first_key
            .get(&first_key)
            .map_or(&[][..], |v| v.as_slice())
    }

    /// The words bucketed under `first_key` (test seam; mirrors `wordsForFirstKey`).
    #[cfg(test)]
    #[must_use]
    pub fn words_for_first(&self, first_key: char) -> Vec<String> {
        self.by_first_key
            .get(&first_key)
            .into_iter()
            .flatten()
            .map(|e| e.word.clone())
            .collect()
    }

    /// The recorded last key for `word`, or `None` if it was skipped (test seam;
    /// mirrors `lastKeyOf`).
    #[cfg(test)]
    #[must_use]
    pub fn last_key_of(&self, word: &str) -> Option<char> {
        self.by_first_key
            .values()
            .flatten()
            .find(|e| e.word == word)
            .map(|e| e.last)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn has_letter_key(c: char) -> bool {
        c.is_ascii_lowercase()
    }

    // --- key_path: path through typeable letters only (ported from Kotlin) ---

    #[test]
    fn apostrophe_words_path_through_their_letters_only() {
        assert_eq!(key_path("I've", has_letter_key), vec!['i', 'v', 'e']);
        assert_eq!(key_path("don't", has_letter_key), vec!['d', 'o', 'n', 't']);
        assert_eq!(key_path("he'll", has_letter_key), vec!['h', 'e', 'l', 'l']);
    }

    #[test]
    fn accented_words_fold_to_their_base_keys() {
        assert_eq!(key_path("café", has_letter_key), vec!['c', 'a', 'f', 'e']);
        assert_eq!(key_path("você", has_letter_key), vec!['v', 'o', 'c', 'e']);
        assert_eq!(
            key_path("também", has_letter_key),
            vec!['t', 'a', 'm', 'b', 'e', 'm']
        );
    }

    #[test]
    fn accents_and_apostrophes_are_dropped_together() {
        assert_eq!(key_path("c'est", has_letter_key), vec!['c', 'e', 's', 't']);
    }

    #[test]
    fn a_trailing_apostrophe_does_not_become_the_last_key() {
        let keys = key_path("goin'", has_letter_key);
        assert_eq!(keys.last(), Some(&'n'));
        assert_eq!(keys, vec!['g', 'o', 'i', 'n']);
    }

    #[test]
    fn a_plain_word_is_unchanged() {
        assert_eq!(
            key_path("hello", has_letter_key),
            vec!['h', 'e', 'l', 'l', 'o']
        );
    }

    // --- Index: first-key-bucketed candidate set (ported from Kotlin) ---

    #[test]
    fn index_buckets_words_by_their_first_key() {
        let idx = GestureIndex::build(&["cat", "car", "dog", "café"]);
        assert_eq!(
            idx.words_for_first('c').into_iter().collect::<HashSet<_>>(),
            ["cat", "car", "café"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        );
        assert_eq!(idx.words_for_first('d'), vec!["dog".to_string()]);
        assert!(idx.words_for_first('z').is_empty());
    }

    #[test]
    fn index_keys_by_folded_first_letter_not_the_raw_character() {
        let idx = GestureIndex::build(&["über"]);
        assert_eq!(idx.words_for_first('u'), vec!["über".to_string()]);
        assert!(idx.words_for_first('ü').is_empty());
    }

    #[test]
    fn index_records_the_last_typeable_key_dropping_a_trailing_apostrophe() {
        let idx = GestureIndex::build(&["goin'"]);
        assert_eq!(idx.last_key_of("goin'"), Some('n'));
    }

    #[test]
    fn index_skips_words_with_fewer_than_two_typeable_keys() {
        let idx = GestureIndex::build(&["a", "I", "hi"]);
        assert!(idx.words_for_first('a').is_empty());
        assert_eq!(idx.words_for_first('h'), vec!["hi".to_string()]);
    }
}
