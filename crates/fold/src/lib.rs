//! Match-folding: lowercase, strip diacritics (NFD + drop combining marks), drop
//! apostrophes. Pure and deterministic — the Rust twin of the Kotlin `Diacritics`
//! object, so the same base input matches the same dictionary word on both sides
//! of the FFI. No persistence, no I/O.
//!
//! Implemented in task W1a of the Tier-1 plan
//! (`docs/superpowers/plans/2026-07-26-tier1-smarter-learning-plan.md`).
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

/// [s] as a bare match key: lowercased, combining diacritics removed (é→e,
/// ç→c), apostrophes dropped (I'm→im, don't→dont).
#[must_use]
pub fn fold(s: &str) -> String {
    if s.bytes().all(|b| b.is_ascii_lowercase()) {
        return s.to_owned(); // fast path: already a bare key
    }
    s.nfd()
        .filter(|&c| !is_combining_mark(c) && c != '\'' && c != '\u{2019}')
        .flat_map(char::to_lowercase)
        .collect()
}

/// A single character folded to its base lowercase letter (É→e, ç→c).
/// Unlike [`fold`], this does not strip apostrophes (matches Kotlin `foldChar`).
#[must_use]
pub fn fold_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    // First non-combining char of the NFD decomposition, lowercased.
    c.nfd()
        .find(|&d| !is_combining_mark(d))
        .unwrap_or(c)
        .to_lowercase()
        .next()
        .unwrap_or(c)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{fold, fold_char};
    use proptest::prelude::*;
    use unicode_normalization::char::is_combining_mark;

    #[test]
    fn fold_matches_kotlin_diacritics() {
        assert_eq!(fold("também"), "tambem");
        assert_eq!(fold("café"), "cafe");
        assert_eq!(fold("I'm"), "im");
        assert_eq!(fold("don't"), "dont");
        assert_eq!(fold("don’t"), "dont"); // curly apostrophe too
        assert_eq!(fold("HELLO"), "hello");
        assert_eq!(fold("hello"), "hello"); // plain ascii unchanged
        assert_eq!(fold(""), "");
    }

    #[test]
    fn fold_char_folds_accents_but_keeps_apostrophe_semantics() {
        assert_eq!(fold_char('É'), 'e');
        assert_eq!(fold_char('ç'), 'c');
        assert_eq!(fold_char('A'), 'a');
        assert_eq!(fold_char('\''), '\''); // fold_char does NOT strip apostrophes (matches Kotlin)
    }

    proptest! {
        /// For any input, the folded key carries no apostrophe and no leftover
        /// `Mn` combining marks, and is lowercase-stable — every retained char
        /// has already been run through `to_lowercase`, so lowercasing again is a
        /// no-op. (Some chars, e.g. U+1D400 `𝐀`, keep the Uppercase property while
        /// having no lowercase mapping; both this fold and the Kotlin twin leave
        /// them unchanged, so idempotence — not `!is_uppercase` — is the invariant.)
        #[test]
        fn fold_output_is_a_bare_key(s in ".{0,32}") {
            let folded = fold(&s);
            prop_assert_eq!(&folded, &folded.to_lowercase(), "fold output is not lowercase-stable");
            for c in folded.chars() {
                prop_assert!(c != '\'' && c != '\u{2019}', "unexpected apostrophe {c:?}");
                prop_assert!(!is_combining_mark(c), "unexpected combining mark {c:?}");
            }
        }
    }
}
