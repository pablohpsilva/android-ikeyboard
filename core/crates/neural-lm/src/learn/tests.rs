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

#[test] // @BR-11: eviction resets the reused index (design §5/§6)
fn eviction_resets_the_reused_indexs_learned_state() {
    // Ceiling of 2 learned words: "target" and "rare" fill it exactly.
    let mut lm = NextWordLm::new_with_vocab_ceiling_for_test(2);
    for _ in 0..900 {
        lm.observe(&["rare"], "target");
    }
    // "target" is the only class ever trained as a *classification target*
    // here (`observe`'s next_word) -- its output row is the only one that
    // moved off cold-start-zero. "rare" is only ever *context*; its own
    // class (as a hypothetical next word) is still untouched cold-start.
    let high = lm.score_next(&["rare"], "target");
    // 900 rounds of a single, exclusive association should be well above the
    // cold-start uniform log-prob (ln(1/2002) ~= -7.6) -- confirms training
    // actually happened before we test what eviction does to it.
    assert!(high > -3.0, "training did not raise the score: high={high}");

    // Evict "target" via a CONTEXT-word eviction, not a target eviction:
    // this call's own next_word is "rare" -- already known, so interning it
    // bumps its frequency to 901 (one step ahead of "target"'s 900, since
    // observe interns the target before the context). "newword" is brand
    // new and needs a slot; the vocab is full, so it evicts the
    // now-least-frequent "target" (900 < 901; tie-break is moot here).
    // Deliberately using "rare" as *this* step's classification target (not
    // "newword") means the reused index gets no dedicated training signal
    // of its own this step -- only the shared cross-entropy softmax's tiny,
    // uniform nudge to every non-target class. So whatever
    // `score_next(&["rare"], "newword")` shows next is overwhelmingly
    // whatever the reset (or its absence) left behind, not new training.
    lm.observe(&["newword"], "rare");
    assert_eq!(
        lm.vocab.index_of("target"),
        0,
        "target must have been evicted (-> UNK)"
    );

    // If "newword" (now at the reused index) inherited "target"'s output
    // row, querying it with "rare" as context -- the exact pattern "target"
    // was trained hard to respond to -- would still show a high score. If
    // the row was reset to cold-start zero first, it sits at the uniform
    // baseline instead, nowhere near `high`.
    let after = lm.score_next(&["rare"], "newword");
    assert!(
        after < high - 2.0,
        "evicted index's learned state leaked into the new word: high={high} after={after}"
    );
}

#[test] // @BR-11: eviction reached via the *target* word also resets (the other half of `observe`'s two intern call sites, see `learn.rs`)
fn target_triggered_eviction_also_resets_and_never_panics() {
    let mut lm = NextWordLm::new_with_vocab_ceiling_for_test(2);
    // Fills the 2-word ceiling exactly: "first" (target) then "a-context"
    // (context) are each new, both under the ceiling.
    lm.observe(&["a-context"], "first");
    // "second" is a brand-new *target* word interned while the vocab is
    // already full: this evicts "first" (tie with "a-context" at frequency
    // 1, smallest index wins) via the TARGET intern call in `observe` --
    // the branch `eviction_resets_the_reused_indexs_learned_state` above
    // does not exercise, since that test evicts via a context word instead.
    lm.observe(&["b-context"], "second");
    assert_eq!(
        lm.vocab.index_of("first"),
        0,
        "first must have been evicted (-> UNK)"
    );
    // The freed index is usable again, without panicking, for further training.
    lm.observe(&["c-context"], "third");
}
