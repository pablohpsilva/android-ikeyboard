//! E-2 — the sensitive-context ordering property (BR-26).
//!
//! The architectural promise is that a keystroke typed into a password/OTP field
//! *structurally* cannot be learned: the composition root consults the
//! sensitivity gate BEFORE any learner runs. These tests pin that promise down
//! at the `featherkey-core` composition root — the one place the ordering is
//! actually wired — rather than trusting prose.
//!
//! - **Lexical learning** is directly observable (`knows_word`/`word_frequency`),
//!   so it is checked as a property over arbitrary input.
//! - **Tap-geometry learning** is observable only through its effect on
//!   decoding, so it is checked with a deterministic scenario where a learned
//!   bias would flip the decoded key — and does under an ordinary field, but is
//!   suppressed under a sensitive one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use featherkey_core::{FeatherKeyCore, SensitiveContextSource};
use proptest::prelude::*;

/// A sensitive field (password/OTP): learning MUST be suppressed.
struct Sensitive;
impl SensitiveContextSource for Sensitive {
    fn is_sensitive(&self) -> bool {
        true
    }
}

/// An ordinary field: learning is allowed.
struct Ordinary;
impl SensitiveContextSource for Ordinary {
    fn is_sensitive(&self) -> bool {
        false
    }
}

fn core() -> FeatherKeyCore {
    FeatherKeyCore::new(vec![(
        "en".to_owned(),
        vec!["cat".to_owned(), "cot".to_owned(), "dog".to_owned()],
    )])
    .expect("valid single-language core")
}

proptest! {
    /// No word typed into a sensitive field is ever learned — for ANY input.
    #[test]
    fn sensitive_field_learns_no_vocabulary(
        words in proptest::collection::vec("[a-z]{1,8}", 0..24)
    ) {
        let mut fk = core();
        for w in &words {
            fk.learn_word("the", w, &Sensitive);
        }
        for w in &words {
            prop_assert!(!fk.knows_word(w), "sensitive field leaked a learned word: {w}");
            prop_assert_eq!(fk.word_frequency(w), 0);
        }
        // The single gate also covers the next-word (context) model: no
        // "the -> w" transition may have been recorded either.
        prop_assert!(
            fk.context_next_words("the", 32).is_empty(),
            "sensitive field leaked a next-word transition"
        );
    }

    /// The same input in an ordinary field IS learned — proving the gate is what
    /// suppresses learning above, not some unrelated no-op.
    #[test]
    fn ordinary_field_learns_vocabulary(
        words in proptest::collection::vec("[a-z]{1,8}", 1..24)
    ) {
        let mut fk = core();
        for w in &words {
            fk.learn_word("the", w, &Ordinary);
        }
        for w in &words {
            prop_assert!(fk.knows_word(w));
            prop_assert!(fk.word_frequency(w) >= 1);
        }
    }

    /// Correction signals are gated by the same rule: nothing a user picks or
    /// reverts inside a sensitive field is ever recorded — for ANY input.
    #[test]
    fn sensitive_field_learns_no_correction_signals(
        picks in proptest::collection::vec("[a-z]{1,8}", 0..24)
    ) {
        let mut fk = core();
        for w in &picks {
            fk.observe_strip_pick("pre", w, &Sensitive);
            fk.observe_delete_retype(w, &Sensitive);
        }
        for w in &picks {
            prop_assert_eq!(fk.correction_pref_count("pre", w), 0);
            prop_assert_eq!(fk.correction_unwanted_count(w), 0);
        }
    }
}

/// Tap-geometry learning is gated too. A consistent off-centre tap on `q`,
/// learned, shifts `q`'s effective centre far enough that a touch at x=140
/// (otherwise nearest `w`) resolves to `q`. Under a sensitive field that
/// learning is dropped, so the same touch still resolves to `w`.
#[test]
fn sensitive_field_learns_no_tap_geometry() {
    let touch_x = 140.0;
    let touch_y = 60.0; // row centre (keys are 100x120 from the origin)

    // Control: ordinary field learns the bias, so q wins.
    let mut ordinary = core();
    for _ in 0..6 {
        ordinary.observe_tap('q', 90.0, 0.0, &Ordinary).unwrap();
    }
    let learned = ordinary.decode(touch_x, touch_y).unwrap();
    assert_eq!(
        learned.best.as_deref(),
        Some("q"),
        "an ordinary field should learn the tap bias and resolve to q"
    );

    // Gated: sensitive field drops the identical taps, so w still wins.
    let mut sensitive = core();
    for _ in 0..6 {
        sensitive.observe_tap('q', 90.0, 0.0, &Sensitive).unwrap();
    }
    let suppressed = sensitive.decode(touch_x, touch_y).unwrap();
    assert_eq!(
        suppressed.best.as_deref(),
        Some("w"),
        "a sensitive field must not learn tap bias; the touch stays nearest w"
    );
}

/// A field that flips from sensitive to ordinary mid-session learns only what is
/// typed while ordinary — the gate is consulted per call, not once.
#[test]
fn gate_is_consulted_per_call() {
    let mut fk = core();
    fk.learn_word("", "secretword", &Sensitive);
    fk.learn_word("", "publicword", &Ordinary);
    assert!(!fk.knows_word("secretword"));
    assert!(fk.knows_word("publicword"));
}

/// End-to-end gating for the W6b correction-ranking wiring: a strip pick made in
/// a sensitive field must not promote anything, because the pick was never
/// recorded. Contrast an ordinary field, where the same picks DO promote — so the
/// test proves it is the gate, not an inert bonus, that suppresses the change.
#[test]
fn sensitive_strip_pick_does_not_change_ranking() {
    // A lexicon whose "te…" completions are tea < team by bundled rank.
    let lex = || {
        FeatherKeyCore::new(vec![(
            "en".to_owned(),
            vec!["tea".to_owned(), "team".to_owned()],
        )])
        .expect("valid core")
    };
    let top = |fk: &mut FeatherKeyCore| fk.rank_suggestions("", "te", Vec::new())[0].word.clone();

    // Sensitive: repeated picks are dropped, so "tea" (bundled-first) still leads.
    let mut sensitive = lex();
    assert_eq!(top(&mut sensitive), "tea");
    for _ in 0..5 {
        sensitive.observe_strip_pick("te", "team", &Sensitive);
    }
    assert_eq!(
        top(&mut sensitive),
        "tea",
        "a sensitive-field strip pick must not promote 'team'"
    );

    // Ordinary control: the identical picks DO promote "team".
    let mut ordinary = lex();
    for _ in 0..5 {
        ordinary.observe_strip_pick("te", "team", &Ordinary);
    }
    assert_eq!(
        top(&mut ordinary),
        "team",
        "an ordinary-field strip pick should promote 'team' (proves the gate suppresses, not a no-op)"
    );
}

/// The unwanted (delete-retype) demotion is gated the same way: repeatedly
/// deleting a word in a sensitive field records nothing, so ranking is unchanged;
/// the identical deletes in an ordinary field DO demote it.
#[test]
fn sensitive_delete_retype_does_not_change_ranking() {
    let lex = || {
        FeatherKeyCore::new(vec![(
            "en".to_owned(),
            vec!["tea".to_owned(), "team".to_owned()],
        )])
        .expect("valid core")
    };
    let top = |fk: &mut FeatherKeyCore| fk.rank_suggestions("", "te", Vec::new())[0].word.clone();

    // Sensitive: deletes are dropped, so "tea" (bundled-first) still leads.
    let mut sensitive = lex();
    for _ in 0..5 {
        sensitive.observe_delete_retype("tea", &Sensitive);
    }
    assert_eq!(
        top(&mut sensitive),
        "tea",
        "a sensitive-field delete-retype must not demote 'tea'"
    );

    // Ordinary control: the identical deletes DO demote "tea" below "team".
    let mut ordinary = lex();
    for _ in 0..5 {
        ordinary.observe_delete_retype("tea", &Ordinary);
    }
    assert_eq!(
        top(&mut ordinary),
        "team",
        "an ordinary-field delete-retype should demote 'tea' (proves the gate suppresses, not a no-op)"
    );
}
