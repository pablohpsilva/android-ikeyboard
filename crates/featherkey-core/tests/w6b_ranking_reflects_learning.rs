//! W6b integration: the strip ranking actually reflects what the core has
//! learned. Three learning channels feed `rank_suggestions`, and each is pinned
//! here end-to-end through the public façade:
//!
//! 1. **Learned frequency** — a word the user types often rises among its prefix
//!    completions.
//! 2. **Context (bigram)** — after a word + space, the next-word prediction comes
//!    from what the user has typed after that word before.
//! 3. **Strip-pick "sticky fix"** — a completion the user repeatedly picks for a
//!    prefix is promoted above the default, even though its frequency and bundled
//!    rank are unchanged (this is the correction signal wired into ranking in W6b).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use featherkey_core::{Candidate, FeatherKeyCore, RankedCandidate, SensitiveContextSource};

struct Ordinary;
impl SensitiveContextSource for Ordinary {
    fn is_sensitive(&self) -> bool {
        false
    }
}

/// A core whose only "te…" completions are `tea` < `team` < `teach` in bundled
/// (frequency) order, so bundled rank alone puts `tea` first — a clean baseline
/// against which learning must be shown to change the order.
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

#[test]
fn baseline_order_follows_bundled_rank() {
    let fk = core();
    let out = fk.rank_suggestions("", "te", no_device());
    assert_eq!(
        words(&out),
        vec!["tea".to_string(), "team".to_string(), "teach".to_string()],
        "with nothing learned, bundled (frequency) order stands"
    );
}

#[test]
fn learned_frequency_promotes_a_completion() {
    let mut fk = core();
    // Baseline: "teach" is last (bundled rank 2).
    assert_eq!(fk.rank_suggestions("", "te", no_device())[2].word, "teach");

    // The user types "teach" several times in an ordinary field.
    for _ in 0..3 {
        fk.learn_word("", "teach", &Ordinary);
    }

    // It now leads its prefix completions — learned frequency outranks bundled rank.
    let out = fk.rank_suggestions("", "te", no_device());
    assert_eq!(
        out[0].word, "teach",
        "a frequently typed word rises among its completions"
    );
}

#[test]
fn context_drives_next_word_prediction_on_an_empty_prefix() {
    let mut fk = core();
    // Nothing follows "open" yet: an empty-prefix query has no next-word guess.
    assert!(
        fk.rank_suggestions("open", "", no_device()).is_empty(),
        "no context learned means no next-word prediction"
    );

    // The user writes "open team" a few times.
    for _ in 0..2 {
        fk.learn_word("open", "team", &Ordinary);
    }

    // After "open ", the strip now predicts "team" from the learned bigram.
    let out = fk.rank_suggestions("open", "", no_device());
    assert!(
        words(&out).contains(&"team".to_string()),
        "next-word prediction reflects the learned context, got {:?}",
        words(&out)
    );
}

#[test]
fn a_repeated_strip_pick_promotes_the_chosen_completion() {
    let mut fk = core();
    // Baseline: "tea" leads, "team" is not first.
    let base = words(&fk.rank_suggestions("", "te", no_device()));
    assert_eq!(base[0], "tea");
    assert_ne!(base[0], "team");

    // The user keeps choosing "team" from the strip after typing "te" — a signal
    // that the default ("tea") is wrong for them. This records ONLY a correction
    // preference; it does not touch learned frequency.
    for _ in 0..3 {
        fk.observe_strip_pick("te", "team", &Ordinary);
    }
    assert_eq!(
        fk.word_frequency("team"),
        0,
        "a strip pick must NOT be counted as a typed-word frequency"
    );

    // The sticky-fix bonus now floats "team" to the top for that prefix.
    let out = words(&fk.rank_suggestions("", "te", no_device()));
    assert_eq!(
        out[0], "team",
        "a repeatedly-picked completion is promoted to the front, got {out:?}"
    );
}

#[test]
fn a_strip_pick_only_promotes_for_the_prefix_it_was_made_under() {
    let mut fk = core();
    // Repeatedly pick "team" — but recorded under a DIFFERENT prefix ("tea").
    for _ in 0..5 {
        fk.observe_strip_pick("tea", "team", &Ordinary);
    }
    // Ranking for prefix "te" is unaffected: the bonus is prefix-scoped.
    let out = words(&fk.rank_suggestions("", "te", no_device()));
    assert_eq!(
        out[0], "tea",
        "a pick under prefix 'tea' must not promote 'team' for prefix 'te', got {out:?}"
    );
}
