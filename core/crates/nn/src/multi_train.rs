//! One backpropagation + SGD step for the multi-output MLP: cross-entropy
//! loss against a target class, returning the input gradient (`dL/dinput`)
//! that later trains the embedding layer upstream of this net. Pure math: no
//! I/O, no panicking indexing, and deltas are computed from the pre-update
//! weights before any parameter is mutated — mirroring `Mlp::train_step`'s
//! discipline in `train.rs`.

use super::error::NnError;
use super::MlpMulti;

impl MlpMulti {
    /// Cross-entropy step: `softmax(forward(x))` vs `target`, backprop, one
    /// SGD update to every parameter (`param -= lr·∂L/∂param`). Returns
    /// `(loss, dL/dinput)`; `dL/dinput` has length `inputs`.
    ///
    /// `target >= outputs` is a caller error, not a panic: `Err(NnError::Shape)`.
    ///
    /// Gradient math, computed from the PRE-update `w1`/`w2` before any
    /// parameter is mutated (mirrors `Mlp::train_step`'s "snapshot deltas
    /// before mutating" discipline):
    /// - `dlogit = softmax(logits)`, then `dlogit[target] -= 1.0`
    /// - output: `∂L/∂w2[o·H+j] = dlogit[o]·h[j]`, `∂L/∂b2[o] = dlogit[o]`
    /// - hidden: `δ_j = (Σ_o dlogit[o]·w2[o·H+j])·relu'(pre_j)`
    /// - input: `dInput[i] = Σ_j δ_j·w1[j·I+i]`
    pub fn train_step(
        &mut self,
        x: &[f32],
        target: usize,
        lr: f32,
    ) -> Result<(f32, Vec<f32>), NnError> {
        if target >= self.outputs() {
            return Err(NnError::Shape);
        }
        let (h, pre) = self.hidden_activations(x);
        let probs = Self::softmax(&self.output_layer(&h));
        let loss = -(probs.get(target).copied().unwrap_or(0.0).max(1e-12)).ln();

        let mut dlogit = probs;
        if let Some(t) = dlogit.get_mut(target) {
            *t -= 1.0;
        }

        // Snapshot deltas and the input gradient against the PRE-update
        // w1/w2 before either is mutated by the SGD steps below.
        let deltas = self.hidden_deltas(&dlogit, &pre);
        let d_input = self.input_gradient(&deltas);

        self.update_output_layer(&dlogit, &h, lr);
        self.update_hidden_layer(&deltas, x, lr);

        Ok((loss, d_input))
    }

    /// `δ_j = (Σ_o dlogit[o]·w2[o·H+j])·relu'(pre_j)` for each hidden unit,
    /// where `relu'(p) = 1.0` if `p > 0` else `0.0`. Uses the forward-pass
    /// (pre-update) `w2`, so call this before mutating `w2`.
    fn hidden_deltas(&self, dlogit: &[f32], pre: &[f32]) -> Vec<f32> {
        let hidden = self.hidden().max(1);
        (0..pre.len())
            .map(|j| {
                let sum: f32 = dlogit
                    .iter()
                    .zip(self.w2.chunks(hidden))
                    .map(|(&dl, row)| dl * row.get(j).copied().unwrap_or(0.0))
                    .sum();
                let relu_grad = if pre[j] > 0.0 { 1.0 } else { 0.0 };
                sum * relu_grad
            })
            .collect()
    }

    /// `dInput[i] = Σ_j δ_j·w1[j·I+i]`. Uses the forward-pass (pre-update)
    /// `w1`, so call this before mutating `w1`.
    fn input_gradient(&self, deltas: &[f32]) -> Vec<f32> {
        let inputs = self.inputs().max(1);
        let mut d_input = vec![0.0; self.inputs()];
        for (row, &delta) in self.w1.chunks(inputs).zip(deltas.iter()) {
            for (gi, &w) in d_input.iter_mut().zip(row.iter()) {
                *gi += delta * w;
            }
        }
        d_input
    }

    /// Output layer SGD update: `w2[o·H+j] -= lr·dlogit[o]·h[j]`,
    /// `b2[o] -= lr·dlogit[o]`.
    fn update_output_layer(&mut self, dlogit: &[f32], h: &[f32], lr: f32) {
        let hidden = self.hidden().max(1);
        for ((row, b2o), &dl) in self
            .w2
            .chunks_mut(hidden)
            .zip(self.b2.iter_mut())
            .zip(dlogit.iter())
        {
            for (w2j, hj) in row.iter_mut().zip(h.iter()) {
                *w2j -= lr * dl * hj;
            }
            *b2o -= lr * dl;
        }
    }

    /// Hidden layer SGD update: `w1[j·I+i] -= lr·δ_j·x[i]`, `b1[j] -= lr·δ_j`.
    fn update_hidden_layer(&mut self, deltas: &[f32], x: &[f32], lr: f32) {
        let width = self.inputs().max(1);
        for ((row, b1j), &delta) in self
            .w1
            .chunks_mut(width)
            .zip(self.b1.iter_mut())
            .zip(deltas.iter())
        {
            for (w1i, xi) in row.iter_mut().zip(x.iter()) {
                *w1i -= lr * delta * xi;
            }
            *b1j -= lr * delta;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::{MlpMulti, NnError};

    /// Test-only helper: cross-entropy loss of `m` at `x` against `target`,
    /// used as the reference function for finite-difference checks.
    fn ce_loss(m: &MlpMulti, x: &[f32], target: usize) -> f32 {
        let p = MlpMulti::softmax(&m.forward(x));
        -(p.get(target).copied().unwrap_or(0.0).max(1e-12)).ln()
    }

    #[test]
    fn repeated_steps_drive_argmax_to_target() {
        let mut m = MlpMulti::with_weights(
            vec![0.2, -0.1, 0.05, 0.3],
            vec![0.1, -0.2],
            vec![0.0; 6],
            vec![0.0; 3],
            2,
            2,
            3,
        );
        let x = [0.5, -0.3];
        for _ in 0..500 {
            let _ = m.train_step(&x, 2, 0.1).unwrap();
        }
        let o = m.forward(&x);
        let argmax = (0..3).max_by(|a, b| o[*a].total_cmp(&o[*b])).unwrap();
        assert_eq!(argmax, 2);
    }

    #[test]
    fn input_gradient_matches_finite_difference() {
        let m = MlpMulti::with_weights(
            vec![0.3, -0.2, 0.1, 0.4],
            vec![0.05, -0.1],
            vec![0.2, 0.1, -0.3, 0.15, 0.0, 0.25],
            vec![0.0, 0.0, 0.0],
            2,
            2,
            3,
        );
        let x = [0.4, -0.6];
        let (_loss, grad) = m.clone().train_step(&x, 1, 0.0).unwrap(); // lr=0 -> no mutation
        let eps = 1e-3_f32;
        for i in 0..2 {
            let mut xp = x;
            xp[i] += eps;
            let mut xm = x;
            xm[i] -= eps;
            let num = (ce_loss(&m, &xp, 1) - ce_loss(&m, &xm, 1)) / (2.0 * eps);
            assert!((grad[i] - num).abs() < 1e-2, "grad[{i}]={} num={num}", grad[i]);
        }
    }

    #[test]
    fn target_out_of_range_is_error_not_panic() {
        let mut m =
            MlpMulti::with_weights(vec![1.0], vec![0.0], vec![0.0, 0.0], vec![0.0, 0.0], 1, 1, 2);
        assert_eq!(m.train_step(&[1.0], 2, 0.1).unwrap_err(), NnError::Shape);
    }
}
