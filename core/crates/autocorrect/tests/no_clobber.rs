//! Executable form of features/autocorrect.feature — exercises the public
//! `AutoCorrect` surface across the crate boundary (BR-12, BR-18).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use featherkey_autocorrect::{LexiconPack, NoClobberCorrector};
use featherkey_contracts::{AutoCorrect, DeviceHints, Token, TypingContext};
use featherkey_dictionary::Dictionary;
use featherkey_language_momentum::Momentum;
use featherkey_locale_manager::{LangId, LocaleManager};
use featherkey_personalization::Personalization;

fn dict(words: &[&str]) -> Dictionary {
    Dictionary::from_sorted_words(words.iter().copied()).expect("fixture is sorted")
}

/// Background: an English fuzzy dictionary with English and Portuguese both
/// active. "cat"/"cot"/"hat" are English words; "mundo" is Portuguese only.
fn corrector() -> NoClobberCorrector {
    let active = LocaleManager::new(vec![
        (LangId::new("en"), dict(&["cat", "cot", "hat"])),
        (LangId::new("pt"), dict(&["mundo", "olph"])),
    ])
    .expect("valid active set");
    let words = ["cat", "cot", "hat"];
    let pack = LexiconPack {
        lang: "en".to_owned(),
        dict: dict(&words),
        rank: words
            .iter()
            .enumerate()
            .map(|(i, w)| ((*w).to_owned(), i as u32))
            .collect(),
    };
    NoClobberCorrector::new(
        vec![pack],
        Personalization::new(),
        active,
        Momentum::new("en", &["en".to_owned(), "pt".to_owned()]),
    )
}

fn correct(text: &str) -> featherkey_contracts::Correction {
    corrector().correct(
        &Token {
            text: text.to_owned(),
        },
        &TypingContext::default(),
        &DeviceHints::default(),
    )
}

#[test]
fn br12_a_real_word_is_returned_unchanged() {
    let got = correct("cat");
    assert_eq!(got.primary, "cat");
    assert!(!got.applied);
    assert!(got.alternatives.is_empty());
}

#[test]
fn br18_a_word_from_a_second_active_language_is_not_corrected() {
    // "mundo" is absent from the English fuzzy dictionary but valid in the
    // second active language, so BR-12 still protects it (BR-18).
    let got = correct("mundo");
    assert_eq!(got.primary, "mundo");
    assert!(!got.applied);
}

#[test]
fn br12_a_non_word_is_offered_correction_candidates() {
    // "cxt" is no language's word; it is one edit from both "cat" and "cot".
    let got = correct("cxt");
    assert!(got.applied);
    assert_eq!(got.primary, "cat");
    assert_eq!(got.alternatives, ["cot"]);
    assert_ne!(got.primary, "cxt");
}

#[test]
fn br12_a_non_word_with_no_neighbours_is_left_untouched() {
    let got = correct("qqqq");
    assert_eq!(got.primary, "qqqq");
    assert!(!got.applied);
}

#[test]
fn br12_a_whitelisted_word_is_never_clobbered() {
    let active = LocaleManager::new(vec![(LangId::new("en"), dict(&["acne", "acre"]))])
        .expect("valid active set");
    let mut personal = Personalization::new();
    personal.whitelist("acme");
    let words = ["acne", "acre"];
    let c = NoClobberCorrector::new(
        vec![LexiconPack {
            lang: "en".to_owned(),
            dict: dict(&words),
            rank: words
                .iter()
                .enumerate()
                .map(|(i, w)| ((*w).to_owned(), i as u32))
                .collect(),
        }],
        personal,
        active,
        Momentum::new("en", &["en".to_owned()]),
    );
    let got = c.correct(
        &Token {
            text: "acme".to_owned(),
        },
        &TypingContext::default(),
        &DeviceHints::default(),
    );
    assert_eq!(got.primary, "acme");
    assert!(!got.applied);
}
