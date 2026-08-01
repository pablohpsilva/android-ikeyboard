//! Unit tests for `learn.rs` (`LearnFromInput` + `ManageUserDictionary`).
//! Kept as a file-based module (ARCH §6: no god-files) so `learn.rs` stays
//! under the 500-line fitness limit while every test remains an in-crate unit
//! test with visibility into `learn`'s private items and `FeatherKeyCore`'s
//! `#[cfg(test)] pub(crate)` accessors (e.g. `tap_warp()`, `layout()`).

use super::*;
use featherkey_autocorrect_gate::GateFeatures;
use featherkey_contracts::Candidate;
use featherkey_neural_ranker::RankFeatures;

struct Ordinary;
impl SensitiveContextSource for Ordinary {
    fn is_sensitive(&self) -> bool {
        false
    }
}

struct Sensitive;
impl SensitiveContextSource for Sensitive {
    fn is_sensitive(&self) -> bool {
        true
    }
}

fn correction_core() -> FeatherKeyCore {
    FeatherKeyCore::new(vec![(
        "en".into(),
        vec!["cat".into(), "dog".into(), "hat".into(), "bat".into()],
    )])
    .expect("core")
}

/// A core whose "te…" completions are `tea` < `team` < `teach`.
fn core() -> FeatherKeyCore {
    FeatherKeyCore::new(vec![(
        "en".to_owned(),
        vec!["tea".to_owned(), "team".to_owned(), "teach".to_owned()],
    )])
    .expect("valid single-language core")
}

fn no_device() -> Vec<Candidate> {
    Vec::new()
}

/// A core over a full QWERTY alpha page (the "en" primary tag), for tap-warp
/// training probes that need `layout().center_of(..)` to resolve real letters.
fn core_qwerty() -> FeatherKeyCore {
    FeatherKeyCore::new(vec![(
        "en".into(),
        vec!["cat".into(), "dog".into(), "hat".into(), "bat".into()],
    )])
    .expect("qwerty core")
}

fn non_sensitive() -> Ordinary {
    Ordinary
}

fn sensitive() -> Sensitive {
    Sensitive
}

/// A tempdir-backed `RedbSecureStore` for persist/restore round-trip tests
/// (the crate has no literal in-memory store; this mirrors the pattern
/// `the_autocorrect_gate_survives_persist_and_restore` already uses). The
/// tempdir is leaked via `keep` so the store outlives the guard.
fn mem_store() -> crate::RedbSecureStore {
    let dir = tempfile::tempdir().expect("tempdir").keep();
    crate::RedbSecureStore::open(dir.join("warp.redb"), [7u8; 32]).expect("open store")
}

/// A representative feature vector for weight-comparison probes (arbitrary
/// values that exercise every slot, so any weight change moves the score).
fn probe() -> RankFeatures {
    RankFeatures {
        positional: -0.7,
        ln_momentum: 0.2,
        is_lexicon: 1.0,
        is_device: 0.0,
        correction_promote: 0.1,
        correction_demote: 0.0,
        spatial: 0.3,
        // stopgap; production never produces anything but 0.0 (Task 5 fills it in)
        lm_logprob: 0.0,
    }
}

fn probe_score(core: &FeatherKeyCore) -> f64 {
    core.neural_ranker().score(&probe())
}

/// A representative feature vector for the autocorrect gate (arbitrary
/// values that exercise every slot, so a weight change moves the residual).
fn gate_probe() -> GateFeatures {
    GateFeatures {
        edit_distance: 1.0,
        winner_confidence: 0.6,
        dict_rank_norm: 0.3,
        typed_len_norm: 0.4,
        momentum_weight: 0.1,
    }
}

#[test]
fn a_pick_that_also_commits_trains_only_once() {
    // Control: exactly one strip pick after one ranked query → one reinforce.
    let mut once = core();
    let _ = once.rank_suggestions("", "te", no_device());
    once.observe_strip_pick("te", "team", &Ordinary);

    // Test: the same pick, then the commit of the same word. The pick
    // consumed the only snapshot, so `learn_word` finds none and trains
    // nothing — the net weights must match the single-reinforce control.
    let mut pick_then_commit = core();
    let _ = pick_then_commit.rank_suggestions("", "te", no_device());
    pick_then_commit.observe_strip_pick("te", "team", &Ordinary);
    pick_then_commit.learn_word("", "team", &Ordinary);

    let untrained = probe_score(&core());
    assert_ne!(
        probe_score(&once),
        untrained,
        "the single pick must actually train the ranker (else the test is vacuous)"
    );
    assert_eq!(
        probe_score(&pick_then_commit),
        probe_score(&once),
        "pick + commit must train exactly once (snapshot consumed by the pick)"
    );

    // And two picks (each refreshing the snapshot) train twice — proving the
    // equality above is the consumed snapshot, not an inert second update.
    let mut twice = core();
    let _ = twice.rank_suggestions("", "te", no_device());
    twice.observe_strip_pick("te", "team", &Ordinary);
    let _ = twice.rank_suggestions("", "te", no_device());
    twice.observe_strip_pick("te", "team", &Ordinary);
    assert_ne!(
        probe_score(&twice),
        probe_score(&once),
        "two full pick rounds must train twice, unlike one pick + commit"
    );
}

#[test]
fn reinforce_from_pick_consumes_the_matching_snapshot() {
    let mut fk = core();
    let _ = fk.rank_suggestions("", "te", no_device());
    assert!(fk.last_ranked().is_some());
    let before = probe_score(&fk);

    fk.reinforce_from_pick("te", "team");

    assert!(
        fk.last_ranked().is_none(),
        "a successful reinforce consumes (clears) the snapshot"
    );
    assert_ne!(
        before,
        probe_score(&fk),
        "the matching pick trained the net"
    );
}

#[test]
fn reinforce_from_pick_ignores_a_prefix_mismatch() {
    let mut fk = core();
    let _ = fk.rank_suggestions("", "te", no_device());
    let before = probe_score(&fk);

    // Snapshot prefix is "te"; a pick reported under a different prefix does
    // not train and leaves the snapshot intact for its real prefix.
    fk.reinforce_from_pick("xy", "team");

    assert!(fk.last_ranked().is_some(), "a mismatch leaves the snapshot");
    assert_eq!(before, probe_score(&fk), "a prefix mismatch trains nothing");
}

#[test]
fn reinforce_from_pick_ignores_a_word_not_in_the_shown_set() {
    let mut fk = core();
    let _ = fk.rank_suggestions("", "te", no_device());
    let before = probe_score(&fk);

    // "zzz" was never shown for this prefix: no chosen index, no training.
    fk.reinforce_from_pick("te", "zzz");

    assert!(
        fk.last_ranked().is_some(),
        "an unshown word leaves the snapshot"
    );
    assert_eq!(before, probe_score(&fk), "an unshown word trains nothing");
}

#[test]
fn reinforce_from_pick_is_a_noop_without_a_snapshot() {
    let mut fk = core();
    // No rank_suggestions call, so there is no cached snapshot at all.
    assert!(fk.last_ranked().is_none());
    let before = probe_score(&fk);

    fk.reinforce_from_pick("te", "team");

    assert_eq!(before, probe_score(&fk), "no snapshot means no training");
}

#[test]
fn a_systematic_bias_generalizes_to_an_untapped_key() {
    // Bias must EXCEED the half-key distance (~50px on a ~100px qwerty key) or an
    // unbiased tap already lands on the intended key and proves nothing. At +60px a
    // tap right of 'f' is nearer 'g' (unbiased mis-resolves). The bounded warp
    // (±WARP_BOUND) pulls it back onto 'f' — even a partial correction flips the
    // argmin, since f+60 − ~30px ≈ f+30 is nearer 'f' (dist 30) than 'g' (dist 70).
    //
    // Training set: the top and bottom rows, deliberately **excluding every home-row
    // key** ('d' and 'g' above all — f's direct left/right neighbours) and 'f' itself.
    // `decode` biases each key's own effective centre by *that key's own*
    // `TouchModel` offset (input-decoder's per-key mean-offset re-centring, wired
    // before this task), independently of the warp. Training 'd'/'g' directly would
    // shift *their own* decode centres away from the touch and resolve 'f' through
    // that pre-existing per-key mechanism alone — passing even with the warp left
    // untrained, and proving nothing about this task's generalization. Training only
    // distant keys leaves 'd', 'g', and 'f' at `TouchModel` offset (0,0), so only the
    // warp (a function of normalized position, shared across keys) can move the
    // decode here — see `a_systematic_bias_would_not_resolve_via_touch_model_alone`.
    //
    // Single pass, not repeated: `observe_tap` reads `mean_k` *before* folding
    // (no-double-correction, design §6), so a constant offset makes `mean_k` equal
    // `dx` from the 2nd observation of a given key onward — every further repeat of
    // an *already-seen* key trains the warp toward a target of (0, 0) there, eroding
    // the 1st pass's signal rather than adding to it. One observation per (distinct)
    // key is what maximizes the retained generalization signal under this rule.
    let mut c = core_qwerty();
    let f = c.layout().center_of('f').expect("f exists");
    // Sanity: unbiased decode of the biased tap resolves the WRONG key.
    assert_ne!(
        c.decode(f.x + 60.0, f.y).expect("decode").best.as_deref(),
        Some("f")
    );
    // Teach the same +60px rightward bias on keys far from 'f' (top + bottom rows),
    // never 'f' and never its direct row-neighbours 'd'/'g'.
    for ch in [
        'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', 'z', 'x', 'c', 'v', 'b', 'n', 'm',
    ] {
        c.observe_tap(ch, 60.0, 0.0, &non_sensitive())
            .expect("observe");
    }
    let got = c.decode(f.x + 60.0, f.y).expect("decode");
    assert_eq!(
        got.best.as_deref(),
        Some("f"),
        "the learned warp must generalize the bias to the never-tapped 'f'"
    );
}

#[test]
fn a_systematic_bias_would_not_resolve_via_touch_model_alone() {
    // Companion proof for the test above: after that exact training set, 'f' and its
    // direct row-neighbours 'd'/'g' still carry a (0.0, 0.0) `TouchModel` offset, so
    // the earlier test's resolution to 'f' cannot be explained by the pre-existing
    // per-key `TouchModel` re-centring — only the warp moved the decode.
    let mut c = core_qwerty();
    for ch in [
        'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', 'z', 'x', 'c', 'v', 'b', 'n', 'm',
    ] {
        c.observe_tap(ch, 60.0, 0.0, &non_sensitive())
            .expect("observe");
    }
    for key in ['f', 'd', 'g'] {
        assert_eq!(
            c.tap_offsets().iter().find(|(k, _, _)| k == &key.to_string()),
            None,
            "'{key}' must remain unobserved by TouchModel for the generalization test to be conclusive"
        );
    }
}

#[test]
fn a_converged_key_gets_no_extra_shift() {
    let mut c = core_qwerty();
    for _ in 0..100 {
        c.observe_tap('j', 0.0, 0.0, &non_sensitive())
            .expect("observe");
    } // on-centre
    let j = c.layout().center_of('j').expect("j exists");
    let (nx, ny) = c.layout().normalize(j.x, j.y);
    let (dx, dy) = c.tap_warp().warp(nx, ny);
    assert!(
        dx.abs() < 3.0 && dy.abs() < 3.0,
        "no double-correction: {dx},{dy}"
    );
}

#[test]
fn a_sensitive_field_does_not_train_the_warp() {
    let mut c = core_qwerty();
    let j = c.layout().center_of('j').expect("j exists");
    let (nx, ny) = c.layout().normalize(j.x, j.y);
    let before = c.tap_warp().warp(nx, ny);
    for _ in 0..50 {
        c.observe_tap('j', 25.0, 10.0, &sensitive())
            .expect("observe");
    }
    assert_eq!(
        c.tap_warp().warp(nx, ny),
        before,
        "sensitive taps must not train"
    );
}

#[test]
fn the_tap_warp_survives_persist_and_restore() {
    let store = mem_store();
    let mut c = core_qwerty();
    for _ in 0..60 {
        c.observe_tap('k', 18.0, -6.0, &non_sensitive())
            .expect("observe");
    }
    c.persist(&store).expect("persist");
    let mut restored = core_qwerty();
    restored.restore(&store).expect("restore");
    let k = c.layout().center_of('k').expect("k exists");
    let (nx, ny) = c.layout().normalize(k.x, k.y);
    assert_eq!(restored.tap_warp().warp(nx, ny), c.tap_warp().warp(nx, ny));
}

#[test]
fn the_autocorrect_gate_survives_persist_and_restore() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store =
        crate::RedbSecureStore::open(dir.path().join("gate.redb"), [11u8; 32]).expect("open store");

    // Drive one suppression so the gate diverges from its cold-start prior
    // (Task 9 wires the real observe call; here the crate-internal field is
    // reached directly, mirroring how the neural-ranker restore test reaches
    // `neural_ranker()`).
    let mut fk = core();
    let f = gate_probe();
    for _ in 0..50 {
        fk.autocorrect_gate.reinforce(&f, -1.0, 0.05);
    }
    fk.persist(&store).expect("persist");

    let mut restored = core();
    restored.restore(&store).expect("restore");
    assert!(
        (restored.autocorrect_gate.residual(&f) - fk.autocorrect_gate.residual(&f)).abs() < 1e-6,
        "the gate's learned residual must survive persist -> restore"
    );
}

#[test]
fn revert_suppresses_a_repeatedly_reverted_correction() {
    let mut fk = correction_core();
    let _ = fk.choose_correction("xat", &[], vec![]).expect("ok");
    // The feature-sensitive prior converges smoothly: a strong correction
    // crosses the floor in ~4 reverts (product-approved 3–5); 8 is the margin.
    for _ in 0..8 {
        let _ = fk.choose_correction("xat", &[], vec![]).expect("ok");
        fk.observe_autocorrect_outcome(AutocorrectOutcome::Reverted, &Ordinary);
    }
    let got = fk.choose_correction("xat", &[], vec![]).expect("ok");
    assert!(!got.applied, "the user's reverts pushed it under the floor");
}

#[test]
fn a_sensitive_field_records_nothing() {
    let mut fk = correction_core();
    let _ = fk.choose_correction("xat", &[], vec![]).expect("ok");
    let features = fk.last_correction.as_ref().expect("cached").features;
    let before = fk.autocorrect_gate.residual(&features);
    fk.observe_autocorrect_outcome(AutocorrectOutcome::Reverted, &Sensitive);
    let after = fk.autocorrect_gate.residual(&features);
    assert_eq!(before, after, "gate must not change");
}
