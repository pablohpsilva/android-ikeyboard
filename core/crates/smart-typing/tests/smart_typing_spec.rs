//! Executable BDD spec for smart-typing (BR-48).
//!
//! The Gherkin form of these scenarios lives in
//! `features/smart-typing.feature`; each test here binds to one scenario,
//! exercising the public rule functions end-to-end the way `editing` /
//! `ime-service` would call them on the commit path.

use featherkey_smart_typing::{
    auto_capitalize, curl_quote, double_space_period, smart_quote, TypingError,
};

// @BR-48 — Auto-capitalization at a sentence start.
#[test]
fn caret_at_start_of_field_capitalizes() {
    assert!(auto_capitalize(""));
}

#[test]
fn caret_after_a_period_and_space_capitalizes() {
    assert!(auto_capitalize("Hello world. "));
}

#[test]
fn caret_in_the_middle_of_a_word_does_not_capitalize() {
    assert!(!auto_capitalize("Hello wor"));
}

// @BR-48 — Double space becomes ". ".
#[test]
fn a_second_space_after_a_word_yields_a_period_space() {
    assert_eq!(double_space_period("hello ", ' '), Some(". ".to_string()));
}

#[test]
fn a_single_space_between_words_is_left_alone() {
    assert_eq!(double_space_period("hello", ' '), None);
}

// @BR-48 — Straight quotes are curled by context.
#[test]
fn a_quote_at_the_start_opens() {
    assert_eq!(smart_quote("", '"'), '\u{201C}');
}

#[test]
fn a_quote_after_a_letter_closes() {
    assert_eq!(smart_quote("bye", '"'), '\u{201D}');
}

#[test]
fn an_apostrophe_inside_a_word_is_a_closing_curl() {
    assert_eq!(smart_quote("don", '\''), '\u{2019}');
}

// @BR-48 — The checked variant reports misuse as a value, never a panic.
#[test]
fn curling_a_non_quote_is_an_error() {
    assert_eq!(curl_quote("abc", 'x'), Err(TypingError::NotAQuote));
}
