//! Acceptance-style integration tests for the public [`Dictionary`] surface.
//!
//! These mirror the scenarios in `features/dictionary.feature`, exercising the
//! crate as a black box through its public API only (no access to internals).
//! They stand in for the executable BDD steps until the cucumber harness is
//! wired up (SEDD §12), and are tagged in comments to the BR each closes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use featherkey_dictionary::{Dictionary, DictionaryError, MAX_COMPLETIONS};

/// The shared lexicon from the feature file's `Background`.
fn lexicon() -> Dictionary {
    // Written pre-sorted, as a real lexicon file is.
    Dictionary::from_sorted_words(["apple", "apply", "apt", "cat", "cot"])
        .expect("fixture is sorted")
}

// @BR-10 — prefix lookup returns the completions for what is typed.
#[test]
fn prefix_lookup_returns_completions_in_order() {
    let d = lexicon();
    let completions = d.prefix("app");
    assert_eq!(completions, ["apple", "apply"]);
    assert!(!completions.contains(&"apt".to_string()));
}

// @BR-10 — completions are capped so the hot path stays bounded.
#[test]
fn completions_are_capped() {
    let words: Vec<String> = (0..MAX_COMPLETIONS + 5)
        .map(|i| format!("x{i:03}"))
        .collect();
    let d = Dictionary::from_sorted_words(words.iter()).expect("generated in order");
    assert_eq!(d.prefix("x").len(), MAX_COMPLETIONS);
}

// @BR-12 — an exact word is reported present, so it is never treated as a typo.
#[test]
fn exact_word_is_present() {
    let d = lexicon();
    assert!(d.contains("apt"));
    assert!(!d.contains("ap"));
}

// @BR-12 — a one-edit typo surfaces the intended word(s) as fuzzy matches.
#[test]
fn one_edit_typo_surfaces_intended_words() {
    let d = lexicon();
    // "cet" is one substitution from both "cat" and "cot".
    let matches = d.fuzzy("cet");
    assert_eq!(matches, ["cat", "cot"]);
    // The query itself is never offered as its own match.
    assert!(!matches.contains(&"cet".to_string()));
}

// The FST's sorted-set contract is surfaced as a value, never a panic.
#[test]
fn out_of_order_construction_is_an_error_value() {
    let built = Dictionary::from_sorted_words(["cot", "cat"]);
    assert_eq!(built.err(), Some(DictionaryError::Unsorted));
}
