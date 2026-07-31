//! Task 12 — gated online training of the neural re-ranker from strip picks.
//!
//! When the user picks a completion from the strip (or commits a word that was
//! shown), the core reinforces the neural re-ranker toward that word — but only
//! in an ordinary field, never a sensitive one, and consuming the single cached
//! shown-set snapshot so one pick trains exactly once. These end-to-end tests
//! pin the observable behaviour through the public façade; the score-level
//! single-train proof lives as a unit test in `learn.rs` (it needs the crate's
//! `neural_ranker()` read seam).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use featherkey_core::{Candidate, FeatherKeyCore, RankedCandidate, SensitiveContextSource};

/// An ordinary field: learning (and thus training) is allowed.
struct Ordinary;
impl SensitiveContextSource for Ordinary {
    fn is_sensitive(&self) -> bool {
        false
    }
}

/// A sensitive field (password/OTP): learning MUST be suppressed.
struct Sensitive;
impl SensitiveContextSource for Sensitive {
    fn is_sensitive(&self) -> bool {
        true
    }
}

/// A core whose "te…" completions are `tea` < `team` < `teach` in bundled
/// (frequency) order, so bundled rank alone puts `tea` first and `team` second.
fn core() -> FeatherKeyCore {
    FeatherKeyCore::new(vec![(
        "en".to_owned(),
        vec!["tea".to_owned(), "team".to_owned(), "teach".to_owned()],
    )])
    .expect("valid single-language core")
}

fn words(ranked: &[RankedCandidate]) -> Vec<String> {
    ranked.iter().map(|r| r.word.clone()).collect()
}

fn no_device() -> Vec<Candidate> {
    Vec::new()
}

/// The rank of `word` in a ranked list, or `usize::MAX` if absent.
fn rank_of(ranked: &[RankedCandidate], word: &str) -> usize {
    ranked
        .iter()
        .position(|r| r.word == word)
        .unwrap_or(usize::MAX)
}

#[test]
fn strip_picks_teach_the_ranker_to_promote_the_chosen_word() {
    let mut fk = core();

    // Baseline: "team" is behind "tea" for the prefix "te".
    let base = fk.rank_suggestions("", "te", no_device());
    assert!(
        rank_of(&base, "team") > rank_of(&base, "tea"),
        "precondition: team starts behind tea, got {:?}",
        words(&base)
    );

    // The user keeps typing "te" and picking "team" from the strip. Each round
    // re-ranks (refreshing the shown-set snapshot) and then records the pick,
    // which now also reinforces the neural ranker toward "team".
    for _ in 0..30 {
        let _ = fk.rank_suggestions("", "te", no_device());
        fk.observe_strip_pick("te", "team", &Ordinary);
    }

    let out = fk.rank_suggestions("", "te", no_device());
    assert!(
        rank_of(&out, "team") < rank_of(&out, "tea"),
        "after repeated picks 'team' should rank ahead of 'tea', got {:?}",
        words(&out)
    );
}

#[test]
fn training_is_suppressed_in_a_sensitive_field() {
    let mut fk = core();
    let before = words(&fk.rank_suggestions("", "te", no_device()));

    // The identical pick sequence, but in a sensitive field: every call
    // short-circuits at the sensitivity gate before recording or training.
    for _ in 0..30 {
        let _ = fk.rank_suggestions("", "te", no_device());
        fk.observe_strip_pick("te", "team", &Sensitive);
    }

    let after = words(&fk.rank_suggestions("", "te", no_device()));
    assert_eq!(
        before, after,
        "a sensitive-field pick must not change the ranking order"
    );
}
