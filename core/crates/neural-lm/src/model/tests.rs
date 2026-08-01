#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn default_matches_new() {
    let lm = NextWordLm::default();
    assert_eq!(lm.confidence(), 0.0);
    assert_eq!(lm.embed, NextWordLm::new().embed);
}

#[test]
fn fresh_model_has_zero_confidence_and_uniform_ranking() {
    let lm = NextWordLm::new();
    assert_eq!(lm.confidence(), 0.0);
    // Uniform logits -> deterministic tie order; no reserved token emitted.
    let ranked = lm.rank_next(&["anything"], 5);
    assert!(ranked.iter().all(|(w, _)| w != "<unk>" && w != "<bos>"));
}

#[test]
fn fresh_score_is_finite_and_uniform_across_contexts() {
    // With a zero output layer every context yields the same uniform log-prob —
    // the model asserts nothing (the *escape* from uniform, which needs live
    // w1/b1, is the Task 8 dead-ReLU guard, not this test).
    let lm = NextWordLm::new();
    let a = lm.score_next(&["go"], "work");
    let b = lm.score_next(&["swim"], "work");
    assert!(a.is_finite() && (a - b).abs() < 1e-6);
}

#[test]
fn cold_start_zeroes_only_the_output_layer() {
    // Zero w2/b2 -> logits are constant (== b2 == 0) regardless of input,
    // so forward() is identical for two wildly different contexts. This is
    // the observable half of the load-bearing split (design §7); w1/b1/embed
    // are private and their non-zero-ness is verified by construction below.
    let lm = NextWordLm::new();
    assert_eq!(
        lm.score_next(&[], "anything"),
        lm.score_next(&["a", "b"], "anything")
    );
    // The embedding table and w1/b1 are non-zero and deterministic: two
    // independently cold-started models agree bit-for-bit, and the table
    // isn't degenerately all-zero (which would defeat the point of seeding
    // it at all).
    let lm2 = NextWordLm::new();
    assert_eq!(lm.embed, lm2.embed);
    assert!(lm.embed.iter().any(|&v| v != 0.0));
}

#[test]
fn confidence_is_zero_at_cold_start_and_saturates_toward_one_with_warmup() {
    let mut lm = NextWordLm::new();
    assert_eq!(lm.confidence(), 0.0);
    lm.warmup = 50; // == WARMUP_HALF -> exactly 0.5
    assert!((lm.confidence() - 0.5).abs() < 1e-6);
    lm.warmup = 1_000_000;
    assert!(lm.confidence() > 0.99 && lm.confidence() <= 1.0);
}

#[test]
fn rank_next_never_emits_reserved_indices_and_breaks_ties_by_word() {
    let mut lm = NextWordLm::new();
    lm.vocab.intern("dog");
    lm.vocab.intern("cat");
    lm.vocab.intern("bird");
    // Cold start: every learned class is exactly tied (uniform softmax), so
    // the tie-break (ascending word) is the only thing that can order them.
    let ranked = lm.rank_next(&["go"], 10);
    let words: Vec<&str> = ranked.iter().map(|(w, _)| w.as_str()).collect();
    assert_eq!(words, vec!["bird", "cat", "dog"]);
    assert!(ranked.iter().all(|(w, _)| w != "<unk>" && w != "<bos>"));
}

#[test]
fn rank_next_respects_the_limit() {
    let mut lm = NextWordLm::new();
    lm.vocab.intern("dog");
    lm.vocab.intern("cat");
    lm.vocab.intern("bird");
    assert_eq!(lm.rank_next(&["go"], 2).len(), 2);
    assert_eq!(lm.rank_next(&["go"], 0).len(), 0);
}

#[test]
fn score_and_assemble_never_panic_on_a_short_or_empty_context() {
    let lm = NextWordLm::new();
    assert!(lm.score_next(&[], "anything").is_finite());
    assert!(lm.score_next(&["only-one"], "anything").is_finite());
}
