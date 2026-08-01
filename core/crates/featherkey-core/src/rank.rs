//! The suggestion-strip blend: everything that turns a keystroke into the words
//! the shell renders.
//!
//! This is the read-only half of the façade (ARCH §9.1 `Suggest`, design option
//! **b**). [`FeatherKeyCore`](crate::FeatherKeyCore) in `lib.rs` owns the state —
//! packs, learned models, momentum; this module owns the *policy* that combines
//! them: the ranked predictor's completions, the shell's device candidates, the
//! correction-signal bias, the language-momentum ordering, and the accent/
//! apostrophe variant guarantee. None of it mutates learned state (the write
//! side lives in `learn.rs`), so the whole path stays safe to run per keystroke.
//!
//! Keeping it out of `lib.rs` is also what keeps both files inside the
//! no-god-file bound (ARCH §6).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use featherkey_candidate_ranker::{LM_WEIGHT_LANG, SOURCE_PRIOR_DEVICE, SOURCE_PRIOR_LEXICON};
use featherkey_contracts::{Candidate, RankedCandidate, Source, TypingContext};
use featherkey_dictionary::Dictionary;
use featherkey_prediction::{StatisticalPredictor, MAX_SUGGESTIONS};

use crate::{FeatherKeyCore, CORRECTION_STICKY_WEIGHT, CORRECTION_UNWANTED_WEIGHT};

/// Weight on a hypothesis's spatial log-probability when it competes in the
/// ranker. Spatial fit nudges; frequency, learning, context and momentum still
/// decide.
const SPATIAL_WEIGHT: f64 = 0.35;

/// Cold-start weight of the `lm_logprob` feature slot. `lm_logprob` is a
/// stopgap literal `0.0` until Task 5 fills in the real value, so this
/// coefficient has no observable effect yet; kept within
/// `|coeff| * FEATURE_BOUND(20) < PRIOR_OFFSET_C(64)`.
const LM_LOGPROB_COEFF: f32 = 1.0;

/// Cold-start coefficients for the neural re-ranker, one per feature slot in
/// `RankFeatures::to_array` order: `[positional, ln_momentum, is_lexicon,
/// is_device, correction_promote, correction_demote, spatial, lm_logprob,
/// bias]`. Assembled from the *same* source consts the classic linear ranking
/// uses (via `as f32` casts, so a change there can never silently drift the
/// prior — pinned by `prior_coeffs_match_the_source_constants`), so the net
/// reproduces today's order until trained (Task 11+). The unit `1.0`/`-1.0`
/// slots are for features pre-weighted at their call sites; the trailing
/// `0.0` is the bias slot.
pub(crate) const PRIOR_COEFFS: [f32; featherkey_neural_ranker::INPUTS] = [
    1.0,
    LM_WEIGHT_LANG as f32,
    SOURCE_PRIOR_LEXICON as f32,
    SOURCE_PRIOR_DEVICE as f32,
    1.0,
    -1.0,
    SPATIAL_WEIGHT as f32,
    LM_LOGPROB_COEFF,
    0.0,
];

impl FeatherKeyCore {
    /// The whole suggestion-strip blend, core-owned (ARCH §9.1 `Suggest`,
    /// option **b**): predictor completions + shell-gathered `device` candidates
    /// → language-momentum ranking → dictionary fold-group variant guarantee.
    /// Read-only — never mutates learned state. The shell just renders the words.
    ///
    /// Ordering within a language is context → learned → bundled rank (via the
    /// ranked predictor); across languages it is the momentum-weighted
    /// [`candidate_ranker`](featherkey_candidate_ranker). Finally the accent/
    /// apostrophe variant of the typed token is guaranteed a slot so a commoner
    /// plain twin (`hell`) cannot crowd out `he'll` — derived from the shipped
    /// lexicons' fold index, never a hand-authored replacement table.
    ///
    /// # Speed (BR-46 / plan Global Constraint)
    /// The learned `freq`/`dict_rank` snapshots handed to the predictor are
    /// **scoped to just this query's completions**, so no whole-vocabulary map is
    /// cloned per keystroke. (The lexicons themselves are cloned into the
    /// predictor exactly as the legacy [`suggest`](Self::suggest) already does;
    /// materialising them is the deferred W4 follow-up.)
    #[must_use]
    pub fn rank_suggestions(
        &mut self,
        preceding: &str,
        prefix: &str,
        device: Vec<Candidate>,
    ) -> Vec<RankedCandidate> {
        let context = self.context.next_counts(preceding);
        let (freq, dict_rank) = self.scoped_learned_snapshots(prefix);
        let lang_lexicons: Vec<(String, Dictionary)> = self
            .packs
            .iter()
            .map(|p| (p.lang.as_str().to_owned(), p.dict.clone()))
            .collect();
        let predictor =
            StatisticalPredictor::new_ranked(lang_lexicons, &freq, &dict_rank, &context);
        let mut cands = predictor.suggest_ranked(&TypingContext {
            preceding: preceding.to_owned(),
            prefix: prefix.to_owned(),
        });
        cands.extend(device);
        // Score every candidate through the neural re-ranker. At cold start its
        // weights are `PRIOR_COEFFS`, so this reproduces the classic linear blend
        // (positional + momentum + source prior + correction net + spatial): the
        // features carry the correction and spatial signals the old per-word bias
        // applied, so no candidate is dropped before its promotion is counted.
        let spatial = self.spatial_hypotheses(prefix);
        for (word, _) in &spatial {
            if !cands.iter().any(|c| &c.word == word) {
                cands.push(Candidate {
                    word: word.clone(),
                    lang: self.primary_lang(),
                    source: Source::Lexicon,
                    // A spatial candidate enters at the back of the lexicon
                    // ordering; its own bias is what lifts it, so it competes
                    // rather than arriving pre-promoted.
                    source_rank: MAX_SUGGESTIONS as u32,
                });
            }
        }
        let ranked = featherkey_candidate_ranker::rank_by(&cands, MAX_SUGGESTIONS, |c| {
            self.neural_ranker
                .score(&self.rank_features(c, prefix, &spatial))
        });
        // Cache the shown set so a later strip pick can train the net against what
        // the user saw (Task 12); bounded to one snapshot, overwritten each query.
        self.last_ranked = Some(self.snapshot_shown(prefix, &ranked, &cands, &spatial));
        self.guarantee_fold_variant(prefix, ranked)
    }

    /// The correction score split into its two non-negative components:
    /// `(promote, demote)`. The neural re-ranker consumes them as independent
    /// features (`correction_promote` weighted `+1`, `correction_demote` `-1`), so
    /// their net reproduces the sticky-fix-minus-unwanted adjustment the classic
    /// linear ranking applied — now expressed as two slots of [`PRIOR_COEFFS`].
    ///
    /// * **`promote`** `CORRECTION_STICKY_WEIGHT * ln(1 + picks)`, or `0.0` when
    ///   `picks == 0` — `picks` is how often the user chose this completion for
    ///   this prefix (`observe_strip_pick`).
    /// * **`demote`** `CORRECTION_UNWANTED_WEIGHT * ln(1 + unwanted)`, or `0.0`
    ///   when `unwanted == 0` — `unwanted` is how often the user deleted and
    ///   retyped this word (`observe_delete_retype`), counted per word. Half the
    ///   promotion weight on purpose: a delete-retype is a weaker, noisier signal.
    ///
    /// Both are always `>= 0.0`.
    pub(crate) fn correction_parts(&self, prefix: &str, word: &str) -> (f64, f64) {
        let picks = self.corrections.pref_count(prefix, word);
        let unwanted = self.corrections.unwanted_count(word);
        let promote = if picks == 0 {
            0.0
        } else {
            CORRECTION_STICKY_WEIGHT * f64::from(1 + picks).ln()
        };
        let demote = if unwanted == 0 {
            0.0
        } else {
            CORRECTION_UNWANTED_WEIGHT * f64::from(1 + unwanted).ln()
        };
        (promote, demote)
    }

    /// The learned `freq` and bundled `dict_rank` snapshots the ranked predictor
    /// needs — restricted to the words that `prefix` actually completes to, so a
    /// keystroke never clones the whole learned/bundled vocabulary. An empty
    /// prefix completes to nothing here (the predictor's empty-prefix branch uses
    /// only `context`), so both maps are empty.
    fn scoped_learned_snapshots(
        &self,
        prefix: &str,
    ) -> (BTreeMap<String, u32>, BTreeMap<String, u32>) {
        if prefix.is_empty() {
            return (BTreeMap::new(), BTreeMap::new());
        }
        let folded = featherkey_fold::fold(prefix);
        let mut words: BTreeSet<String> = BTreeSet::new();
        for p in &self.packs {
            for w in p.dict.fold_prefix(&folded) {
                words.insert(w);
            }
        }
        let mut freq = BTreeMap::new();
        let mut dict_rank = BTreeMap::new();
        for w in &words {
            let f = self.personalization.frequency(w);
            if f > 0 {
                freq.insert(w.clone(), f);
            }
            if let Some(r) = self
                .packs
                .iter()
                .filter_map(|p| p.rank.get(w).copied())
                .min()
            {
                dict_rank.insert(w.clone(), r);
            }
        }
        (freq, dict_rank)
    }

    /// Guarantee the typed token's accent/apostrophe variant a strip slot, exactly
    /// as the Kotlin `SuggestionStrip.withGuaranteedVariant` did — moved core-side
    /// (plan W5 Step 1). The **device**-derived variant stays a thin Kotlin
    /// post-step; this covers the shipped-lexicon fold group only.
    fn guarantee_fold_variant(
        &self,
        prefix: &str,
        ranked: Vec<RankedCandidate>,
    ) -> Vec<RankedCandidate> {
        if prefix.is_empty() {
            return dedup_cap(ranked, MAX_SUGGESTIONS);
        }
        let shown: HashSet<String> = ranked.iter().map(|r| r.word.to_lowercase()).collect();
        let variant = self
            .accent_variants(prefix)
            .into_iter()
            .find(|v| !shown.contains(&v.word.to_lowercase()));
        let Some(variant) = variant else {
            return dedup_cap(ranked, MAX_SUGGESTIONS);
        };
        let mut out = ranked;
        let at = std::cmp::min(1, out.len());
        out.insert(at, variant);
        dedup_cap(out, MAX_SUGGESTIONS)
    }

    /// Real dictionary words in `prefix`'s **exact** accent-fold group whose
    /// spelling differs from what was typed (`ive → I've`, `voce → você`,
    /// `hell → he'll`, `tambem → também`), best-ranked (commonest) first. Derived
    /// purely from the shipped lexicons via the fold index — the Rust twin of
    /// `Vocabulary.accentVariantsOf`.
    fn accent_variants(&self, prefix: &str) -> Vec<RankedCandidate> {
        let folded = featherkey_fold::fold(prefix);
        let lower_prefix = prefix.to_lowercase();
        let mut seen: HashSet<String> = HashSet::new();
        let mut hits: Vec<(String, String, u32)> = Vec::new();
        for p in &self.packs {
            for w in p.dict.fold_prefix(&folded) {
                // fold_prefix returns prefix matches; keep only the *exact* group.
                if featherkey_fold::fold(&w) != folded || w.to_lowercase() == lower_prefix {
                    continue;
                }
                if !seen.insert(w.to_lowercase()) {
                    continue;
                }
                let rank = self
                    .packs
                    .iter()
                    .filter_map(|q| q.rank.get(&w).copied())
                    .min()
                    .unwrap_or(u32::MAX);
                hits.push((w, p.lang.as_str().to_owned(), rank));
            }
        }
        hits.sort_by_key(|(_, _, rank)| *rank); // most frequent first
        hits.into_iter()
            .map(|(word, lang, _)| {
                let score = featherkey_candidate_ranker::score(
                    &Candidate {
                        word: word.clone(),
                        lang: lang.clone(),
                        source: Source::Lexicon,
                        source_rank: 0,
                    },
                    &self.momentum,
                );
                RankedCandidate { word, lang, score }
            })
            .collect()
    }
}

/// De-duplicate `words` by lowercased spelling (first occurrence wins, preserving
/// order) and cap to `cap`. Mirrors the Kotlin `SuggestionStrip.dedupCap`.
fn dedup_cap(words: Vec<RankedCandidate>, cap: usize) -> Vec<RankedCandidate> {
    let mut seen: HashSet<String> = HashSet::new();
    words
        .into_iter()
        .filter(|w| seen.insert(w.word.to_lowercase()))
        .take(cap)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
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
        let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["car".into(), "cat".into()])])
            .expect("core");
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
    fn correction_parts_are_zero_without_history() {
        // Both count == 0 branches: a word with no correction history yields (0, 0),
        // so ranking is unchanged — the two together net to the same zero adjustment.
        let core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
        let (promote, demote) = core.correction_parts("ca", "cat");
        assert_eq!(promote, 0.0);
        assert_eq!(demote, 0.0);
    }
}
