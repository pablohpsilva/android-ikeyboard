#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

/// Best-ranked word from a `rank_next` result, `""` if empty. Owned (not
/// `&str`) so callers can pass a temporary `rank_next(..)` result directly,
/// matching the brief's `top(&lm.rank_next(...))` call shape.
fn top(ranked: &[(String, f32)]) -> String {
    ranked.first().map_or(String::new(), |(w, _)| w.clone())
}

#[test] // @BR-11 scenario 1
fn learns_two_word_context_the_bigram_cannot() {
    let mut lm = NextWordLm::new();
    for _ in 0..900 {
        lm.observe(&["going", "to"], "work");
        lm.observe(&["walking", "to"], "school");
    }
    let after_going = top(&lm.rank_next(&["going", "to"], 5));
    let after_walking = top(&lm.rank_next(&["walking", "to"], 5));
    assert_eq!(after_going, "work");
    assert_eq!(after_walking, "school");
}

#[test] // @BR-11 escape-from-uniform (dead-ReLU guard)
fn training_escapes_uniform() {
    let mut lm = NextWordLm::new();
    for _ in 0..300 {
        lm.observe(&["hello"], "there");
    }
    assert_eq!(top(&lm.rank_next(&["hello"], 3)), "there");
    assert!(lm.confidence() > 0.0);
}

#[test] // @BR-11: warmup (and therefore confidence) rises with every observe
fn confidence_rises_monotonically_with_observe() {
    let mut lm = NextWordLm::new();
    assert_eq!(lm.confidence(), 0.0);
    let mut last = lm.confidence();
    for _ in 0..20 {
        lm.observe(&["going", "to"], "work");
        let now = lm.confidence();
        assert!(now > last, "confidence did not rise: {now} <= {last}");
        last = now;
    }
}

#[test] // @BR-11: a non-learnable target teaches nothing (no warmup bump)
fn observing_a_non_learnable_target_is_a_no_op() {
    let mut lm = NextWordLm::new();
    lm.observe(&["going", "to"], "a"); // "a" is too short to be learnable
    assert_eq!(lm.confidence(), 0.0);
    assert_eq!(lm.vocab.index_of("a"), 0); // UNK, never registered
}

#[test] // @BR-11 scenario 2, with contamination guard
fn generalises_across_similar_contexts_via_embeddings() {
    // NOTE: the design's illustrative example uses "a"/"the" as the two
    // determiners; this test uses "an"/"the" instead. "a" is 1 character, and
    // `Vocab::intern` (via the shared `is_learnable` rule, `MIN_TOKEN_CHARS ==
    // 2` — see `featherkey_context::is_learnable`) never registers it: as a
    // context word it always resolves to the shared `UNK` row, not a
    // dedicated one, so it has no embedding of its own to generalise from —
    // no amount of tuning rounds/LR/margin fixes that, it is structural.
    // "an" (2 chars) is learnable and plays the identical grammatical role,
    // so it exercises the actual mechanism under test.
    let train = |lm: &mut NextWordLm| {
        // 250 (not 300, matching the other two scenarios): the margin between
        // the trained and frozen-embedding twins is a transient peak, not a
        // monotone gap that only grows with more training (see the report for
        // why: "an" has no contrastive dog signal of its own, so with enough
        // rounds SGD drives its embedding to a maximally cat-specific point
        // and the transfer effect fades). 250 sits centered in a wide, stable
        // plateau (empirically margin > 0.5 for rounds in roughly [210, 280]
        // at this LR — see the report's tuning table), so this is not a
        // knife-edge choice.
        for _ in 0..250 {
            lm.observe(&["the"], "cat");
            lm.observe(&["an"], "cat");
            lm.observe(&["the"], "dog");
        }
    };
    // "an dog" was never typed; the shared behaviour of "the"/"an" (learned
    // into their embeddings) must pull "dog" up after "an".
    let mut lm = NextWordLm::new();
    train(&mut lm);
    let learned = lm.score_next(&["an"], "dog");

    // Contamination guard (app #3 lesson: assert a MARGIN, not a binary — w1/b1
    // still train in the twin, so "dog" could otherwise sneak in and the test
    // would never go RED). The frozen twin's `observe` skips the embedding
    // update, so the embedding is the *only* remaining path to generalisation.
    let mut frozen = NextWordLm::new_frozen_embeddings_for_test();
    train(&mut frozen);
    let without = frozen.score_next(&["an"], "dog");

    // Embedding learning must lift "dog" after "an" by a clear margin over
    // the frozen twin — this fails (goes RED) if the embedding update is
    // dropped.
    assert!(
        learned > without + 0.5,
        "learned={learned} without={without}"
    );
}
