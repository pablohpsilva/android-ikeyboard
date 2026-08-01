//! Tiny per-user neural gate: decide whether to trust an autocorrect.

mod persist;

use featherkey_nn::Mlp;

/// Number of feature slots the gate consumes.
pub const INPUTS: usize = 5;

/// The residual is clamped to this magnitude so the gate can only nudge the
/// apply threshold, never overturn a no-clobber veto (which is applied first).
pub const RESIDUAL_BOUND: f64 = 1.5;

/// Per-feature typical value the prior centres each feature on (slot order). A
/// hidden-unit *pair* per feature reads `feature − centre`, so the learned
/// residual is driven by how far each feature sits from its centre rather than
/// by its raw magnitude. Centres are order-of-magnitude estimates from the Task-8
/// fixtures (edit distance ~1–2, confidence ~0.5, dict-rank-norm ~0.5, typed
/// length ~0.3, momentum ~ln 1 ≈ 0); their exact values only shift where the
/// piecewise response bends, not the cold-start residual (which is ~0 for any
/// realistic feature by construction).
const FEATURE_CENTERS: [f32; INPUTS] = [1.5, 0.5, 0.5, 0.3, 0.0];

/// Input-weight magnitude of each hidden unit (`±PRIOR_SCALE` on its own
/// feature). Large enough that the per-feature gradient energy dominates the
/// single global bias term, so training one correction barely moves an unrelated
/// one (no collateral suppression); see [`from_prior`](AutocorrectGate::from_prior).
const PRIOR_SCALE: f32 = 4.0;

/// Bias that keeps both halves of a feature's unit pair *marginally* active at
/// the centre (`pre = PRIOR_MARGIN > 0`), so every input weight retains a
/// gradient path from step 1 (Task 4) while the two halves' constant
/// contributions cancel exactly (`+κ·δ − κ·δ = 0`) — a ~0 cold-start residual.
const PRIOR_MARGIN: f32 = 0.05;

/// Output weight magnitude (`±PRIOR_WEIGHT`) of each unit pair. Deliberately
/// small: it scales the cold-start output (kept ~0) and the input-layer
/// gradients, without touching the output-weight gradient (`h_j`, which carries
/// the feature signal), so it tunes cold-start smallness independently of
/// learning sensitivity.
const PRIOR_WEIGHT: f32 = 0.005;

/// Default learning rate for one correction outcome. Gentle on purpose: with the
/// feature-sensitive prior, ~4 reverts of one correction cross the apply floor
/// (the product-approved 3–5), so a single accidental revert does not kill a
/// correction, and the per-step move is small enough that convergence is smooth
/// (no oscillation) rather than the overshoot the old constant-activation prior
/// produced.
pub const GATE_LR: f32 = 0.008;

/// Structural features of one correction decision (slot order = the contract).
#[derive(Debug, Clone, Copy)]
pub struct GateFeatures {
    /// Edit distance between the typed word and the winning candidate.
    pub edit_distance: f32,
    /// Confidence score of the winning candidate.
    pub winner_confidence: f32,
    /// Normalized dictionary rank of the winning candidate.
    pub dict_rank_norm: f32,
    /// Normalized length of the typed word.
    pub typed_len_norm: f32,
    /// Language-momentum weight for the candidate's language.
    pub momentum_weight: f32,
}

impl GateFeatures {
    /// Slot-ordered array fed to the net.
    #[must_use]
    pub fn to_array(&self) -> [f32; INPUTS] {
        [
            self.edit_distance,
            self.winner_confidence,
            self.dict_rank_norm,
            self.typed_len_norm,
            self.momentum_weight,
        ]
    }
}

/// A tiny per-user MLP that produces a bounded residual on the autocorrect apply
/// threshold. At cold start (see [`from_prior`](Self::from_prior)) the residual
/// is ~0, so autocorrect behaves as its base+floor policy until training moves
/// the weights.
#[derive(Debug, Clone)]
pub struct AutocorrectGate {
    nn: Mlp,
}

impl AutocorrectGate {
    /// Cold start: a ~0 residual (autocorrect behaves as base+floor) built from a
    /// **feature-sensitive** prior, so training one correction's outcome moves
    /// that correction without dragging unrelated ones with it.
    ///
    /// Two hidden units per feature form a centred, signed reader of feature `j`:
    /// unit `2j` fires above [`FEATURE_CENTERS`]`[j]` (`ReLU(+s·x_j − s·μ_j + δ)`)
    /// and unit `2j+1` below it (`ReLU(−s·x_j + s·μ_j + δ)`), with output weights
    /// `+κ` / `−κ`. At `x_j = μ_j` both pre-activations equal `δ > 0` (marginally
    /// active, so their input weights keep a gradient path) and their outputs
    /// cancel (`+κδ − κδ = 0`), giving a ~0 cold-start residual. Because each pair
    /// only activates for *its* feature's deviation, a revert of correction *A*
    /// concentrates its gradient on the units *A* actually excites, leaving a
    /// differently-shaped correction *B* almost untouched (no global coupling —
    /// the failure of the old constant-activation prior).
    #[must_use]
    pub fn from_prior() -> Self {
        let hidden = 2 * INPUTS;
        let mut w1 = vec![0.0_f32; hidden * INPUTS];
        let mut b1 = vec![0.0_f32; hidden];
        let mut w2 = vec![0.0_f32; hidden];
        for (j, &mu) in FEATURE_CENTERS.iter().enumerate() {
            let (pos, neg) = (2 * j, 2 * j + 1);
            w1[pos * INPUTS + j] = PRIOR_SCALE;
            b1[pos] = -PRIOR_SCALE * mu + PRIOR_MARGIN;
            w2[pos] = PRIOR_WEIGHT;
            w1[neg * INPUTS + j] = -PRIOR_SCALE;
            b1[neg] = PRIOR_SCALE * mu + PRIOR_MARGIN;
            w2[neg] = -PRIOR_WEIGHT;
        }
        Self {
            nn: Mlp::with_weights(w1, b1, w2, 0.0, INPUTS, hidden),
        }
    }

    /// The learned nudge on the apply threshold, clamped to ±[`RESIDUAL_BOUND`].
    #[must_use]
    pub fn residual(&self, f: &GateFeatures) -> f64 {
        f64::from(self.nn.forward(&f.to_array())).clamp(-RESIDUAL_BOUND, RESIDUAL_BOUND)
    }

    /// One SGD step of squared-error regression toward `target` (the desired
    /// residual for these features): reverts train toward a negative target,
    /// kept/reached toward positive. `d_output = 2 * (forward - target)`.
    pub fn reinforce(&mut self, f: &GateFeatures, target: f32, lr: f32) {
        let x = f.to_array();
        let d = 2.0 * (self.nn.forward(&x) - target);
        self.nn.train_step(&x, d, lr);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn features_serialize_in_slot_order() {
        let f = GateFeatures {
            edit_distance: 1.0,
            winner_confidence: 0.5,
            dict_rank_norm: 0.25,
            typed_len_norm: 0.375,
            momentum_weight: 0.0,
        };
        assert_eq!(f.to_array(), [1.0, 0.5, 0.25, 0.375, 0.0]);
    }

    /// A representative strong correction: one edit from the commonest neighbour,
    /// high confidence (~0.75), top dict rank. Mirrors the Task-8 "xat"→"cat"
    /// fixture the core gate applies at cold start.
    fn strong() -> GateFeatures {
        GateFeatures {
            edit_distance: 1.0,
            winner_confidence: 0.75,
            dict_rank_norm: 1.0,
            typed_len_norm: 0.1875,
            momentum_weight: 0.0,
        }
    }

    #[test]
    fn cold_start_residual_is_small() {
        // The prior is a near-no-op: for a realistic feature vector its residual
        // is within the design's "residual ≈ 0" tolerance (exact zero is not
        // required; the centred unit pairs cancel to a small, not zero, output).
        let g = AutocorrectGate::from_prior();
        let f = GateFeatures {
            edit_distance: 1.0,
            winner_confidence: 0.5,
            dict_rank_norm: 0.2,
            typed_len_norm: 0.3,
            momentum_weight: 0.0,
        };
        assert!(
            g.residual(&f).abs() < 0.05,
            "cold-start residual must be ~0"
        );
    }

    #[test]
    fn a_few_reverts_suppress_one_correction() {
        // Reverting ONE strong correction a handful of times must pull its
        // residual far enough negative that `winner_confidence + residual` drops
        // below the core's `AUTOCORRECT_FLOOR` (0.3) — i.e. it would be withheld.
        let mut g = AutocorrectGate::from_prior();
        let f = strong();
        for _ in 0..5 {
            g.reinforce(&f, -1.0, GATE_LR); // the core's REVERT_TARGET
        }
        let gated = f64::from(f.winner_confidence) + g.residual(&f);
        assert!(
            gated < 0.3,
            "five reverts must push a strong correction under the floor: {gated}"
        );
    }

    #[test]
    fn reverting_one_correction_does_not_suppress_another() {
        // Suppressing correction A must not drag a DISTINCT correction B down with
        // it (no global collateral). B differs from A in edit distance, confidence
        // and dict rank, so it excites a different set of centred unit pairs.
        let mut g = AutocorrectGate::from_prior();
        let a = strong();
        let b = GateFeatures {
            edit_distance: 2.0,
            winner_confidence: 0.4,
            dict_rank_norm: 0.15,
            typed_len_norm: 0.5,
            momentum_weight: 0.1,
        };
        let b_cold = g.residual(&b);
        for _ in 0..5 {
            g.reinforce(&a, -1.0, GATE_LR);
        }
        assert!(
            (g.residual(&b) - b_cold).abs() < 0.1,
            "B's residual must stay ~unchanged: cold {b_cold}, now {}",
            g.residual(&b)
        );
        // And B, being a different correction, would still apply.
        assert!(f64::from(b.winner_confidence) + g.residual(&b) >= 0.3);
    }

    #[test]
    fn residual_is_bounded() {
        // Even a hand-built extreme model cannot exceed the clamp.
        let g = AutocorrectGate::from_prior();
        let f = GateFeatures {
            edit_distance: 1e6,
            winner_confidence: 1e6,
            dict_rank_norm: 1e6,
            typed_len_norm: 1e6,
            momentum_weight: 1e6,
        };
        assert!(g.residual(&f).abs() <= RESIDUAL_BOUND + 1e-9);
    }

    #[test]
    fn reinforce_moves_the_residual_toward_the_target() {
        let f = GateFeatures {
            edit_distance: 2.0,
            winner_confidence: 0.1,
            dict_rank_norm: 0.05,
            typed_len_norm: 0.25,
            momentum_weight: 0.0,
        };
        let mut up = AutocorrectGate::from_prior();
        let before = up.residual(&f);
        for _ in 0..200 {
            up.reinforce(&f, 1.0, GATE_LR);
        }
        assert!(
            up.residual(&f) > before + 0.1,
            "target +1 must raise residual"
        );

        let mut down = AutocorrectGate::from_prior();
        for _ in 0..200 {
            down.reinforce(&f, -1.0, GATE_LR);
        }
        assert!(down.residual(&f) < -0.1, "target -1 must lower residual");
    }
}
