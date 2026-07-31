//! Tiny per-user neural gate: decide whether to trust an autocorrect.

use featherkey_nn::Mlp;

/// Number of feature slots the gate consumes.
pub const INPUTS: usize = 5;

/// The residual is clamped to this magnitude so the gate can only nudge the
/// apply threshold, never overturn a no-clobber veto (which is applied first).
pub const RESIDUAL_BOUND: f64 = 1.5;

/// Output-region offset handed to [`Mlp::from_linear`]. Deliberately small: for
/// an all-zero coefficient prior every unit is degenerate, so `from_linear`
/// gives each unit the `DEAD_UNIT_WEIGHT` floor `η` and cancels the per-unit
/// constant `η·offset_c` uniformly against `b2`. A *large* `offset_c` turns that
/// cancellation into a difference of large f32 magnitudes (catastrophic
/// cancellation, see the re-ranker design); `C = 8` keeps the residual ~0 at
/// cold start while `b1 = C > 0` keeps every unit in its ReLU linear region so
/// its input weights retain a gradient path for training (Task 4).
const PRIOR_OFFSET_C: f32 = 8.0;

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
    /// Cold start: a ~0 residual (autocorrect behaves as base+floor), with the
    /// dead-unit weights `from_linear` supplies so training still flows step 1.
    #[must_use]
    pub fn from_prior() -> Self {
        let zero = [0.0_f32; INPUTS];
        Self {
            nn: Mlp::from_linear(&zero, 0.0, 1.0, PRIOR_OFFSET_C),
        }
    }

    /// The learned nudge on the apply threshold, clamped to ±[`RESIDUAL_BOUND`].
    #[must_use]
    pub fn residual(&self, f: &GateFeatures) -> f64 {
        f64::from(self.nn.forward(&f.to_array())).clamp(-RESIDUAL_BOUND, RESIDUAL_BOUND)
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

    #[test]
    fn cold_start_residual_is_negligible() {
        let g = AutocorrectGate::from_prior();
        let f = GateFeatures {
            edit_distance: 1.0,
            winner_confidence: 0.5,
            dict_rank_norm: 0.2,
            typed_len_norm: 0.3,
            momentum_weight: 0.0,
        };
        assert!(g.residual(&f).abs() < 1e-3, "cold-start residual must be ~0");
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
}
