//! Executable form of features/locale-manager.feature — exercises the public
//! API across the crate boundary (BR-16, BR-17, BR-18, BR-19b).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use featherkey_dictionary::Dictionary;
use featherkey_locale_manager::{LangId, LocaleManager};

fn dict(words: &[&str]) -> Dictionary {
    Dictionary::from_sorted_words(words.iter().copied()).expect("fixture is sorted")
}

/// Background: English then Portuguese both active, "hello" shared by both.
fn en_then_pt() -> LocaleManager {
    LocaleManager::new(vec![
        (LangId::new("en"), dict(&["hello", "world"])),
        (LangId::new("pt"), dict(&["hello", "mundo"])),
    ])
    .expect("valid active set")
}

#[test]
fn br16_two_languages_are_concurrently_active_in_order() {
    let mgr = en_then_pt();
    let active: Vec<&str> = mgr.active().iter().map(LangId::as_str).collect();
    assert_eq!(active, ["en", "pt"]);
}

#[test]
fn br19b_word_in_exactly_one_language_detects_it() {
    let mgr = en_then_pt();
    assert_eq!(mgr.detect("world"), Some(LangId::new("en")));
}

#[test]
fn br18_shared_word_resolves_by_hysteresis_to_the_most_recent_language() {
    let mgr = en_then_pt();
    assert_eq!(mgr.detect("hello"), Some(LangId::new("en")));
}

#[test]
fn br19b_prefix_breadth_discriminates_independently_of_active_order() {
    // Neither language contains "ap" as a whole word, so scoring reduces to
    // prefix breadth alone. The broader language ("hi": 3 completions) is placed
    // SECOND, so if only active-order (hysteresis) mattered "lo" would win — the
    // fact that "hi" wins proves the graded `+ prefix().len()` term discriminates.
    let mgr = LocaleManager::new(vec![
        (LangId::new("lo"), dict(&["apex"])),
        (LangId::new("hi"), dict(&["apple", "apply", "apricot"])),
    ])
    .expect("valid active set");
    assert_eq!(mgr.detect("ap"), Some(LangId::new("hi")));
}

#[test]
fn br19b_unrecognised_word_detects_nothing() {
    let mgr = en_then_pt();
    assert_eq!(mgr.detect("qwxyz"), None);
}

#[test]
fn br17_manual_switch_is_instant_and_flips_the_hysteresis_winner() {
    let mut mgr = en_then_pt();
    mgr.set_active(vec![
        (LangId::new("pt"), dict(&["hello", "mundo"])),
        (LangId::new("en"), dict(&["hello", "world"])),
    ])
    .expect("valid active set");
    assert_eq!(mgr.detect("hello"), Some(LangId::new("pt")));
}
