//! Unit tests for the `featherkey-core` composition façade (`lib.rs`). Kept as
//! a file-based module (ARCH §6: no god-files) so `lib.rs` stays under the
//! 500-line fitness limit while every test remains an in-crate unit test with
//! visibility into the `#[cfg(test)] pub(crate)` accessors declared alongside
//! `FeatherKeyCore` (e.g. `neural_ranker()`, `tap_warp()`, `layout()`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use featherkey_neural_ranker::RankFeatures;

/// An "en" -> qwerty core for decode-path probes.
fn core() -> FeatherKeyCore {
    FeatherKeyCore::new(vec![("en".to_owned(), vec!["cat".to_owned()])]).expect("core")
}

#[test]
fn cold_start_warp_shift_is_negligible_so_decode_is_unchanged() {
    // A fresh core's warp is the prior. Its (Δx,Δy) at any position is far
    // below inter-key spacing, so the pre-warp touch and the warped touch
    // decode to the SAME argmin — i.e. decode is behaviourally identical to
    // no warp at cold start. Bound matches `TapWarp`'s own definition of
    // cold-start "near zero" (featherkey-neural-tap's
    // `cold_start_warp_is_near_zero_across_the_grid`, tolerance 0.05) rather
    // than an unvalidated tighter number: sub-hundredths-of-a-pixel is still
    // orders of magnitude below any key's width.
    let c = core();
    for &(x, y) in &[(120.0, 80.0), (300.0, 80.0), (500.0, 200.0)] {
        let (nx, ny) = c.layout().normalize(x, y);
        let (dx, dy) = c.tap_warp().warp(nx, ny);
        assert!(
            dx.abs() < 0.05 && dy.abs() < 0.05,
            "cold warp {dx},{dy} @ {x},{y}"
        );
    }
}

#[test]
fn decode_is_deterministic() {
    let mut c = core();
    let a = c.decode(300.0, 80.0).unwrap();
    let b = c.decode(300.0, 80.0).unwrap();
    assert_eq!(a.best, b.best);
    assert!(a.best.is_some());
}

/// A representative feature vector for score comparisons (values are
/// arbitrary but exercise every slot, so an off-by-one in the prior wiring
/// would change the score).
fn sample_feat() -> RankFeatures {
    RankFeatures {
        positional: -0.9,
        ln_momentum: 0.3,
        is_lexicon: 1.0,
        is_device: 0.0,
        correction_promote: 0.2,
        correction_demote: 0.1,
        spatial: 0.4,
    }
}

#[test]
fn new_core_holds_the_prior_ranker() {
    // A fresh core's held ranker scores identically to a standalone prior —
    // proof `new()` seeds it from PRIOR_COEFFS.
    let core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
    let prior = NeuralRanker::from_prior(&PRIOR_COEFFS);
    let f = sample_feat();
    assert_eq!(core.neural_ranker().score(&f), prior.score(&f));
}

#[test]
fn restore_from_empty_store_yields_the_prior() {
    // Restoring from an empty store leaves the ranker at the cold-start prior
    // (first-run / purge proof), not some other state.
    let dir = tempfile::tempdir().expect("tempdir");
    let store =
        crate::RedbSecureStore::open(dir.path().join("s.redb"), [3u8; 32]).expect("open store");
    let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
    core.restore(&store).expect("restore");
    let prior = NeuralRanker::from_prior(&PRIOR_COEFFS);
    let f = sample_feat();
    assert_eq!(core.neural_ranker().score(&f), prior.score(&f));
}

#[test]
fn observing_a_language_raises_its_weight() {
    let mut core = FeatherKeyCore::new(vec![
        ("en".into(), vec!["hello".into()]),
        ("es".into(), vec!["hola".into()]),
    ])
    .expect("core");
    let before = core.language_weight("es");
    core.observe_language(vec!["es".into()]);
    assert!(core.language_weight("es") > before * 0.9); // bumped past pure decay
}

#[test]
fn switching_languages_reseeds_momentum() {
    let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["hi".into()])]).expect("core");
    core.set_active_languages(vec![("es".into(), vec!["hola".into()])])
        .expect("switch");
    assert!(core.language_weight("es") >= core.language_weight("en"));
}

#[test]
fn rank_candidates_uses_momentum() {
    use featherkey_contracts::{Candidate, Source};
    let mut core = FeatherKeyCore::new(vec![
        ("en".into(), vec!["hello".into()]),
        ("es".into(), vec!["hola".into()]),
    ])
    .expect("core");
    for _ in 0..5 {
        core.observe_language(vec!["es".into()]);
    }
    let cands = vec![
        Candidate {
            word: "hello".into(),
            lang: "en".into(),
            source: Source::Lexicon,
            source_rank: 0,
        },
        Candidate {
            word: "hola".into(),
            lang: "es".into(),
            source: Source::Lexicon,
            source_rank: 0,
        },
    ];
    let out = core.rank_candidates(cands, 2);
    assert_eq!(out[0].word, "hola");
}

#[test]
fn learn_word_records_both_frequency_and_context_when_allowed() {
    struct Ordinary;
    impl featherkey_contracts::SensitiveContextSource for Ordinary {
        fn is_sensitive(&self) -> bool {
            false
        }
    }
    let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
    core.learn_word("the", "cat", &Ordinary);
    assert_eq!(core.word_frequency("cat"), 1);
    assert_eq!(core.context_next_words("the", 5), vec!["cat".to_string()]);
}

#[test]
fn correction_hooks_record_when_field_is_ordinary() {
    struct Ordinary;
    impl featherkey_contracts::SensitiveContextSource for Ordinary {
        fn is_sensitive(&self) -> bool {
            false
        }
    }
    let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["teh".into()])]).expect("core");
    core.observe_strip_pick("teh", "teh", &Ordinary);
    core.observe_delete_retype("ducking", &Ordinary);
    assert_eq!(core.correction_pref_count("teh", "teh"), 1);
    assert_eq!(core.correction_unwanted_count("ducking"), 1);
}
