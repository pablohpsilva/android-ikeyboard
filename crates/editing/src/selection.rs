//! Word selection — the range a "select word" gesture (double-tap) highlights.

use unicode_segmentation::UnicodeSegmentation;

use crate::error::{validate, EditError};

/// The `[start, end)` byte range of the word at `idx`.
///
/// If `idx` lands inside a word, that word's bounds are returned. If `idx` sits
/// exactly at a word's trailing edge (the caret just after a word), that word is
/// still selected. If `idx` falls in the whitespace or punctuation *between*
/// words, the empty range `(idx, idx)` is returned — there is nothing to select
/// there. Words are Unicode words (`unicode-segmentation`), so the result is
/// correct across combining marks and multi-byte scalars (BR-49).
///
/// # Errors
/// [`EditError`] if `idx` is out of range or not on a char boundary.
pub fn select_word(text: &str, idx: usize) -> Result<(usize, usize), EditError> {
    validate(text, idx)?;

    // A word strictly containing the caret takes priority.
    for (start, word) in text.unicode_word_indices() {
        let end = start + word.len();
        if start <= idx && idx < end {
            return Ok((start, end));
        }
    }
    // Otherwise the caret may rest on a word's trailing edge.
    for (start, word) in text.unicode_word_indices() {
        let end = start + word.len();
        if idx == end {
            return Ok((start, end));
        }
    }
    // The caret is in inter-word space: nothing to select.
    Ok((idx, idx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_word_the_caret_is_inside() {
        let text = "hello world";
        assert_eq!(select_word(text, 2), Ok((0, 5)));
        assert_eq!(select_word(text, 8), Ok((6, 11)));
    }

    #[test]
    fn selects_the_word_at_its_leading_edge() {
        let text = "hello world";
        assert_eq!(select_word(text, 0), Ok((0, 5)));
        assert_eq!(select_word(text, 6), Ok((6, 11)));
    }

    #[test]
    fn selects_the_word_at_its_trailing_edge() {
        // Caret just after "hello" (index 5) still selects "hello".
        let text = "hello world";
        assert_eq!(select_word(text, 5), Ok((0, 5)));
        // Caret at end of text after "world".
        assert_eq!(select_word(text, 11), Ok((6, 11)));
    }

    #[test]
    fn a_caret_in_inter_word_space_selects_nothing() {
        // "a  b" — index 2 is the second space, between both words.
        let text = "a  b";
        assert_eq!(select_word(text, 2), Ok((2, 2)));
    }

    #[test]
    fn selects_a_word_containing_a_combining_mark() {
        // "cafe\u{301}" is one 6-byte word; a caret anywhere in it selects it all.
        let text = "cafe\u{301}";
        assert_eq!(select_word(text, 2), Ok((0, 6)));
        assert_eq!(select_word(text, 6), Ok((0, 6)));
    }

    #[test]
    fn selects_a_multibyte_word_among_others() {
        // "go café now": café spans bytes 3..8 (5 bytes).
        let text = "go café now";
        assert_eq!(select_word(text, 5), Ok((3, 8)));
    }

    #[test]
    fn out_of_range_index_is_an_error() {
        assert_eq!(select_word("hi", 3), Err(EditError::OutOfBounds));
    }

    #[test]
    fn mid_scalar_index_is_an_error() {
        assert_eq!(select_word("café", 4), Err(EditError::NotCharBoundary));
    }
}
