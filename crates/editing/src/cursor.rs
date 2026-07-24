//! Grapheme- and word-granular cursor movement.
//!
//! Every function takes a `text` and a byte offset `idx` and returns the byte
//! offset the caret should occupy after the movement. Movement is measured in
//! **extended grapheme clusters** (`unicode-segmentation`), never in bytes or
//! `char`s: one press of the left arrow steps over a whole emoji or a base
//! letter plus its combining marks, not a single scalar (BR-49).
//!
//! Movement saturates at the ends of the buffer — moving left from `0` yields
//! `0`, moving right from `text.len()` yields `text.len()` — rather than
//! erroring, so held-arrow behaviour needs no special-casing at the edges.

use unicode_segmentation::UnicodeSegmentation;

use crate::error::{validate, EditError};

/// Move the caret one grapheme cluster to the left, saturating at `0`.
///
/// # Errors
/// [`EditError`] if `idx` is out of range or not on a char boundary.
pub fn move_left(text: &str, idx: usize) -> Result<usize, EditError> {
    validate(text, idx)?;
    let prev = text
        .grapheme_indices(true)
        .map(|(start, _)| start)
        .take_while(|&start| start < idx)
        .last();
    Ok(prev.unwrap_or(0))
}

/// Move the caret one grapheme cluster to the right, saturating at `text.len()`.
///
/// # Errors
/// [`EditError`] if `idx` is out of range or not on a char boundary.
pub fn move_right(text: &str, idx: usize) -> Result<usize, EditError> {
    validate(text, idx)?;
    let next = text
        .grapheme_indices(true)
        .map(|(start, _)| start)
        .find(|&start| start > idx);
    Ok(next.unwrap_or(text.len()))
}

/// Move the caret to the start of the previous word, saturating at `0`.
///
/// Words are Unicode words (`unicode-segmentation`), so punctuation and
/// whitespace between words are skipped. From inside a word the caret lands on
/// that word's start; from a word's start it lands on the previous word's start.
///
/// # Errors
/// [`EditError`] if `idx` is out of range or not on a char boundary.
pub fn word_left(text: &str, idx: usize) -> Result<usize, EditError> {
    validate(text, idx)?;
    let start = text
        .unicode_word_indices()
        .map(|(start, _)| start)
        .take_while(|&start| start < idx)
        .last();
    Ok(start.unwrap_or(0))
}

/// Move the caret to the end of the next word, saturating at `text.len()`.
///
/// From inside a word the caret lands on that word's end; from a word's end it
/// lands on the following word's end, skipping the separators between.
///
/// # Errors
/// [`EditError`] if `idx` is out of range or not on a char boundary.
pub fn word_right(text: &str, idx: usize) -> Result<usize, EditError> {
    validate(text, idx)?;
    let end = text
        .unicode_word_indices()
        .map(|(start, word)| start + word.len())
        .find(|&end| end > idx);
    Ok(end.unwrap_or(text.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_right_steps_over_ascii() {
        assert_eq!(move_right("abc", 0), Ok(1));
        assert_eq!(move_right("abc", 1), Ok(2));
    }

    #[test]
    fn move_right_saturates_at_the_end() {
        assert_eq!(move_right("abc", 3), Ok(3));
    }

    #[test]
    fn move_left_saturates_at_the_start() {
        assert_eq!(move_left("abc", 0), Ok(0));
    }

    #[test]
    fn move_left_steps_back_one_grapheme() {
        assert_eq!(move_left("abc", 2), Ok(1));
    }

    #[test]
    fn move_right_treats_a_precomposed_accent_as_one_step() {
        // "café" — 'é' is a single 2-byte scalar spanning bytes 3..5.
        let text = "café";
        assert_eq!(move_right(text, 3), Ok(5));
        assert_eq!(move_left(text, 5), Ok(3));
    }

    #[test]
    fn move_right_steps_over_a_base_letter_plus_combining_mark() {
        // "cafe\u{301}" — 'e' + COMBINING ACUTE ACCENT is one grapheme cluster
        // occupying bytes 3..6, even though it is two scalars.
        let text = "cafe\u{301}";
        assert_eq!(text.len(), 6);
        assert_eq!(move_right(text, 3), Ok(6));
        assert_eq!(move_left(text, 6), Ok(3));
    }

    #[test]
    fn move_right_steps_over_a_multi_scalar_emoji_cluster() {
        // Family emoji: three people joined by zero-width joiners — many scalars,
        // one grapheme. A grapheme-naive walk would stop mid-cluster.
        let text = "👨‍👩‍👧!";
        let cluster_len = text.len() - 1; // trailing '!' is one byte
        assert_eq!(move_right(text, 0), Ok(cluster_len));
        assert_eq!(move_left(text, cluster_len), Ok(0));
    }

    #[test]
    fn word_right_lands_on_word_ends() {
        // "hello world": hello=[0,5), world=[6,11).
        let text = "hello world";
        assert_eq!(word_right(text, 0), Ok(5)); // to end of "hello"
        assert_eq!(word_right(text, 3), Ok(5)); // from mid-word to its end
        assert_eq!(word_right(text, 5), Ok(11)); // skip space to end of "world"
    }

    #[test]
    fn word_right_saturates_at_the_end() {
        assert_eq!(word_right("hello world", 11), Ok(11));
    }

    #[test]
    fn word_left_lands_on_word_starts() {
        let text = "hello world";
        assert_eq!(word_left(text, 11), Ok(6)); // to start of "world"
        assert_eq!(word_left(text, 6), Ok(0)); // to start of "hello"
        assert_eq!(word_left(text, 3), Ok(0)); // from mid-word to its start
    }

    #[test]
    fn word_left_saturates_at_the_start() {
        assert_eq!(word_left("hello world", 0), Ok(0));
    }

    #[test]
    fn word_movement_crosses_multibyte_words() {
        // "café złoty" — both words carry multi-byte scalars.
        let text = "café złoty";
        let cafe_end = "café".len(); // 5
        let zloty_start = cafe_end + 1; // after the space
        assert_eq!(word_right(text, 0), Ok(cafe_end));
        assert_eq!(word_left(text, text.len()), Ok(zloty_start));
    }

    #[test]
    fn every_operation_surfaces_out_of_range_indices_as_errors() {
        let text = "hi";
        assert_eq!(move_left(text, 3), Err(EditError::OutOfBounds));
        assert_eq!(move_right(text, 3), Err(EditError::OutOfBounds));
        assert_eq!(word_left(text, 3), Err(EditError::OutOfBounds));
        assert_eq!(word_right(text, 3), Err(EditError::OutOfBounds));
    }

    #[test]
    fn every_operation_surfaces_mid_scalar_indices_as_errors() {
        let text = "café"; // index 4 splits 'é'
        assert_eq!(move_left(text, 4), Err(EditError::NotCharBoundary));
        assert_eq!(move_right(text, 4), Err(EditError::NotCharBoundary));
        assert_eq!(word_left(text, 4), Err(EditError::NotCharBoundary));
        assert_eq!(word_right(text, 4), Err(EditError::NotCharBoundary));
    }
}
