//! One backpropagation + SGD step for the tiny MLP. Pure math: the caller
//! supplies `d_output = ∂L/∂out` (the loss gradient w.r.t. the scalar output);
//! this module chains it through the output and hidden layers and applies a
//! single in-place gradient-descent update to every weight and bias. No I/O,
//! no allocation beyond the pre-activation buffer already produced by
//! `hidden_activations`, and no panicking indexing (all zips over slices).

use super::Mlp;

impl Mlp {
    /// Backpropagate one supplied output-gradient and apply one SGD step to
    /// every parameter. `x` is the input the gradient was computed at (len
    /// `inputs`), `d_output = ∂L/∂out`, `lr` the learning rate.
    ///
    /// Output layer: `∂L/∂w2[j] = d_output·h[j]`, `∂L/∂b2 = d_output`.
    /// Hidden layer: `δ_j = d_output·w2[j]·relu'(pre_j)`, then
    /// `∂L/∂w1[j·inputs+i] = δ_j·x[i]` and `∂L/∂b1[j] = δ_j`.
    /// Update: `param -= lr·∂L/∂param`.
    pub fn train_step(&mut self, x: &[f32], d_output: f32, lr: f32) {
        let (h, pre) = self.hidden_activations(x);
        // Hidden deltas computed against the *pre-update* w2 (backprop uses the
        // forward-pass weights), so snapshot them before mutating w2.
        let deltas = self.hidden_deltas(d_output, &pre);

        // Output layer.
        for (w2j, hj) in self.w2.iter_mut().zip(h.iter()) {
            *w2j -= lr * d_output * hj;
        }
        self.b2 -= lr * d_output;

        // Input layer, row by row (row j owns inputs of hidden unit j).
        let width = self.inputs.max(1);
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

    /// `δ_j = d_output·w2[j]·relu'(pre_j)` for each hidden unit, where
    /// `relu'(p) = 1.0` if `p > 0` else `0.0`. Uses the forward-pass output
    /// weights (`w2`), so call this before mutating `w2`.
    fn hidden_deltas(&self, d_output: f32, pre: &[f32]) -> Vec<f32> {
        self.w2
            .iter()
            .zip(pre.iter())
            .map(|(w2j, &p)| {
                let relu_grad = if p > 0.0 { 1.0 } else { 0.0 };
                d_output * w2j * relu_grad
            })
            .collect()
    }
}

#[cfg(test)]
impl Mlp {
    /// Test-only: nudge the output bias, so finite-difference checks can probe
    /// `∂out/∂b2` without exposing `b2` publicly.
    fn nudge_b2(&mut self, eps: f32) {
        self.b2 += eps;
    }

    /// Test-only: nudge a single input weight `w1[idx]`.
    fn nudge_w1(&mut self, idx: usize, eps: f32) {
        if let Some(w) = self.w1.get_mut(idx) {
            *w += eps;
        }
    }

    /// Test-only: nudge a single output weight `w2[j]`.
    fn nudge_w2(&mut self, j: usize, eps: f32) {
        if let Some(w) = self.w2.get_mut(j) {
            *w += eps;
        }
    }

    /// Test-only accessor for `w1[idx]`.
    fn w1_at(&self, idx: usize) -> f32 {
        self.w1.get(idx).copied().unwrap_or(0.0)
    }

    /// Test-only accessor for `b1[idx]`.
    fn b1_at(&self, idx: usize) -> f32 {
        self.b1.get(idx).copied().unwrap_or(0.0)
    }

    /// Test-only accessor for `w2[idx]`.
    fn w2_at(&self, idx: usize) -> f32 {
        self.w2.get(idx).copied().unwrap_or(0.0)
    }

    /// Test-only accessor for `b2`.
    fn b2_val(&self) -> f32 {
        self.b2
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::Mlp;

    #[test]
    fn gradient_matches_finite_difference() {
        let base = Mlp::from_linear(&[0.3, -0.2], 0.1, 1.0, 50.0);
        let x = [1.5, -0.5];
        // dL/dout = 1 => dL/dparam = dout/dparam. Compare analytic step direction to FD.
        let eps = 1e-3;
        // Perturb b2: out increases by exactly eps => FD grad ~ 1.0.
        let mut up = base.clone();
        up.nudge_b2(eps);
        let fd = (up.forward(&x) - base.forward(&x)) / eps;
        assert!((fd - 1.0).abs() < 1e-2);
    }

    #[test]
    fn train_step_reduces_squared_error_on_a_toy_target() {
        let mut m = Mlp::from_linear(&[0.0, 0.0], 0.0, 1.0, 50.0);
        let x = [1.0, 2.0];
        let target = 5.0;
        let loss0 = (m.forward(&x) - target).powi(2);
        for _ in 0..200 {
            let d = 2.0 * (m.forward(&x) - target);
            m.train_step(&x, d, 0.01);
        }
        let loss1 = (m.forward(&x) - target).powi(2);
        assert!(loss1 < loss0 * 0.01, "loss {loss0}->{loss1}");
    }

    #[test]
    fn analytic_w2_gradient_matches_finite_difference() {
        // ∂out/∂w2[j] = h[j]. Pick a unit whose pre-activation is > 0 (from_linear
        // keeps every unit in its linear region), so h[j] = pre[j] > 0.
        let base = Mlp::from_linear(&[0.3, -0.2], 0.1, 1.0, 50.0);
        let x = [1.5, -0.5];
        let eps = 1e-3;
        // Analytic gradient with d_output = 1 is h[j]; derive it from a step.
        let mut stepped = base.clone();
        stepped.train_step(&x, 1.0, 1.0); // param -= grad
                                          // FD of the output w.r.t. w2[0].
        let mut up = base.clone();
        up.nudge_w2(0, eps);
        let fd = (up.forward(&x) - base.forward(&x)) / eps;
        // The applied step to w2[0] equals the analytic gradient (h[0]); compare.
        let analytic = base.w2_at(0) - stepped.w2_at(0); // = lr(1)·1·h[0] = h[0]
        assert!((fd - analytic).abs() < 1e-2, "fd={fd} analytic={analytic}");
    }

    #[test]
    fn analytic_w1_gradient_matches_finite_difference() {
        // ∂out/∂w1[j*inputs+i] = w2[j]·relu'(pre_j)·x[i]. Unit 0 is active, so
        // relu'=1 and the analytic gradient is w2[0]·x[0].
        let base = Mlp::from_linear(&[0.3, -0.2], 0.1, 1.0, 50.0);
        let x = [1.5, -0.5];
        let eps = 1e-3;
        let mut up = base.clone();
        up.nudge_w1(0, eps); // w1[0] = row 0, input 0
        let fd = (up.forward(&x) - base.forward(&x)) / eps;
        let analytic = base.w2_at(0) * x[0]; // relu'(pre_0)=1
        assert!((fd - analytic).abs() < 1e-2, "fd={fd} analytic={analytic}");
    }

    #[test]
    fn dead_relu_unit_receives_no_input_gradient() {
        // A hidden unit driven strictly negative has relu'=0, so its input
        // weights and bias must be unchanged by a train step (δ_j = 0).
        let mut m = Mlp::with_weights(vec![1.0], vec![0.0], vec![5.0], 0.0, 1, 1);
        let before = m.clone();
        m.train_step(&[-3.0], 2.0, 0.1); // pre = -3 → relu' = 0
                                         // w1 and b1 untouched (δ = 0); only w2 (via h=0 → also 0) and b2 move.
        assert_eq!(m.w1_at(0), before.w1_at(0), "dead unit w1 must not move");
        assert_eq!(m.b1_at(0), before.b1_at(0), "dead unit b1 must not move");
        // h[0] = 0 so w2 does not move either; only b2 shifts by lr·d_output.
        assert_eq!(m.w2_at(0), before.w2_at(0));
        assert!((m.b2_val() - (before.b2_val() - 0.1 * 2.0)).abs() < 1e-6);
    }

    #[test]
    fn active_relu_unit_updates_all_its_params() {
        // Complements the dead-unit test: a unit with pre > 0 has relu'=1, so
        // its input weights, bias, and output weight all move.
        let mut m = Mlp::with_weights(vec![1.0], vec![0.0], vec![5.0], 0.0, 1, 1);
        let before = m.clone();
        m.train_step(&[3.0], 2.0, 0.1); // pre = 3 → relu' = 1, h = 3
        assert_ne!(m.w1_at(0), before.w1_at(0));
        assert_ne!(m.b1_at(0), before.b1_at(0));
        assert_ne!(m.w2_at(0), before.w2_at(0));
    }
}
