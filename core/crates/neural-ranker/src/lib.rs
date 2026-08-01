//! Tiny neural re-ranker. Holds a 9-slot feature vector ([`RankFeatures`]) and
//! a [`NeuralRanker`] whose cold-start prior reproduces today's linear
//! candidate ranking exactly, so enabling the net causes no regression before
//! any training has happened. Pure math: no I/O, no clock, no RNG.

use featherkey_nn::Mlp;

mod persist;

/// Number of feature slots the ranker consumes: eight signals plus a constant
/// bias slot (slot 8 == 1.0).
pub const INPUTS: usize = 9;

/// The linear-region half-width the prior is built to cover. Every feature this
/// crate produces stays well inside `[-BOUND, BOUND]`; `PRIOR_OFFSET_C` is set
/// with a comfortable margin over it (see [`NeuralRanker::from_prior`]).
const FEATURE_BOUND: f32 = 20.0;

/// Output-region offset handed to [`Mlp::from_linear`]. Deliberately small:
/// `from_linear` cancels a per-unit constant of `w2[j]·offset_c` against `b2`,
/// and a *large* `offset_c` makes that a subtraction of two large f32 values —
/// catastrophic cancellation that swamps the tiny score with rounding error.
/// `C = 64` clears the bound `B = FEATURE_BOUND = 20` with ~3× margin (so every
/// prior unit stays in its linear region) while keeping parity error ~1e-5.
const PRIOR_OFFSET_C: f32 = 64.0;

/// The eight named ranking features for a single candidate, in slot order.
/// `to_array` appends the constant bias slot, so the array a [`NeuralRanker`]
/// scores is `[positional, ln_momentum, is_lexicon, is_device,
/// correction_promote, correction_demote, spatial, lm_logprob, 1.0]`.
#[derive(Debug, Clone, PartialEq)]
pub struct RankFeatures {
    /// Monotone transform of within-source rank (0 = best); typically negative.
    pub positional: f32,
    /// `ln` of the candidate language's momentum weight.
    pub ln_momentum: f32,
    /// 1.0 if the candidate came from a bundled lexicon, else 0.0.
    pub is_lexicon: f32,
    /// 1.0 if the candidate came from the device spell-checker, else 0.0.
    pub is_device: f32,
    /// Correction "sticky-fix" promotion signal (0.0 when no history).
    pub correction_promote: f32,
    /// Correction demotion signal from delete-retype (0.0 when no history).
    pub correction_demote: f32,
    /// Spatial/geometry agreement signal (0.0 when unused).
    pub spatial: f32,
    /// Language-model log-probability signal (0.0 when unused).
    pub lm_logprob: f32,
}

impl RankFeatures {
    /// Slot-ordered array fed to the net; slot 8 is the constant bias `1.0`.
    #[must_use]
    pub fn to_array(&self) -> [f32; INPUTS] {
        [
            self.positional,
            self.ln_momentum,
            self.is_lexicon,
            self.is_device,
            self.correction_promote,
            self.correction_demote,
            self.spatial,
            self.lm_logprob,
            1.0,
        ]
    }
}

/// A tiny MLP re-ranker. At cold start (see [`from_prior`](Self::from_prior))
/// its `forward` reproduces the linear score `coeffs·features`, so its ordering
/// matches the classic `candidate-ranker` before any training occurs.
#[derive(Debug, Clone)]
pub struct NeuralRanker {
    mlp: Mlp,
}

impl NeuralRanker {
    /// Cold-start init from linear coefficients (one per [`INPUTS`] slot). Builds
    /// an [`Mlp`] whose `forward` reproduces `coeffs·features` for features
    /// bounded by [`FEATURE_BOUND`], using `hidden == INPUTS`, `scale == 1.0`,
    /// and `offset_c == PRIOR_OFFSET_C == 64.0`.
    ///
    /// `C = 64` is intentionally small. `Mlp::from_linear` cancels a per-unit
    /// constant `w2[j]·C` against `b2`; a large `C` turns that into a difference
    /// of large f32 magnitudes and loses the small score to catastrophic
    /// cancellation. 64 still clears the input bound `B = 20` with ~3× margin
    /// (keeping every unit linear), so parity error stays ~1e-5.
    #[must_use]
    pub fn from_prior(coeffs: &[f32; INPUTS]) -> Self {
        let _ = FEATURE_BOUND; // documents the margin `PRIOR_OFFSET_C` covers.
        Self {
            mlp: Mlp::from_linear(coeffs, 0.0, 1.0, PRIOR_OFFSET_C),
        }
    }

    /// Score one candidate's features. At cold start this equals the linear
    /// `coeffs·features` (within ~1e-5), matching the classic ranker's order.
    #[must_use]
    pub fn score(&self, f: &RankFeatures) -> f64 {
        f64::from(self.mlp.forward(&f.to_array()))
    }

    /// Online pairwise learning-to-rank from one observed choice: the user
    /// picked candidate `chosen` out of the `shown` list, so nudge its score
    /// above each of the others by one SGD step per pair.
    ///
    /// For the pairwise logistic loss `L = -ln σ(s_c - s_j)` (chosen `c`,
    /// other `j`), `∂L/∂s_c = -σ(s_j - s_c)` and `∂L/∂s_j = +σ(s_j - s_c)`.
    /// With `d = σ(s_j - s_c)` we push the chosen up (`-d`) and each `j` down
    /// (`+d`) via [`Mlp::train_step`], which descends the supplied `∂L/∂out`.
    /// Scores are recomputed per pair (bounded to `shown.len() - 1` pairs).
    ///
    /// A no-op if `shown.len() < 2` (no pair to order) or `chosen` is out of
    /// range (nothing valid to promote).
    pub fn reinforce(&mut self, shown: &[RankFeatures], chosen: usize, lr: f32) {
        if shown.len() < 2 || chosen >= shown.len() {
            return;
        }
        let chosen_x = shown[chosen].to_array();
        for (j, other) in shown.iter().enumerate() {
            if j == chosen {
                continue;
            }
            let s_c = self.score(&shown[chosen]);
            let s_j = self.score(other);
            let d = sigmoid(s_j - s_c) as f32;
            self.mlp.train_step(&chosen_x, -d, lr);
            self.mlp.train_step(&other.to_array(), d, lr);
        }
    }
}

/// Numerically stable logistic sigmoid `1 / (1 + e^-x)`, deterministic (no RNG,
/// no clock). Used only to weigh each pairwise gradient in [`reinforce`].
fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use featherkey_candidate_ranker::{rank, Candidate, Source};
    use featherkey_language_momentum::Momentum;

    /// The prior coefficients the reinforce tests share.
    const COEFFS: [f32; INPUTS] = [1.0, 1.0, 0.2, 0.0, 1.0, -1.0, 0.35, 0.0, 0.0];

    /// An all-zero feature vector, so tests can vary one slot with `..zero()`.
    fn zero() -> RankFeatures {
        RankFeatures {
            positional: 0.0,
            ln_momentum: 0.0,
            is_lexicon: 0.0,
            is_device: 0.0,
            correction_promote: 0.0,
            correction_demote: 0.0,
            spatial: 0.0,
            lm_logprob: 0.0,
        }
    }

    #[test]
    fn repeatedly_choosing_a_lower_word_promotes_it() {
        let mut r = NeuralRanker::from_prior(&COEFFS);
        let strong = RankFeatures {
            positional: 0.0,
            ..zero()
        };
        let weak = RankFeatures {
            positional: -1.4,
            ..zero()
        };
        assert!(r.score(&strong) > r.score(&weak));
        for _ in 0..300 {
            r.reinforce(&[strong.clone(), weak.clone()], 1, 0.05);
        }
        assert!(
            r.score(&weak) > r.score(&strong),
            "weak should have overtaken"
        );
    }

    #[test]
    fn a_single_reinforce_does_not_unseat_a_strong_default() {
        let mut r = NeuralRanker::from_prior(&COEFFS);
        let strong = RankFeatures {
            positional: 0.0,
            ..zero()
        };
        let weak = RankFeatures {
            positional: -1.4,
            ..zero()
        };
        r.reinforce(&[strong.clone(), weak.clone()], 1, 0.05);
        assert!(r.score(&strong) > r.score(&weak));
    }

    #[test]
    fn reinforce_is_a_no_op_below_two_candidates() {
        let mut r = NeuralRanker::from_prior(&COEFFS);
        let f = RankFeatures {
            positional: -0.5,
            ..zero()
        };
        let before = r.score(&f);
        r.reinforce(&[], 0, 0.05); // empty
        r.reinforce(std::slice::from_ref(&f), 0, 0.05); // single
        assert_eq!(r.score(&f), before);
    }

    #[test]
    fn reinforce_is_a_no_op_when_chosen_out_of_range() {
        let mut r = NeuralRanker::from_prior(&COEFFS);
        let a = RankFeatures {
            positional: 0.0,
            ..zero()
        };
        let b = RankFeatures {
            positional: -1.0,
            ..zero()
        };
        let before = r.score(&a);
        r.reinforce(&[a.clone(), b.clone()], 2, 0.05); // chosen == len
        r.reinforce(&[a.clone(), b.clone()], 9, 0.05); // chosen > len
        assert_eq!(r.score(&a), before);
    }

    #[test]
    fn to_array_has_nine_slots_lm_logprob_before_bias() {
        let f = RankFeatures {
            positional: 1.0,
            ln_momentum: 0.2,
            is_lexicon: 1.0,
            is_device: 0.0,
            correction_promote: 0.0,
            correction_demote: 0.0,
            spatial: 0.3,
            lm_logprob: -0.7,
        };
        let a = f.to_array();
        assert_eq!(a.len(), 9);
        assert_eq!(a[7], -0.7); // lm_logprob
        assert_eq!(a[8], 1.0); // bias last
    }

    #[test]
    fn cold_prior_zero_lm_logprob_reproduces_eight_slot_score() {
        // With lm_logprob = 0 and a 9th coeff within the offset margin, the
        // 9-wide prior scores a candidate identically (±1e-4) to the 8-wide
        // prior — the parity that makes enabling the feature a no-op until warm.
        let c8 = [1.0, 1.0, 0.2, 0.0, 1.0, -1.0, 0.35];
        let mut c9 = [0.0f32; 9];
        c9[..7].copy_from_slice(&c8[..7]);
        c9[7] = 1.0; // slot 7 = LM coeff
        c9[8] = 0.0; // bias slot stays 0.0
        let r9 = NeuralRanker::from_prior(&c9);
        let f = RankFeatures {
            positional: 0.5,
            ln_momentum: 0.1,
            is_lexicon: 1.0,
            is_device: 0.0,
            correction_promote: 0.0,
            correction_demote: 0.0,
            spatial: 0.0,
            lm_logprob: 0.0,
        };
        // Reference 8-wide linear score of the same 8 signals:
        let lin8: f32 = 0.5 * 1.0 + 0.1 * 1.0 + 1.0 * 0.2 + 0.0 + 0.0 + 0.0 + 0.0 * 0.35;
        assert!((r9.score(&f) as f32 - lin8).abs() < 1e-3);
    }

    #[test]
    fn from_prior_reproduces_the_linear_score() {
        let coeffs = [1.0, 1.0, 0.2, 0.0, 1.0, -1.0, 0.35, 0.0, 0.0];
        let r = NeuralRanker::from_prior(&coeffs);
        let f = RankFeatures {
            positional: -1.1,
            ln_momentum: 0.4,
            is_lexicon: 1.0,
            is_device: 0.0,
            correction_promote: 0.0,
            correction_demote: 0.0,
            spatial: 0.0,
            lm_logprob: 0.0,
        };
        let want = coeffs[0] * f.positional + coeffs[1] * f.ln_momentum + coeffs[2] * 1.0;
        assert!((r.score(&f) - f64::from(want)).abs() < 1e-3);
    }

    #[test]
    fn to_array_places_the_constant_bias_in_the_last_slot() {
        let f = RankFeatures {
            positional: -0.7,
            ln_momentum: 0.2,
            is_lexicon: 1.0,
            is_device: 0.0,
            correction_promote: 0.3,
            correction_demote: -0.4,
            spatial: 0.5,
            lm_logprob: 0.6,
        };
        assert_eq!(
            f.to_array(),
            [-0.7, 0.2, 1.0, 0.0, 0.3, -0.4, 0.5, 0.6, 1.0]
        );
    }

    /// Build the exact same feature vector the cold-start prior scores as the
    /// classic ranker would score a candidate: positional from `source_rank`,
    /// `ln` of the language's momentum weight, and the one-hot source flags.
    fn features_of(cand: &Candidate, momentum: &Momentum) -> RankFeatures {
        RankFeatures {
            positional: -(1.0 + cand.source_rank as f32).ln(),
            ln_momentum: momentum.weight_of(&cand.lang).ln() as f32,
            is_lexicon: u8::from(cand.source == Source::Lexicon) as f32,
            is_device: u8::from(cand.source == Source::Device) as f32,
            correction_promote: 0.0,
            correction_demote: 0.0,
            spatial: 0.0,
            lm_logprob: 0.0,
        }
    }

    /// Coefficients that make the prior's linear score identical to the classic
    /// ranker's blend. Slots 0/1/2/3 mirror `positional_score`, `LM_WEIGHT_LANG`,
    /// and the per-source priors. Slots 4/5/6/7 multiply features that are 0 in
    /// this corpus, so their exact values are irrelevant here.
    fn cold_start_coeffs() -> [f32; INPUTS] {
        use featherkey_candidate_ranker::{
            LM_WEIGHT_LANG, SOURCE_PRIOR_DEVICE, SOURCE_PRIOR_LEXICON,
        };
        [
            1.0,
            LM_WEIGHT_LANG as f32,
            SOURCE_PRIOR_LEXICON as f32,
            SOURCE_PRIOR_DEVICE as f32,
            1.0,
            -1.0,
            0.35,
            0.0,
            0.0,
        ]
    }

    fn neural_order(cands: &[Candidate], momentum: &Momentum, k: usize) -> Vec<String> {
        let ranker = NeuralRanker::from_prior(&cold_start_coeffs());
        // Score, dedupe by word keeping the best score, sort desc, top-k —
        // mirroring `candidate_ranker::rank`'s dedupe/order/truncate.
        let mut best: Vec<(String, f64)> = Vec::new();
        for cand in cands {
            let s = ranker.score(&features_of(cand, momentum));
            match best.iter_mut().find(|(w, _)| *w == cand.word) {
                Some((_, existing)) if *existing >= s => {}
                Some((_, existing)) => *existing = s,
                None => best.push((cand.word.clone(), s)),
            }
        }
        best.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        best.truncate(k);
        best.into_iter().map(|(w, _)| w).collect()
    }

    fn cand(word: &str, lang: &str, source: Source, source_rank: u32) -> Candidate {
        Candidate {
            word: word.into(),
            lang: lang.into(),
            source,
            source_rank,
        }
    }

    #[test]
    fn cold_start_order_matches_candidate_ranker() {
        // Multi-language momentum, mixed sources, and source_ranks spread out so
        // adjacent scores differ by far more than the ~1e-5 parity error
        // (smallest positional step is ln 2 ≈ 0.69; source prior gap 0.2; the
        // momentum head-start is ~3.0 in ln space) — no exact ties are built.
        let mut mom = Momentum::new("en", &["en".into(), "es".into()]);
        mom.observe(&["es".into()]); // give Spanish some genuine momentum too
        let cands = vec![
            cand("hello", "en", Source::Lexicon, 0),
            cand("help", "en", Source::Lexicon, 3),
            cand("hola", "es", Source::Lexicon, 0),
            cand("helado", "es", Source::Lexicon, 5),
            cand("helo", "en", Source::Device, 1),
            cand("hey", "en", Source::Device, 0),
        ];
        let k = 5;
        let classic: Vec<String> = rank(&cands, &mom, k).into_iter().map(|r| r.word).collect();
        assert_eq!(neural_order(&cands, &mom, k), classic);
    }

    #[test]
    fn cold_start_order_matches_with_spanish_dominant_momentum() {
        // Flip the momentum so Spanish dominates; the neural prior must still
        // reproduce the classic order under a different momentum snapshot.
        let mut mom = Momentum::new("en", &["en".into(), "es".into()]);
        for _ in 0..8 {
            mom.observe(&["es".into()]);
        }
        let cands = vec![
            cand("hello", "en", Source::Lexicon, 0),
            cand("hola", "es", Source::Lexicon, 2),
            cand("adios", "es", Source::Lexicon, 4),
            cand("bye", "en", Source::Device, 1),
        ];
        let k = 4;
        let classic: Vec<String> = rank(&cands, &mom, k).into_iter().map(|r| r.word).collect();
        assert_eq!(neural_order(&cands, &mom, k), classic);
    }

    use proptest::prelude::*;
    proptest! {
        // Property: over randomly generated corpora whose scores are spread by
        // construction (distinct source_ranks per word), the cold-start prior's
        // order equals the classic ranker's order.
        #[test]
        fn cold_start_matches_over_random_corpora(
            n in 1usize..8,
            es_bumps in 0u32..12,
        ) {
            let mut mom = Momentum::new("en", &["en".into(), "es".into()]);
            for _ in 0..es_bumps { mom.observe(&["es".into()]); }
            // Distinct words and distinct source_ranks → no exact ties.
            let langs = ["en", "es"];
            let cands: Vec<Candidate> = (0..n)
                .map(|i| {
                    let lang = langs[i % 2];
                    let source = if i % 3 == 0 { Source::Device } else { Source::Lexicon };
                    cand(&format!("w{i}"), lang, source, i as u32)
                })
                .collect();
            let k = n;
            let classic: Vec<String> =
                rank(&cands, &mom, k).into_iter().map(|r| r.word).collect();
            prop_assert_eq!(neural_order(&cands, &mom, k), classic);
        }
    }
}
