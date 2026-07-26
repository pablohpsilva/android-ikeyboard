//! Edit-distance-1 candidate generation for [`crate::Dictionary::fuzzy`].
//!
//! A pure, allocation-only helper (no I/O, no panics): given a query and the
//! dictionary's alphabet, it enumerates every string one edit away using the
//! four classic single-character edits (Norvig's `edits1`): deletion,
//! adjacent transposition, substitution, and insertion. The caller filters the
//! candidates through the FST, so this module never needs to know what a real
//! word is — it only proposes.

/// Every string exactly one edit from `word`, drawing new characters from
/// `alphabet` (the distinct characters present in the dictionary).
///
/// Duplicates are possible across edit classes; the caller de-duplicates while
/// filtering, so this stays a straight concatenation for clarity. Because any
/// dictionary word's characters are all in `alphabet`, the substitution and
/// insertion classes are complete: every distance-1 *dictionary* word is
/// reachable from here.
pub(crate) fn edits1(word: &str, alphabet: &[char]) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut out = deletes(&chars);
    out.extend(transposes(&chars));
    out.extend(substitutions(&chars, alphabet));
    out.extend(insertions(&chars, alphabet));
    out
}

/// Remove one character at each position.
fn deletes(chars: &[char]) -> Vec<String> {
    (0..chars.len())
        .map(|i| {
            let mut c = chars.to_vec();
            c.remove(i);
            c.into_iter().collect()
        })
        .collect()
}

/// Swap each adjacent pair of characters.
fn transposes(chars: &[char]) -> Vec<String> {
    (0..chars.len().saturating_sub(1))
        .map(|i| {
            let mut c = chars.to_vec();
            c.swap(i, i + 1);
            c.into_iter().collect()
        })
        .collect()
}

/// Replace each character with every *other* character in the alphabet.
fn substitutions(chars: &[char], alphabet: &[char]) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..chars.len() {
        for &a in alphabet {
            if a != chars[i] {
                let mut c = chars.to_vec();
                c[i] = a;
                out.push(c.into_iter().collect());
            }
        }
    }
    out
}

/// Insert every alphabet character at every gap (including both ends).
fn insertions(chars: &[char], alphabet: &[char]) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..=chars.len() {
        for &a in alphabet {
            let mut c = chars.to_vec();
            c.insert(i, a);
            out.push(c.into_iter().collect());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn deletes_removes_one_char_at_each_position() {
        let got: BTreeSet<String> = deletes(&chars("abc")).into_iter().collect();
        let want: BTreeSet<String> = ["bc", "ac", "ab"].iter().map(|s| s.to_string()).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn deletes_of_empty_is_empty() {
        assert!(deletes(&chars("")).is_empty());
    }

    #[test]
    fn transposes_swaps_adjacent_pairs() {
        let got: BTreeSet<String> = transposes(&chars("abc")).into_iter().collect();
        let want: BTreeSet<String> = ["bac", "acb"].iter().map(|s| s.to_string()).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn transposes_of_single_char_is_empty() {
        // saturating_sub keeps a 1-char (and 0-char) word from underflowing.
        assert!(transposes(&chars("a")).is_empty());
        assert!(transposes(&chars("")).is_empty());
    }

    #[test]
    fn substitutions_replace_with_other_alphabet_chars_only() {
        let got: BTreeSet<String> = substitutions(&chars("a"), &['a', 'b', 'c'])
            .into_iter()
            .collect();
        // 'a'->'a' is skipped (not an edit); only 'b' and 'c' remain.
        let want: BTreeSet<String> = ["b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn insertions_add_a_char_at_every_gap() {
        let got: BTreeSet<String> = insertions(&chars("a"), &['x']).into_iter().collect();
        // Insert 'x' before and after 'a'.
        let want: BTreeSet<String> = ["xa", "ax"].iter().map(|s| s.to_string()).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn insertions_into_empty_yield_single_chars() {
        let got: BTreeSet<String> = insertions(&chars(""), &['a', 'b']).into_iter().collect();
        let want: BTreeSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn edits1_contains_a_known_neighbour_from_each_class() {
        let alphabet = ['a', 'c', 't', 's'];
        let e: BTreeSet<String> = edits1("cat", &alphabet).into_iter().collect();
        assert!(e.contains("ca"), "deletion");
        assert!(e.contains("act"), "transposition");
        assert!(e.contains("cas"), "substitution: cat -> cas");
        assert!(e.contains("cast"), "insertion");
        assert!(e.contains("cats"), "insertion at end");
    }

    #[test]
    fn edits1_never_needs_more_than_one_edit_to_recover_the_word() {
        // Sanity: the query itself is reachable in one step from several of its
        // own neighbours, but edits1 of a word never *is* the word (each element
        // differs by exactly one edit). We assert none equals the input.
        let alphabet = ['a', 'b'];
        for cand in edits1("ab", &alphabet) {
            assert_ne!(cand, "ab");
        }
    }

    #[test]
    fn edits1_of_empty_is_the_alphabet() {
        let got: BTreeSet<String> = edits1("", &['a', 'b']).into_iter().collect();
        let want: BTreeSet<String> = vec!["a".to_string(), "b".to_string()].into_iter().collect();
        assert_eq!(got, want);
    }
}
