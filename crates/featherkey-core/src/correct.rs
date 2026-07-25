//! `Correct` use-case (ARCH §9.1): decide whether/how to correct a typed token
//! without ever clobbering a word the user clearly intended (BR-12).
//!
//! The corrector owns its lexical substrate by value, so the façade rebuilds one
//! on demand from its authoritative state — a clone of each lexicon, a clone of
//! the current [`Personalization`] snapshot, and a fresh [`LocaleManager`] over
//! the active languages. Rebuilding (rather than caching) keeps correction
//! consulting the *current* learned vocabulary with no cache to invalidate;
//! correction is off the sub-millisecond decode hot path, so the clone cost is
//! acceptable at MVP (a materialized read model is a v1.x optimization).

use featherkey_autocorrect::NoClobberCorrector;
use featherkey_contracts::{AutoCorrect, Correction, Token, TypingContext};
use featherkey_locale_manager::LocaleManager;

use crate::error::FeatherKeyError;
use crate::FeatherKeyCore;

/// Stickiness of the trusted edit-distance fix versus the momentum nudge. The
/// primary-language closest fix carries this bonus, so an unambiguous typo keeps
/// its fix unless a competing-language candidate's momentum-weighted score
/// overtakes it. High ⇒ legacy behaviour; low ⇒ momentum flips corrections sooner.
pub const CORE_FUZZY_PRIOR: f64 = 0.5;

impl FeatherKeyCore {
    /// Multilingual, momentum-aware correction. See the design spec §Correction.
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
        use featherkey_contracts::{Candidate, Source};
        let locales = LocaleManager::new(self.packs.clone())?;
        let lower = text.to_lowercase();

        // (1) No-clobber: real in any active language, known to the user, or in the
        // device's known set. Empty text has nothing to correct.
        let known_device = device_known.iter().any(|w| w.eq_ignore_ascii_case(text));
        if text.is_empty()
            || self.personalization.is_known(text)
            || locales.detect(text).is_some()
            || locales.detect(&lower).is_some()
            || known_device
        {
            return Ok(Correction {
                primary: text.to_owned(),
                alternatives: Vec::new(),
                applied: false,
            });
        }

        // (2) Candidates: all-language fuzzy (per-language rank) ∪ device candidates.
        let mut cands: Vec<Candidate> = Vec::new();
        let mut per_lang_rank: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        for (id, w) in locales.fuzzy_all(text) {
            let r = per_lang_rank.entry(id.as_str().to_owned()).or_insert(0);
            cands.push(Candidate {
                word: w,
                lang: id.as_str().to_owned(),
                source: Source::Lexicon,
                source_rank: *r,
            });
            *r += 1;
        }
        cands.extend(device_cands);
        if cands.is_empty() {
            return Ok(Correction {
                primary: text.to_owned(),
                alternatives: Vec::new(),
                applied: false,
            });
        }

        // (3) The sticky fix = the primary language's closest lexicon neighbour
        // (fallback: the first lexicon candidate). It carries CORE_FUZZY_PRIOR so a
        // trusted fix holds unless a competing candidate's momentum score overtakes it.
        let primary = self.packs.first().map(|(id, _)| id.as_str().to_owned());
        let sticky = cands
            .iter()
            .position(|c| {
                c.source == Source::Lexicon
                    && c.source_rank == 0
                    && Some(&c.lang) == primary.as_ref()
            })
            .or_else(|| cands.iter().position(|c| c.source == Source::Lexicon));

        let mut scored: Vec<(usize, f64)> = cands
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut s = featherkey_candidate_ranker::score(c, &self.momentum);
                if Some(i) == sticky {
                    s += CORE_FUZZY_PRIOR;
                }
                (i, s)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let winner = cands[scored[0].0].word.clone();
        let applied = winner != text;
        let alternatives: Vec<String> = if applied {
            // Distinct words only: pre-seed the winner so it is filtered, then let
            // the set drop any cognate already emitted for another language (the
            // same word can be a fuzzy neighbour in several active languages).
            let mut seen = std::collections::HashSet::new();
            seen.insert(winner.clone());
            scored
                .iter()
                .skip(1)
                .map(|&(i, _)| cands[i].word.clone())
                .filter(|w| seen.insert(w.clone()))
                .take(2)
                .collect()
        } else {
            Vec::new()
        };
        Ok(Correction {
            primary: if applied { winner } else { text.to_owned() },
            alternatives,
            applied,
        })
    }

    /// Correct `text` in its `(preceding, prefix)` context. A word already known
    /// — in a lexicon, whitelisted/learned, or valid in any active language — is
    /// returned verbatim with `applied == false` (the no-clobber rule, BR-12).
    ///
    /// # Errors
    /// [`FeatherKeyError::Locale`] / [`FeatherKeyError::NoLanguages`] if the
    /// active language set cannot form a corrector (structurally prevented by the
    /// constructor's validation, but surfaced rather than panicked).
    pub fn correct(
        &self,
        text: &str,
        preceding: &str,
        prefix: &str,
    ) -> Result<Correction, FeatherKeyError> {
        let corrector = self.build_corrector()?;
        let ctx = TypingContext {
            preceding: preceding.to_owned(),
            prefix: prefix.to_owned(),
        };
        Ok(corrector.correct(
            &Token {
                text: text.to_owned(),
            },
            &ctx,
        ))
    }

    /// Assemble a `NoClobberCorrector` from a clone of the current state: the
    /// primary (first active) lexicon for fuzzy candidates, the live learned
    /// vocabulary, and a locale manager over every active language.
    fn build_corrector(&self) -> Result<NoClobberCorrector, FeatherKeyError> {
        let locales = LocaleManager::new(self.packs.clone())?;
        let (_, primary) = self.packs.first().ok_or(FeatherKeyError::NoLanguages)?;
        Ok(NoClobberCorrector::new(
            primary.clone(),
            self.personalization.clone(),
            locales,
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
    fn single_language_choose_correction_matches_legacy_correct() {
        let core = FeatherKeyCore::new(vec![(
            "en".into(),
            vec!["bat".into(), "cat".into(), "hat".into()],
        )])
        .expect("core");
        let legacy = core.correct("zat", "", "zat").expect("legacy");
        let now = core.choose_correction("zat", &[], vec![]).expect("now");
        assert_eq!(now.primary, legacy.primary);
        assert_eq!(now.applied, legacy.applied);
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
