//! Linear-prior initializer. Constructs an `Mlp` whose `forward` reproduces an
//! arbitrary bounded linear function `a·x + bias` exactly, while keeping every
//! weight non-zero so all units are trainable from the first gradient step.

use super::Mlp;

/// Below this magnitude a coefficient is treated as zero (degenerate unit).
const PRIOR_EPSILON: f32 = 1e-12;
/// Non-zero output-weight floor for a degenerate (`a_j == 0`) unit, so its
/// input weights keep a gradient path and can learn from step 1.
const DEAD_UNIT_WEIGHT: f32 = 1e-3;

impl Mlp {
    /// Cold-start init: a net whose `forward` reproduces `a·x + bias` exactly
    /// for inputs bounded by `B` when `offset_c > scale·B`, with every output
    /// weight non-zero so all units are trainable from the first step.
    ///
    /// `hidden == inputs == a.len()`. Unit `j` covers input `j`:
    /// `h_j = ReLU(scale·x_j + offset_c)`. `offset_c` keeps every unit in its
    /// linear region, so `w2[j]·h_j = a[j]·x[j] + w2[j]·offset_c`; the per-unit
    /// constant `w2[j]·offset_c` is cancelled uniformly by `b2`.
    ///
    /// For `a[j] == 0`, `w2[j] = 0` would freeze the unit, so it instead gets
    /// zero input weights, `b1[j] = offset_c`, and `w2[j] = η` (a small non-zero
    /// floor). Its constant `η·offset_c` is likewise cancelled, so it adds 0 at
    /// init yet keeps a gradient path to its input weights.
    ///
    /// `scale == 0` is a caller error (the division `a[j]/scale` is undefined):
    /// rather than panic, the result is a degenerate constant net that returns
    /// `bias` for every input.
    #[must_use]
    pub fn from_linear(a: &[f32], bias: f32, scale: f32, offset_c: f32) -> Self {
        let n = a.len();
        if scale == 0.0 {
            return Self::with_weights(
                vec![0.0; n * n],
                vec![offset_c; n],
                vec![0.0; n],
                bias,
                n,
                n,
            );
        }
        let mut w1 = vec![0.0_f32; n * n];
        let mut w2 = vec![0.0_f32; n];
        for (j, &aj) in a.iter().enumerate() {
            if aj.abs() > PRIOR_EPSILON {
                w1[j * n + j] = scale;
                w2[j] = aj / scale;
            } else {
                // Input weights stay zero; the floor keeps the unit trainable.
                w2[j] = DEAD_UNIT_WEIGHT;
            }
        }
        let b1 = vec![offset_c; n];
        let b2 = bias - offset_c * w2.iter().sum::<f32>();
        Self::with_weights(w1, b1, w2, b2, n, n)
    }
}

#[cfg(test)]
impl Mlp {
    /// Test-only view of the output weights.
    fn w2_iter(&self) -> impl Iterator<Item = f32> + '_ {
        self.w2.iter().copied()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::Mlp;

    #[test]
    fn from_linear_reproduces_the_linear_function_including_negative_outputs() {
        let a = [1.0, -2.0, 0.5];
        let mlp = Mlp::from_linear(&a, 0.7, 1.0, 100.0); // C=100 >> B
        for x in [[0.0, 0.0, 0.0], [3.0, -1.0, 2.0], [-4.0, 5.0, -3.0]] {
            let want = a[0] * x[0] + a[1] * x[1] + a[2] * x[2] + 0.7;
            assert!((mlp.forward(&x) - want).abs() < 1e-3, "x={x:?}");
        }
    }

    #[test]
    fn from_linear_leaves_every_output_weight_nonzero() {
        let mlp = Mlp::from_linear(&[1.0, 0.0, 0.35], 0.0, 1.0, 100.0);
        assert!(mlp.w2_iter().all(|w| w.abs() > 0.0));
    }

    #[test]
    fn a_zero_entry_still_reproduces_the_function_and_stays_trainable() {
        // Middle coefficient is exactly zero.
        let a = [2.0, 0.0, -1.5];
        let mlp = Mlp::from_linear(&a, -0.4, 1.0, 100.0);
        for x in [[0.0, 0.0, 0.0], [3.0, 7.0, -2.0], [-4.0, -9.0, 5.0]] {
            let want = a[0] * x[0] + a[1] * x[1] + a[2] * x[2] - 0.4;
            assert!((mlp.forward(&x) - want).abs() < 1e-3, "x={x:?}");
        }
        // The zero-coefficient unit (index 1) keeps a non-zero output weight,
        // so its input weights retain a gradient path.
        let weights: Vec<f32> = mlp.w2_iter().collect();
        assert!(weights[1].abs() > 0.0);
    }

    #[test]
    fn from_linear_is_deterministic() {
        let a = [0.3, -0.7, 1.1];
        let m1 = Mlp::from_linear(&a, 0.2, 1.0, 100.0);
        let m2 = Mlp::from_linear(&a, 0.2, 1.0, 100.0);
        assert_eq!(m1, m2);
    }

    #[test]
    fn zero_scale_is_a_degenerate_constant_net_returning_bias() {
        // scale == 0 is a caller error; the net must not panic and returns bias.
        let mlp = Mlp::from_linear(&[1.0, -2.0, 0.5], 0.9, 0.0, 100.0);
        for x in [[0.0, 0.0, 0.0], [3.0, -1.0, 2.0], [-4.0, 5.0, -3.0]] {
            assert!((mlp.forward(&x) - 0.9).abs() < 1e-6, "x={x:?}");
        }
    }
}
