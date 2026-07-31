//! Tiny per-user neural gate: decide whether to trust an autocorrect.

/// Number of feature slots the gate consumes.
pub const INPUTS: usize = 5;

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
}
