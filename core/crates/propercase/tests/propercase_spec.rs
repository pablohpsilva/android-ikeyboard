//! Executable form of features/propercase.feature (BR-69). Each `#[test]`
//! mirrors one Gherkin scenario one-to-one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use featherkey_propercase::ProperCaser;

fn common(words: &'static [&'static str]) -> impl Fn(&str) -> bool {
    move |w: &str| words.contains(&w)
}

// @BR-69 — A known proper noun typed lowercase is capitalized mid-sentence
#[test]
fn known_proper_noun_typed_lowercase_is_capitalized() {
    let c = ProperCaser::new(["Paris"], std::iter::empty::<&str>());
    assert_eq!(c.case("paris", false, &common(&[])), Some("Paris".to_owned()));
}

// @BR-69 — A word that is also a common lowercase word is left alone
#[test]
fn common_word_twin_is_left_alone() {
    let c = ProperCaser::new(["Rose"], std::iter::empty::<&str>());
    assert_eq!(c.case("rose", false, &common(&["rose"])), None);
}

// @BR-69 — The canonical form restores accents as well as case
#[test]
fn canonical_restores_accents_and_case() {
    let c = ProperCaser::new(["João"], std::iter::empty::<&str>());
    assert_eq!(c.case("joao", false, &common(&[])), Some("João".to_owned()));
}

// @BR-69 — A word at a sentence start is left to auto-capitalization
#[test]
fn sentence_start_is_left_to_auto_caps() {
    let c = ProperCaser::new(["Paris"], std::iter::empty::<&str>());
    assert_eq!(c.case("paris", true, &common(&[])), None);
}

// @BR-69 — Deliberate all-caps is never rewritten
#[test]
fn all_caps_is_never_rewritten() {
    let c = ProperCaser::new(["Paris"], std::iter::empty::<&str>());
    assert_eq!(c.case("PARIS", false, &common(&[])), None);
}

// @BR-69 — An unknown word is left unchanged
#[test]
fn unknown_word_is_unchanged() {
    let c = ProperCaser::new(["Paris"], std::iter::empty::<&str>());
    assert_eq!(c.case("florp", false, &common(&[])), None);
}
