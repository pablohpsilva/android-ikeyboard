//! BR-49 editing tracer — the caret-control slice a user drives from the
//! keyboard's cursor keys and the double-tap "select word" gesture.
//!
//! This is the executable form of the BDD scenarios in
//! `features/editing.feature`. It exercises the public surface exactly as
//! `ime-service` would: hand it the editor's text and the current selection
//! offset, get back the next offset (or the word range) to send through
//! `InputConnection` (BR-49).

use featherkey_editing::{
    move_left, move_right, select_word, word_left, word_right, EditError,
};

#[test]
fn arrow_keys_walk_a_line_grapheme_by_grapheme() {
    // A line mixing an emoji cluster and an accented word.
    let text = "hi 👋 café";
    // From the start, right-arrow steps h, i, space, then the whole wave emoji.
    let after_h = move_right(text, 0).unwrap();
    let after_i = move_right(text, after_h).unwrap();
    let after_space = move_right(text, after_i).unwrap();
    let after_emoji = move_right(text, after_space).unwrap();
    assert_eq!(&text[after_space..after_emoji], "👋");
    // Left-arrow is the exact inverse of that emoji step.
    assert_eq!(move_left(text, after_emoji).unwrap(), after_space);
}

#[test]
fn word_jumps_move_between_words_and_saturate_at_the_ends() {
    let text = "the quick brown";
    let end_of_the = word_right(text, 0).unwrap();
    assert_eq!(&text[..end_of_the], "the");
    let end_of_quick = word_right(text, end_of_the).unwrap();
    assert_eq!(&text[..end_of_quick], "the quick");
    // Jumping left from the very end lands on the last word's start.
    let start_last = word_left(text, text.len()).unwrap();
    assert_eq!(&text[start_last..], "brown");
    // Saturation: no movement past either edge.
    assert_eq!(word_left(text, 0).unwrap(), 0);
    assert_eq!(word_right(text, text.len()).unwrap(), text.len());
}

#[test]
fn double_tap_selects_the_whole_word_under_the_caret() {
    let text = "edit café now";
    let (start, end) = select_word(text, 7).unwrap(); // caret inside "café"
    assert_eq!(&text[start..end], "café");
}

#[test]
fn a_caret_that_splits_a_scalar_is_rejected_not_panicked() {
    // Index 5 splits the 2-byte 'é' in "café" (bytes 3..5) — must be an error.
    let text = "café";
    assert_eq!(move_right(text, 4), Err(EditError::NotCharBoundary));
    assert_eq!(select_word(text, 99), Err(EditError::OutOfBounds));
}
