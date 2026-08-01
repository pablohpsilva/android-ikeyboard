//! Multi-output MLP: 1-hidden-layer, ReLU hidden activation, linear output
//! layer with `outputs` scalar heads. Sibling to `Mlp` (single scalar output)
//! rather than a generalization of it — `Mlp` is untouched and still serves
//! its three shipped callers. Pure math: no I/O, no Android types, errors are
//! values.

/// A 1-hidden-layer MLP with `outputs` linear output heads (ReLU hidden).
/// `w2`/`b2` are the output layer: `w2` is `[outputs * hidden]` row-major by
/// output, `b2` is `[outputs]`.
#[derive(Debug, Clone, PartialEq)]
pub struct MlpMulti {
    // `pub(crate)`: the `multi_train` sibling module backpropagates through
    // this net and applies SGD updates directly to these, mirroring how
    // `Mlp::train_step` (in `train.rs`) mutates `Mlp`'s fields in place.
    pub(crate) w1: Vec<f32>, // [hidden * inputs], row-major by hidden unit
    pub(crate) b1: Vec<f32>, // [hidden]
    pub(crate) w2: Vec<f32>, // [outputs * hidden], row-major by output
    pub(crate) b2: Vec<f32>, // [outputs]
    inputs: usize,
    hidden: usize,
    outputs: usize,
}

impl MlpMulti {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn with_weights(
        w1: Vec<f32>,
        b1: Vec<f32>,
        w2: Vec<f32>,
        b2: Vec<f32>,
        inputs: usize,
        hidden: usize,
        outputs: usize,
    ) -> Self {
        Self {
            w1,
            b1,
            w2,
            b2,
            inputs,
            hidden,
            outputs,
        }
    }

    #[must_use]
    pub fn inputs(&self) -> usize {
        self.inputs
    }

    #[must_use]
    pub fn hidden(&self) -> usize {
        self.hidden
    }

    #[must_use]
    pub fn outputs(&self) -> usize {
        self.outputs
    }

    /// Forward pass: `x` (len == inputs) → `outputs` scalar scores.
    /// Truncation-safe: a length mismatch on `x`, `w1`/`b1`, or `w2`/`b2` is
    /// truncated via zipped iteration, never a panic.
    #[must_use]
    pub fn forward(&self, x: &[f32]) -> Vec<f32> {
        let (h, _pre) = self.hidden_activations(x);
        self.output_layer(&h)
    }

    /// Output layer over already-computed hidden activations `h`. Split out
    /// of `forward` so `multi_train::train_step` can reuse it without
    /// recomputing the hidden pass a second time.
    pub(crate) fn output_layer(&self, h: &[f32]) -> Vec<f32> {
        self.w2
            .chunks(self.hidden.max(1))
            .zip(self.b2.iter())
            .map(|(row, &bias)| {
                row.iter()
                    .zip(h.iter())
                    .fold(bias, |acc, (w, hj)| acc + w * hj)
            })
            .collect()
    }

    /// Hidden activations and pre-activations (`pre` reused by backprop in
    /// `multi_train`, mirroring `Mlp::hidden_activations`). Iterates via
    /// zipped slices so a length mismatch is truncated, never a panic.
    pub(crate) fn hidden_activations(&self, x: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let mut h = Vec::with_capacity(self.hidden);
        let mut pre = Vec::with_capacity(self.hidden);
        for (row, &bias) in self.w1.chunks(self.inputs.max(1)).zip(self.b1.iter()) {
            let z = row
                .iter()
                .zip(x.iter())
                .fold(bias, |acc, (w, xi)| acc + w * xi);
            pre.push(z);
            h.push(if z > 0.0 { z } else { 0.0 });
        }
        (h, pre)
    }

    /// Numerically-stable softmax: subtracts the max logit before `exp`, then
    /// divides by the sum. If the sum is zero or non-finite (e.g. all
    /// `-inf`), falls back to a uniform distribution — never `NaN`, never a
    /// panic. Empty input yields an empty vector.
    #[must_use]
    pub fn softmax(logits: &[f32]) -> Vec<f32> {
        if logits.is_empty() {
            return Vec::new();
        }
        let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let exps: Vec<f32> = logits
            .iter()
            .map(|&l| {
                if max.is_finite() {
                    (l - max).exp()
                } else {
                    1.0
                }
            })
            .collect();
        let sum: f32 = exps.iter().sum();
        if sum.is_finite() && sum > 0.0 {
            exps.iter().map(|&e| e / sum).collect()
        } else {
            let uniform = 1.0 / logits.len() as f32;
            vec![uniform; logits.len()]
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::MlpMulti;

    #[test]
    fn forward_computes_multi_output_by_hand() {
        // 2 inputs, 2 hidden, 2 outputs. W1 row-major [h][i]; W2 row-major [o][h].
        let m = MlpMulti::with_weights(
            vec![1.0, 0.0, 0.0, 1.0], // W1: h0=x0, h1=x1
            vec![0.0, 0.0],           // b1
            vec![1.0, 0.0, 0.0, 2.0], // W2: o0=h0, o1=2*h1
            vec![0.5, -1.0],          // b2
            2,
            2,
            2,
        );
        // x=[3,-4] -> h=relu([3,-4])=[3,0] -> out=[1*3+0.5, 2*0-1.0]=[3.5,-1.0]
        let o = m.forward(&[3.0, -4.0]);
        assert!((o[0] - 3.5).abs() < 1e-6 && (o[1] + 1.0).abs() < 1e-6);
    }

    #[test]
    fn softmax_sums_to_one_and_is_stable_on_large_logits() {
        let p = MlpMulti::softmax(&[1000.0, 1000.0, 1000.0]);
        let sum: f32 = p.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
        assert!(p.iter().all(|x| x.is_finite()));
        assert!((p[0] - 1.0 / 3.0).abs() < 1e-5);
    }

    #[test]
    fn softmax_degenerate_input_falls_back_to_uniform() {
        // Empty / zero-length logits: no panic, no NaN (covers the fallback branch).
        assert!(MlpMulti::softmax(&[]).is_empty());
        let p = MlpMulti::softmax(&[f32::NEG_INFINITY, f32::NEG_INFINITY]);
        assert!(p.iter().all(|x| x.is_finite()) && (p.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn forward_is_truncation_safe_on_short_input() {
        let m = MlpMulti::with_weights(vec![1.0, 1.0], vec![0.0], vec![1.0], vec![0.0], 2, 1, 1);
        // Too-short input must not panic (mirrors Mlp::forward).
        let _ = m.forward(&[1.0]);
    }
}
