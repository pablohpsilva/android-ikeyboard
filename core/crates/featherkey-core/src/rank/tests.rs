//! Unit tests for `rank.rs` (`rank_suggestions` and its helpers). Kept as a
//! file-based module (ARCH §6: no god-files) so `rank.rs` stays under the
//! 500-line fitness limit while every test remains an in-crate unit test with
//! visibility into `rank`'s private items and `FeatherKeyCore`'s
//! `#[cfg(test)] pub(crate)` accessors (e.g. `lm_mut()`, `recent_mut()`).

use super::*;

/// The ranked words, dropping scores, for order assertions.
fn words_of(ranked: &[RankedCandidate]) -> Vec<&str> {
    ranked.iter().map(|r| r.word.as_str()).collect()
}

#[test]
fn rank_suggestions_orders_by_bundled_rank_when_nothing_learned() {
    // No context, no learned usage: the commoner bundled word (lower rank,
    // earlier in the frequency-ordered input) wins. Proves dict_rank flows.
    let mut core = FeatherKeyCore::new(vec![(
        "en".into(),
        vec!["cat".into(), "car".into(), "can".into()],
    )])
    .expect("core");
    let out = core.rank_suggestions("", "ca", vec![]);
    assert_eq!(words_of(&out), ["cat", "car", "can"]);
}

#[test]
fn rank_suggestions_lets_context_beat_bundled_rank() {
    // "car" is commoner (rank 0) than "cat" (rank 1), but the bigram context
    // after "the" favours "cat", which must then win. Proves context flows.
    let mut core =
        FeatherKeyCore::new(vec![("en".into(), vec!["car".into(), "cat".into()])]).expect("core");
    core.import_context([("the".to_string(), "cat".to_string(), 3)]);
    let out = core.rank_suggestions("the", "ca", vec![]);
    assert_eq!(out[0].word, "cat");
}

#[test]
fn rank_suggestions_tags_completion_with_its_pack_language() {
    // A completion drawn from the es pack keeps its language across the blend.
    let mut core = FeatherKeyCore::new(vec![
        ("en".into(), vec!["cat".into()]),
        ("es".into(), vec!["gato".into()]),
    ])
    .expect("core");
    let out = core.rank_suggestions("", "ga", vec![]);
    assert_eq!(out[0].word, "gato");
    assert_eq!(out[0].lang, "es");
}

#[test]
fn rank_suggestions_surfaces_the_apostrophe_variant_of_the_typed_token() {
    // Typing "hell" must still offer "he'll" — derived from the fold group,
    // never a hand-authored table.
    let mut core = FeatherKeyCore::new(vec![(
        "en".into(),
        vec!["hell".into(), "hello".into(), "he'll".into()],
    )])
    .expect("core");
    let out = core.rank_suggestions("", "hell", vec![]);
    assert!(
        out.iter().any(|r| r.word == "he'll"),
        "he'll not offered: {:?}",
        words_of(&out)
    );
}

#[test]
fn accent_variants_are_the_exact_fold_group_minus_the_typed_word() {
    // "hell" folds to itself; its exact fold group is {hell, he'll}. The
    // typed word is excluded and "hello" (different fold) is not a member.
    let core = FeatherKeyCore::new(vec![(
        "en".into(),
        vec!["hell".into(), "hello".into(), "he'll".into()],
    )])
    .expect("core");
    let variants: Vec<String> = core
        .accent_variants("hell")
        .into_iter()
        .map(|r| r.word)
        .collect();
    assert_eq!(variants, vec!["he'll".to_string()]);
}

#[test]
fn accent_variants_rank_by_minimum_across_all_active_packs() {
    // Regression pin (r-u-sure round 1): a variant shared across languages
    // with crossed frequency ranks must sort by the MINIMUM rank across packs
    // (Kotlin Vocabulary.rankOf), not the first pack's rank. Here "café" is
    // rare in en (position 2) but commonest in es (position 0), while "cafè"
    // is position 1 in en only. Min ranks: café=0, cafè=1 -> café first. The
    // old first-pack-only lookup would have ranked cafè (en pos 1) ahead.
    let core = FeatherKeyCore::new(vec![
        (
            "en".into(),
            vec!["the".into(), "cafè".into(), "café".into()],
        ),
        ("es".into(), vec!["café".into(), "and".into()]),
    ])
    .expect("core");
    let variants: Vec<String> = core
        .accent_variants("cafe")
        .into_iter()
        .map(|r| r.word)
        .collect();
    assert_eq!(variants, vec!["café".to_string(), "cafè".to_string()]);
}

#[test]
fn guarantee_fold_variant_inserts_an_unshown_variant_at_slot_two() {
    // With only the plain twin ranked, the guarantee splices the accented
    // form into the second slot (index 1), mirroring the Kotlin behaviour.
    let core = FeatherKeyCore::new(vec![("en".into(), vec!["hell".into(), "he'll".into()])])
        .expect("core");
    let ranked = vec![RankedCandidate {
        word: "hell".into(),
        lang: "en".into(),
        score: 0.0,
    }];
    let out = core.guarantee_fold_variant("hell", ranked);
    assert_eq!(words_of(&out), ["hell", "he'll"]);
}

#[test]
fn rank_suggestions_appends_device_candidates_under_momentum() {
    // Device candidates blend in; strong es momentum promotes the es word
    // over an equally-ranked en one — proving language survives the blend.
    use featherkey_contracts::{Candidate, Source};
    let mut core = FeatherKeyCore::new(vec![
        ("en".into(), vec!["hello".into()]),
        ("es".into(), vec!["hola".into()]),
    ])
    .expect("core");
    for _ in 0..5 {
        core.observe_language(vec!["es".into()]);
    }
    let device = vec![
        Candidate {
            word: "hello".into(),
            lang: "en".into(),
            source: Source::Device,
            source_rank: 0,
        },
        Candidate {
            word: "hola".into(),
            lang: "es".into(),
            source: Source::Device,
            source_rank: 0,
        },
    ];
    let out = core.rank_suggestions("", "", device);
    assert_eq!(out[0].word, "hola");
}

#[test]
fn correction_parts_split_the_two_correction_signals() {
    // The promote/demote split the neural re-ranker consumes as two features:
    // each is its own closed form and both are strictly positive once observed.
    // Record 3 sticky-picks for ("ca","cat") and 2 delete-retypes for "cat".
    struct Ordinary;
    impl featherkey_contracts::SensitiveContextSource for Ordinary {
        fn is_sensitive(&self) -> bool {
            false
        }
    }
    let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
    for _ in 0..3 {
        core.observe_strip_pick("ca", "cat", &Ordinary);
    }
    for _ in 0..2 {
        core.observe_delete_retype("cat", &Ordinary);
    }
    let (promote, demote) = core.correction_parts("ca", "cat");
    // Both terms exercise their count > 0 branch and are strictly positive.
    assert!(promote > 0.0, "promote should be positive: {promote}");
    assert!(demote > 0.0, "demote should be positive: {demote}");
    // Exact closed form: sticky-weighted ln(1+picks), unwanted-weighted ln(1+unwanted).
    assert_eq!(promote, CORRECTION_STICKY_WEIGHT * f64::from(1 + 3).ln());
    assert_eq!(demote, CORRECTION_UNWANTED_WEIGHT * f64::from(1 + 2).ln());
}

#[test]
fn prior_coeffs_match_the_source_constants() {
    // Drift guard: the prior must stay assembled from the consts the classic
    // ranking uses, so a source change can't silently desync it.
    assert_eq!(
        PRIOR_COEFFS,
        [
            1.0,
            LM_WEIGHT_LANG as f32,
            SOURCE_PRIOR_LEXICON as f32,
            SOURCE_PRIOR_DEVICE as f32,
            1.0,
            -1.0,
            SPATIAL_WEIGHT as f32,
            LM_LOGPROB_COEFF,
            0.0,
        ]
    );
    // Pin the concrete literals too, so a change to any source const is caught
    // even if the assembly expression above were edited in lockstep with it.
    assert_eq!(
        PRIOR_COEFFS,
        [1.0, 1.0, 0.2, 0.0, 1.0, -1.0, 0.35, 1.0, 0.0]
    );
}

#[test]
fn no_lm_seeds_at_warmup_zero() {
    // Fresh core: `self.lm`'s vocab is empty, so `rank_next` yields nothing
    // and the empty-prefix candidate set is exactly as it was pre-Task-6 —
    // no phantom candidates, golden ordering untouched.
    let mut core =
        FeatherKeyCore::new(vec![("en".into(), vec!["cat".into(), "dog".into()])]).expect("core");
    let out = core.rank_suggestions("the", "", vec![]);
    assert!(
        out.is_empty(),
        "expected no candidates: {:?}",
        words_of(&out)
    );
}

#[test]
fn a_generalised_next_word_is_seeded_after_a_boundary() {
    // Warm the LM directly via the Task-5 test accessors (`learn_word`
    // doesn't train the LM until Task 7): 250 rounds of "the"->cat,
    // "an"->cat, "the"->dog teaches the shared "the"/"an" embedding
    // context to favour "cat", and the embedding-transfer effect lifts
    // "dog" after "an" too, even though "an dog" was never observed —
    // mirrors `neural-lm`'s own
    // `generalises_across_similar_contexts_via_embeddings` proof, now
    // exercised through the live `rank_suggestions` strip.
    let mut core =
        FeatherKeyCore::new(vec![("en".into(), vec!["cat".into(), "dog".into()])]).expect("core");
    for _ in 0..250 {
        core.lm_mut().observe(&["the"], "cat");
        core.lm_mut().observe(&["an"], "cat");
        core.lm_mut().observe(&["the"], "dog");
    }
    // `recent` is left untouched (empty buffer): `two_word_context("an")`
    // already degrades to `["an"]` — exactly the context trained above —
    // so no `recent_mut` positioning is needed for this scenario.
    let out = core.rank_suggestions("an", "", vec![]);
    assert!(
        out.iter().any(|r| r.word == "dog"),
        "\"dog\" not seeded: {:?}",
        words_of(&out)
    );
}

#[test]
fn correction_parts_are_zero_without_history() {
    // Both count == 0 branches: a word with no correction history yields (0, 0),
    // so ranking is unchanged — the two together net to the same zero adjustment.
    let core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
    let (promote, demote) = core.correction_parts("ca", "cat");
    assert_eq!(promote, 0.0);
    assert_eq!(demote, 0.0);
}
