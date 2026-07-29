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

use featherkey_autocorrect::{LexiconPack, NoClobberCorrector};
use featherkey_contracts::{AutoCorrect, Correction, DeviceHints, Token, TypingContext};
use featherkey_locale_manager::LocaleManager;

use crate::error::FeatherKeyError;
use crate::FeatherKeyCore;

impl FeatherKeyCore {
    /// Multilingual, momentum-aware correction — the one correction entry point.
    ///
    /// `device_known` is what the platform spell-checker recognises (each such
    /// word is intended, BR-12) and `device_cands` its candidate spellings; both
    /// are gathered by the shell because the core cannot see the OS dictionary.
    ///
    /// # Errors
    /// [`FeatherKeyError::Locale`]/[`NoLanguages`] if the active set cannot form a
    /// locale manager (structurally prevented by the constructor, surfaced not panicked).
    pub fn choose_correction(
        &self,
        text: &str,
        device_known: &[String],
        device_cands: Vec<featherkey_contracts::Candidate>,
    ) -> Result<Correction, FeatherKeyError> {
        let corrector = self.build_corrector()?;
        Ok(corrector.correct(
            &Token {
                text: text.to_owned(),
            },
            &TypingContext::default(),
            &DeviceHints {
                known: device_known.to_vec(),
                candidates: device_cands,
            },
        ))
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
        let core = FeatherKeyCore::new(vec![("en".into(), vec!["hello".into()])]).expect("core");
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
        let core = FeatherKeyCore::new(vec![
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
        let core = FeatherKeyCore::new(vec![
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
        let core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into(), "dog".into()])])
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
