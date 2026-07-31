//! Tiny dependency-free neural substrate. A 1-hidden-layer MLP with a single
//! scalar output, ReLU hidden activation, linear output. Pure math: no I/O, no
//! Android types, errors are values (see `error`/`codec`).

mod prior;

#[derive(Debug, Clone, PartialEq)]
pub struct Mlp {
    w1: Vec<f32>, // [hidden * inputs], row-major by hidden unit
    b1: Vec<f32>, // [hidden]
    w2: Vec<f32>, // [hidden]
    b2: f32,
    inputs: usize,
    hidden: usize,
}

impl Mlp {
    #[must_use]
    pub fn with_weights(
        w1: Vec<f32>,
        b1: Vec<f32>,
        w2: Vec<f32>,
        b2: f32,
        inputs: usize,
        hidden: usize,
    ) -> Self {
        Self {
            w1,
            b1,
            w2,
            b2,
            inputs,
            hidden,
        }
    }

    #[must_use]
    pub fn inputs(&self) -> usize {
        self.inputs
    }

    /// Forward pass: `x` (len == inputs) → scalar score.
    #[must_use]
    pub fn forward(&self, x: &[f32]) -> f32 {
        let (h, _pre) = self.hidden_activations(x);
        self.w2
            .iter()
            .zip(h.iter())
            .fold(self.b2, |out, (w, hj)| out + w * hj)
    }

    /// Hidden activations and pre-activations (pre reused by backprop). Iterates
    /// via zipped slices so a length mismatch is truncated, never a panic.
    fn hidden_activations(&self, x: &[f32]) -> (Vec<f32>, Vec<f32>) {
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::Mlp;

    #[test]
    fn forward_computes_relu_mlp_by_hand() {
        // 2 inputs, 2 hidden, 1 output. W1 row-major [h][i].
        let mlp = Mlp::with_weights(
            vec![1.0, 0.0, 0.0, 1.0], // W1: h0=x0, h1=x1
            vec![0.0, 0.0],           // b1
            vec![2.0, -3.0],          // W2
            1.0,                      // b2
            2,
            2,
        );
        // h = relu([x0, x1]) = [1, 0] for x=[1,-4]; out = 2*1 + (-3)*0 + 1 = 3
        assert!((mlp.forward(&[1.0, -4.0]) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn forward_is_deterministic() {
        let m = Mlp::with_weights(vec![0.5, 0.5], vec![0.1], vec![2.0], 0.0, 2, 1);
        assert_eq!(m.forward(&[1.0, 1.0]), m.forward(&[1.0, 1.0]));
    }

    #[test]
    fn relu_clamps_a_negative_pre_activation_to_zero() {
        // Single hidden unit driven strictly negative; ReLU → 0, so only b2 remains.
        let m = Mlp::with_weights(vec![1.0], vec![0.0], vec![5.0], 7.0, 1, 1);
        // z = -3 → relu 0 → out = 5*0 + 7 = 7
        assert!((m.forward(&[-3.0]) - 7.0).abs() < 1e-6);
    }

    #[test]
    fn inputs_reports_the_configured_width() {
        let m = Mlp::with_weights(vec![0.0; 6], vec![0.0, 0.0], vec![1.0, 1.0], 0.0, 3, 2);
        assert_eq!(m.inputs(), 3);
    }

    #[test]
    fn clone_and_eq_round_trip() {
        let m = Mlp::with_weights(vec![1.0, 2.0], vec![0.3], vec![4.0], 0.5, 2, 1);
        assert_eq!(m.clone(), m);
    }
}
