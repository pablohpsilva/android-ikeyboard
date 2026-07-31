//! Task 12 — the personalized autocorrect gate, end-to-end via the public API.
//!
//! Pins the four BDD scenarios in `core/features/autocorrect-gate.feature`
//! (`@BR-12`): a strong correction still applies at cold start; repeated
//! reverts of one correction suppress it; a known/intended word is never
//! clobbered no matter how eager the gate has become (the no-clobber veto is
//! absolute and runs *before* the gate is ever consulted); and a sensitive
//! field records nothing, so the identical revert sequence that suppresses the
//! correction in an ordinary field leaves it fully applying there.
//!
//! Unlike the module-internal unit tests in `correct.rs`/`learn.rs` (which
//! reach `pub(crate)` fields for fine-grained assertions), everything here goes
//! through `FeatherKeyCore`'s public surface only — `choose_correction` and
//! `observe_autocorrect_outcome` — the same surface the shell (and the UniFFI
//! layer) calls.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use featherkey_core::{AutocorrectOutcome, FeatherKeyCore, SensitiveContextSource};

/// An ordinary field: outcomes are recorded and train the gate.
struct Ordinary;
impl SensitiveContextSource for Ordinary {
    fn is_sensitive(&self) -> bool {
        false
    }
}

/// A sensitive field (password/OTP): outcomes MUST be dropped (E-2, BR-26).
struct Sensitive;
impl SensitiveContextSource for Sensitive {
    fn is_sensitive(&self) -> bool {
        true
    }
}

/// The Background: a frequency-ordered English lexicon ("cat" is the commonest
/// word, "bat" the rarest). "xat" is one substitution from all four, so the
/// commonest-neighbour rule (and the sticky fix) both pick "cat" — a
/// high-confidence winner (~0.749), well above `AUTOCORRECT_FLOOR` (0.3), so it
/// applies even at cold start (mirrors `correct::gate_tests::en_core`).
fn core() -> FeatherKeyCore {
    FeatherKeyCore::new(vec![(
        "en".to_owned(),
        vec![
            "cat".to_owned(),
            "dog".to_owned(),
            "hat".to_owned(),
            "bat".to_owned(),
        ],
    )])
    .expect("valid single-language core")
}

#[test]
fn a_strong_correction_still_applies_at_cold_start() {
    let mut fk = core();
    let got = fk.choose_correction("xat", &[], vec![]).expect("ok");
    assert!(
        got.applied,
        "a high-confidence correction must apply at cold start"
    );
    assert_eq!(got.primary, "cat");
}

#[test]
fn repeatedly_reverting_one_correction_suppresses_it() {
    let mut fk = core();
    // The feature-sensitive prior converges smoothly: a strong correction
    // crosses the floor in ~4 reverts (product-approved 3-5); 8 is the margin
    // (mirrors `learn.rs::revert_suppresses_a_repeatedly_reverted_correction`,
    // proven here through the public API instead of a crate-internal field).
    for _ in 0..8 {
        let _ = fk.choose_correction("xat", &[], vec![]).expect("ok");
        fk.observe_autocorrect_outcome(AutocorrectOutcome::Reverted, &Ordinary);
    }
    let got = fk.choose_correction("xat", &[], vec![]).expect("ok");
    assert!(
        !got.applied,
        "the user's reverts must push this specific correction under the floor"
    );
    assert_eq!(
        got.primary, "xat",
        "an unapplied correction leaves the typed token untouched"
    );
    assert_eq!(
        got.withheld.as_deref(),
        Some("cat"),
        "the withheld winner is still surfaced for the counterfactual signal"
    );
}

#[test]
fn a_known_word_is_never_clobbered_no_matter_how_eager_the_gate_has_become() {
    let mut fk = core();
    // Train the gate hard toward "apply": the gate is a single per-user MLP
    // shared across every decision, so a long run of confirming (Reached)
    // outcomes on an unrelated correction makes it about as eager as it can get.
    for _ in 0..200 {
        let _ = fk.choose_correction("xat", &[], vec![]).expect("ok");
        fk.observe_autocorrect_outcome(AutocorrectOutcome::Reached, &Ordinary);
    }
    // "cat" IS the intended word: the no-clobber veto (BR-12) is absolute and
    // runs before the gate is even consulted (`choose_correction` returns the
    // no-clobber outcome verbatim whenever `assessment.available` is `None`),
    // so no amount of gate training can make this apply.
    let got = fk.choose_correction("cat", &[], vec![]).expect("ok");
    assert!(
        !got.applied,
        "a word the user clearly intended must never be corrected, however eager the gate is"
    );
    assert_eq!(got.primary, "cat");
}

#[test]
fn a_sensitive_field_records_nothing_for_the_gate() {
    let mut fk = core();
    // The identical 8-revert sequence that suppresses the correction in an
    // ordinary field (see `repeatedly_reverting_one_correction_suppresses_it`)
    // must leave it fully applying here — proving the sensitive-field gate
    // drops the signal entirely, rather than merely training it more slowly.
    for _ in 0..8 {
        let _ = fk.choose_correction("xat", &[], vec![]).expect("ok");
        fk.observe_autocorrect_outcome(AutocorrectOutcome::Reverted, &Sensitive);
    }
    let got = fk.choose_correction("xat", &[], vec![]).expect("ok");
    assert!(
        got.applied,
        "a sensitive-field revert must not move the gate (else it would have suppressed this too)"
    );
    assert_eq!(got.primary, "cat");
}
