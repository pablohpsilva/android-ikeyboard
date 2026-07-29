//! The live correction policy, exercised across the crate boundary.
//!
//! These behaviours previously existed only against
//! `FeatherKeyCore::choose_correction` — the composition root — while this crate
//! held a second, rank-blind corrector. The policy now lives here (its documented
//! home: SEDD §5/§15, ARCH §5.4, BR-12/BR-15/BR-45), so its characterisation
//! tests do too.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use featherkey_autocorrect::{LexiconPack, NoClobberCorrector};
use featherkey_contracts::{AutoCorrect, DeviceHints, Token, TypingContext};
use featherkey_dictionary::Dictionary;
use featherkey_language_momentum::Momentum;
use featherkey_locale_manager::{LangId, LocaleManager};
use featherkey_personalization::Personalization;

/// A pack from words given in **frequency order** (most common first) — the
/// shape `build_packs` produces from the bundled assets.
fn pack(tag: &str, words: &[&str]) -> LexiconPack {
    let rank: HashMap<String, u32> = words
        .iter()
        .enumerate()
        .map(|(i, w)| ((*w).to_owned(), i as u32))
        .collect();
    let mut sorted: Vec<&str> = words.to_vec();
    sorted.sort_unstable();
    LexiconPack {
        lang: tag.to_owned(),
        dict: Dictionary::from_sorted_words(sorted).expect("sorted"),
        rank,
    }
}

fn locales(packs: &[LexiconPack]) -> LocaleManager {
    LocaleManager::new(
        packs
            .iter()
            .map(|p| (LangId::new(p.lang.clone()), p.dict.clone()))
            .collect(),
    )
    .expect("valid active set")
}

fn corrector(packs: Vec<LexiconPack>, momentum: Momentum) -> NoClobberCorrector {
    let locales = locales(&packs);
    NoClobberCorrector::new(packs, Personalization::new(), locales, momentum)
}

/// One English lexicon where "cat" is the commonest word and "bat" the rarest —
/// but "bat" sorts first alphabetically.
fn en_corrector() -> NoClobberCorrector {
    let packs = vec![pack("en", &["cat", "dog", "hat", "bat"])];
    let momentum = Momentum::new("en", &["en".to_owned()]);
    corrector(packs, momentum)
}

fn fix(
    c: &NoClobberCorrector,
    text: &str,
    device: &DeviceHints,
) -> featherkey_contracts::Correction {
    c.correct(
        &Token {
            text: text.to_owned(),
        },
        &TypingContext::default(),
        device,
    )
}

#[test]
fn corrects_to_the_commonest_neighbour() {
    // "xat" is one substitution from bat/cat/hat.
    let got = fix(&en_corrector(), "xat", &DeviceHints::default());
    assert!(got.applied);
    assert_eq!(got.primary, "cat");
}

#[test]
fn alternatives_are_frequency_ordered() {
    let got = fix(&en_corrector(), "xat", &DeviceHints::default());
    assert_eq!(got.alternatives, vec!["hat".to_string(), "bat".to_string()]);
}

#[test]
fn momentum_decides_across_languages() {
    let packs = vec![pack("en", &["cat", "dog"]), pack("es", &["cas", "gato"])];
    let mut momentum = Momentum::new("en", &["en".to_owned(), "es".to_owned()]);
    for _ in 0..5 {
        momentum.observe(&["es".to_owned()]);
    }
    // "cax" is one substitution from en "cat" and es "cas", and prefixes neither.
    let got = fix(&corrector(packs, momentum), "cax", &DeviceHints::default());
    assert_eq!(got.primary, "cas");
}

#[test]
fn a_word_the_device_knows_is_not_clobbered() {
    let device = DeviceHints {
        known: vec!["privet".to_owned()],
        candidates: Vec::new(),
    };
    let got = fix(&en_corrector(), "privet", &device);
    assert_eq!(got.primary, "privet");
    assert!(!got.applied);
}

#[test]
fn an_unranked_neighbour_sorts_last() {
    // A pack whose `rank` omits "bat": it must trail the ranked neighbours
    // despite sorting first alphabetically. Unreachable through `build_packs`
    // (which derives rank and dict from one list), so it is pinned here.
    let mut p = pack("en", &["cat", "hat", "bat"]);
    p.rank.remove("bat");
    let momentum = Momentum::new("en", &["en".to_owned()]);
    let got = fix(
        &corrector(vec![p], momentum),
        "xat",
        &DeviceHints::default(),
    );
    assert_eq!(got.primary, "cat");
    assert_eq!(got.alternatives, vec!["hat".to_string(), "bat".to_string()]);
}
