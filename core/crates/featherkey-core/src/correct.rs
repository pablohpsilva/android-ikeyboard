//! `Correct` use-case (ARCH §9.1): decide whether/how to correct a typed token
//! without ever clobbering a word the user clearly intended (BR-12).
//!
//! The *policy* lives in [`featherkey_autocorrect`] — the crate SEDD §5/§15 and
//! ARCH §5.4 name as the owner of BR-12/BR-15/BR-45. This module is the
//! composition half: it hands that crate the current substrate (every active
//! pack with its bundled ranks, the learned vocabulary, the active-language set,
//! the live momentum) and returns its verdict.
//!
//! The corrector owns its substrate by value, so the façade rebuilds one on
//! demand from its authoritative state rather than caching — correction then
//! always consults the *current* learned vocabulary, with no cache to
//! invalidate. Correction runs at a word boundary, off the sub-millisecond
//! decode hot path, so the clone cost is acceptable at MVP (a materialized read
//! model is a v1.x optimization).

use featherkey_autocorrect::{AvailableCorrection, LexiconPack, NoClobberCorrector};
use featherkey_autocorrect_gate::GateFeatures;
use featherkey_contracts::{Correction, DeviceHints, Token, TypingContext};
use featherkey_locale_manager::LocaleManager;

use crate::error::FeatherKeyError;
use crate::FeatherKeyCore;

/// Apply threshold for the neural autocorrect gate: an available correction is
/// only committed when `winner_confidence + gate.residual(features) >= FLOOR`.
///
/// The winner confidence is the ranker's own (non-normalised) score. Measured
/// on the existing fixtures, every strong winner sits at ≥ 0.64 (the lowest is
/// `core_fuzzy_prior`'s decayed sticky fix at ~0.643; ordinary sticky primary
/// fixes are ~0.749), while a bare non-primary neighbour in a barely-warm
/// language sits near ~0.244 (`positional 0 + ln(~1.045) + lexicon prior 0.2`,
/// no sticky bonus). `0.3` is the smallest round floor that cleanly separates
/// the two: it keeps every strong fixture applying and withholds the weak class
/// at cold start, while leaving only a ~0.056 gap for the gate's bounded
/// residual (±1.5) to close once the user's picks train it up.
///
/// Existing fixtures re-baselined by introducing this floor: **none** — every
/// pre-existing `correct.rs`/`rank_tests`/`composition.rs` fixture's winner sits
/// at ≥ 0.64 confidence and continues to apply unchanged. The only newly-withheld
/// class is the deliberately-weak `core_with_weak_only` fixture added here.
pub(crate) const AUTOCORRECT_FLOOR: f64 = 0.3;

/// The most recent gated correction decision, bounded to one snapshot: the
/// features that were scored, the winner that was weighed, and whether it was
/// applied. Written by every [`choose_correction`](FeatherKeyCore::choose_correction);
/// read by the observe-outcome trainer (Task 9) to reinforce the gate, and by
/// this module's gate tests today. The lib build has no reader until Task 9
/// wires `observe_correction_outcome`, so its fields are dead there for now.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone)]
pub(crate) struct LastCorrection {
    pub(crate) features: GateFeatures,
    pub(crate) winner: String,
    pub(crate) applied: bool,
}

impl FeatherKeyCore {
    /// Multilingual, momentum-aware correction, **gated** by the per-user neural
    /// autocorrect gate — the one correction entry point.
    ///
    /// `device_known` is what the platform spell-checker recognises (each such
    /// word is intended, BR-12) and `device_cands` its candidate spellings; both
    /// are gathered by the shell because the core cannot see the OS dictionary.
    ///
    /// The no-clobber veto (BR-12) is absolute and runs first: a vetoed or
    /// no-candidate token is returned unchanged with the gate never consulted.
    /// Only when a real winner is available does the gate weigh it against
    /// [`AUTOCORRECT_FLOOR`]; a withheld winner is surfaced in
    /// [`Correction::withheld`] for the shell's counterfactual signal, and the
    /// whole decision is cached in `last_correction`.
    ///
    /// # Errors
    /// [`FeatherKeyError::Locale`]/[`NoLanguages`] if the active set cannot form a
    /// locale manager (structurally prevented by the constructor, surfaced not panicked).
    pub fn choose_correction(
        &mut self,
        text: &str,
        device_known: &[String],
        device_cands: Vec<featherkey_contracts::Candidate>,
    ) -> Result<Correction, FeatherKeyError> {
        let corrector = self.build_corrector()?;
        let assessment = corrector.assess(
            &Token {
                text: text.to_owned(),
            },
            &TypingContext::default(),
            &DeviceHints {
                known: device_known.to_vec(),
                candidates: device_cands,
            },
        );
        // No winner to weigh (BR-12 veto or no candidate): the gate is never
        // consulted and the no-clobber outcome stands verbatim.
        let Some(av) = assessment.available else {
            return Ok(assessment.correction);
        };
        let features = self.gate_features(text, &av);
        let applied =
            (av.winner_confidence + self.autocorrect_gate.residual(&features)) >= AUTOCORRECT_FLOOR;
        self.last_correction = Some(LastCorrection {
            features,
            winner: av.winner.clone(),
            applied,
        });
        Ok(Correction {
            primary: if applied {
                av.winner.clone()
            } else {
                text.to_owned()
            },
            applied,
            alternatives: if applied {
                assessment.correction.alternatives
            } else {
                Vec::new()
            },
            withheld: if applied { None } else { Some(av.winner) },
        })
    }

    /// Assemble the [`GateFeatures`] for a weighed correction winner.
    fn gate_features(&self, text: &str, av: &AvailableCorrection) -> GateFeatures {
        GateFeatures {
            edit_distance: av.edit_distance as f32,
            winner_confidence: av.winner_confidence as f32,
            dict_rank_norm: av.winner_dict_rank.map_or(0.0, |r| 1.0 / (1.0 + r as f32)),
            typed_len_norm: (text.chars().count() as f32 / 16.0).min(1.0),
            momentum_weight: self.momentum.weight_of(&av.winner_lang).ln() as f32,
        }
    }

    /// Assemble a corrector from a snapshot of the current state: every active
    /// pack (lexicon + bundled ranks), the live learned vocabulary, a locale
    /// manager over the active languages, and the current language momentum.
    fn build_corrector(&self) -> Result<NoClobberCorrector, FeatherKeyError> {
        let locales = LocaleManager::new(self.locale_packs())?;
        let packs = self
            .packs
            .iter()
            .map(|p| LexiconPack {
                lang: p.lang.as_str().to_owned(),
                dict: p.dict.clone(),
                rank: p.rank.clone(),
            })
            .collect();
        Ok(NoClobberCorrector::new(
            packs,
            self.personalization.clone(),
            locales,
            self.momentum.clone(),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::FeatherKeyCore;

    #[test]
    fn a_word_only_the_device_knows_is_not_clobbered() {
        let mut core =
            FeatherKeyCore::new(vec![("en".into(), vec!["hello".into()])]).expect("core");
        let got = core
            .choose_correction("privet", &["privet".into()], vec![])
            .expect("ok");
        assert_eq!(got.primary, "privet");
        assert!(!got.applied);
    }

    #[test]
    fn a_non_primary_typo_is_corrected_in_its_own_language() {
        let mut core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["cat".into(), "cot".into()]),
            ("es".into(), vec!["gato".into(), "pato".into()]),
        ])
        .expect("core");
        for _ in 0..5 {
            core.observe_language(vec!["es".into()]);
        } // writing Spanish
          // "rato" (typo, not a real word here) is one edit from es "gato"/"pato";
          // momentum + es-only candidates make the es fix win. (Brief wrote the input
          // as "gato", but that is itself in the es lexicon and would trip the
          // no-clobber rule before any correction — so the intended typo is used.)
        let got = core.choose_correction("rato", &[], vec![]).expect("ok");
        assert!(got.applied);
        assert_eq!(got.primary, "gato");
    }

    #[test]
    fn a_real_word_in_any_active_language_is_left_alone() {
        let mut core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["hello".into()]),
            ("es".into(), vec!["hola".into()]),
        ])
        .expect("core");
        let got = core.choose_correction("hola", &[], vec![]).expect("ok");
        assert!(!got.applied);
        assert_eq!(got.primary, "hola");
    }

    #[test]
    fn the_sticky_fix_holds_under_mild_momentum_and_flips_under_strong_momentum() {
        // "cit" is one edit from "cat" (en) and "cot" (es). Primary is en, so the
        // sticky fix is "cat". With no/mild momentum CORE_FUZZY_PRIOR keeps "cat";
        // sustained Spanish eventually overtakes the bonus and flips to "cot".
        let mut core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["cat".into()]),
            ("es".into(), vec!["cot".into()]),
        ])
        .expect("core");
        let mild = core.choose_correction("cit", &[], vec![]).expect("ok");
        assert_eq!(mild.primary, "cat"); // sticky primary fix survives mild momentum
        for _ in 0..20 {
            core.observe_language(vec!["es".into()]);
        }
        let strong = core.choose_correction("cit", &[], vec![]).expect("ok");
        assert_eq!(strong.primary, "cot"); // strong Spanish momentum overrides the bonus
    }

    #[test]
    fn core_fuzzy_prior_keeps_the_primary_fix_against_a_slightly_hotter_language() {
        // "cit" is one edit from "cat" (en, primary) and "cot" (es). After ONE Spanish
        // word, es's weight (~1.045) edges just above en's decayed head-start (~0.945) —
        // so WITHOUT the sticky bonus "cot" would win. CORE_FUZZY_PRIOR must keep the
        // primary fix "cat". This test fails if the const regresses to 0, locking the dial.
        let mut core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["cat".into()]),
            ("es".into(), vec!["cot".into()]),
        ])
        .expect("core");
        core.observe_language(vec!["es".into()]); // one Spanish word: es edges just ahead
        let got = core.choose_correction("cit", &[], vec![]).expect("ok");
        assert_eq!(got.primary, "cat");
    }

    #[test]
    fn alternatives_do_not_repeat_a_cognate_across_languages() {
        // "cit" fuzzes to "cot" in BOTH en and es (a cognate). The winner is the
        // primary sticky fix "cat"; "cot" must appear at most once among the
        // alternatives, not once per language — the spec requires distinct words.
        let mut core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["cat".into(), "cot".into()]),
            ("es".into(), vec!["cot".into()]),
        ])
        .expect("core");
        let got = core.choose_correction("cit", &[], vec![]).expect("ok");
        assert!(got.applied);
        assert_eq!(got.primary, "cat");
        let distinct: std::collections::HashSet<&String> = got.alternatives.iter().collect();
        assert_eq!(
            distinct.len(),
            got.alternatives.len(),
            "alternatives must be distinct words: {:?}",
            got.alternatives
        );
        // ...and never echo the winner itself.
        assert!(!got.alternatives.contains(&"cat".to_string()));
    }

    #[test]
    fn a_nonword_with_no_neighbour_in_any_language_is_left_unchanged() {
        // "qqqq" is far from every dictionary word and unknown to the device:
        // no candidates -> nothing to correct, returned as typed.
        let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into(), "dog".into()])])
            .expect("core");
        let got = core.choose_correction("qqqq", &[], vec![]).expect("ok");
        assert_eq!(got.primary, "qqqq");
        assert!(!got.applied);
        assert!(got.alternatives.is_empty());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod rank_tests {
    use crate::FeatherKeyCore;

    /// Activation order IS frequency order (`build_packs`): "cat" is the
    /// commonest word here, "bat" the rarest — but "bat" sorts first
    /// alphabetically. "xat" is one substitution from bat/cat/hat.
    fn en_core() -> FeatherKeyCore {
        FeatherKeyCore::new(vec![(
            "en".into(),
            vec!["cat".into(), "dog".into(), "hat".into(), "bat".into()],
        )])
        .expect("core")
    }

    #[test]
    fn a_typo_is_corrected_to_the_commonest_neighbour() {
        let got = en_core().choose_correction("xat", &[], vec![]).expect("ok");
        assert!(got.applied);
        assert_eq!(got.primary, "cat");
    }

    #[test]
    fn correction_alternatives_are_frequency_ordered() {
        let got = en_core().choose_correction("xat", &[], vec![]).expect("ok");
        assert_eq!(got.alternatives, vec!["hat".to_string(), "bat".to_string()]);
    }

    #[test]
    fn momentum_still_decides_across_languages() {
        let mut core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["cat".into(), "dog".into()]),
            ("es".into(), vec!["cas".into(), "gato".into()]),
        ])
        .expect("core");
        for _ in 0..5 {
            core.observe_language(vec!["es".into()]);
        }
        // "cax" is one substitution from en "cat" and es "cas", and is not a
        // prefix of either (a prefix would be treated as intended, BR-12).
        let got = core.choose_correction("cax", &[], vec![]).expect("ok");
        assert_eq!(got.primary, "cas");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod gate_tests {
    use crate::FeatherKeyCore;

    /// A high-confidence single-language fixture: "xat" is one substitution from
    /// the primary sticky fix "cat", whose winner confidence (~0.749) is well
    /// above `AUTOCORRECT_FLOOR`, so the gate applies it even at cold start.
    fn en_core() -> FeatherKeyCore {
        FeatherKeyCore::new(vec![("en".into(), vec!["cat".into(), "dog".into()])]).expect("core")
    }

    /// A deliberately-weak fixture whose winner earns no sticky bonus. The
    /// primary `en` has no neighbour of "xöq", so the sticky `CORE_FUZZY_PRIOR`
    /// falls back to the *first* lexicon candidate — the floor-weight decoy `de`
    /// fix "xoq" (ö→o) — which then loses to the warmed winner. `fr`'s fix "xöz"
    /// (q→z) is *not* the sticky one, so after one observed French word warms
    /// `fr` to weight ~1.045 its winner confidence is only ~0.244
    /// (`ln(1.045) + lexicon prior 0.2`, no bonus) — just under the floor. This
    /// is the class the gate withholds at cold start.
    fn core_with_weak_only() -> FeatherKeyCore {
        let mut core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["hello".into()]),
            ("de".into(), vec!["xoq".into()]),
            ("fr".into(), vec!["xöz".into()]),
        ])
        .expect("core");
        core.observe_language(vec!["fr".into()]); // warm fr just off the floor
        core
    }

    #[test]
    fn cold_start_below_floor_is_withheld_and_cached() {
        let mut core = core_with_weak_only();
        let got = core.choose_correction("xöq", &[], vec![]).expect("ok");
        // The weak winner is withheld: the token is returned as typed, but the
        // withheld winner is surfaced for the counterfactual signal.
        assert!(!got.applied);
        assert_eq!(got.primary, "xöq");
        assert!(got.alternatives.is_empty());
        assert_eq!(got.withheld.as_deref(), Some("xöz"));
        // ...and the whole decision is cached, applied == false.
        assert_eq!(
            core.last_correction.as_ref().map(|l| l.applied),
            Some(false)
        );
        assert_eq!(
            core.last_correction.as_ref().map(|l| l.winner.as_str()),
            Some("xöz")
        );
    }

    #[test]
    fn a_strong_correction_still_applies_at_cold_start() {
        let mut core = en_core();
        let got = core.choose_correction("xat", &[], vec![]).expect("ok");
        assert!(got.applied);
        assert_eq!(got.primary, "cat");
        assert!(got.withheld.is_none());
    }

    #[test]
    fn a_trained_up_gate_applies_a_previously_withheld_correction() {
        let mut core = core_with_weak_only();
        // First pass: withheld, and it caches the exact features that were scored.
        let first = core.choose_correction("xöq", &[], vec![]).expect("ok");
        assert!(!first.applied);
        let f = core
            .last_correction
            .as_ref()
            .map(|l| l.features)
            .expect("cached features");
        // Reinforce the gate toward "apply" for these features.
        for _ in 0..200 {
            core.autocorrect_gate.reinforce(&f, 1.0, 0.05);
        }
        let got = core.choose_correction("xöq", &[], vec![]).expect("ok");
        assert!(got.applied, "residual lifted it over the floor");
        assert_eq!(got.primary, "xöz");
        assert!(got.withheld.is_none());
    }
}
