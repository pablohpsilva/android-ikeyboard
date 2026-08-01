//! Tiny per-user neural tap-warp: a bounded (dx,dy) shift over normalized tap
//! position, generalizing systematic aim across keys. Pure math; no I/O, no RNG.

use featherkey_nn::Mlp;

/// Position inputs: normalized (x, y).
pub const INPUTS: usize = 2;
/// Max per-axis shift in logical px — a warp can never fling a tap across keys.
pub const WARP_BOUND: f32 = 40.0;
/// Learning rate per tap. Small: track the slow systematic field, not per-tap noise.
pub const WARP_LR: f32 = 0.01;

// Signed-pair prior constants (mirror autocorrect-gate::from_prior). Position is
// centred at 0 in [-1,1], so each feature centre is 0.0.
const PRIOR_SCALE: f32 = 4.0;
const PRIOR_MARGIN: f32 = 0.05;
const PRIOR_WEIGHT: f32 = 0.005;

/// A per-user coordinate warp: two independent scalar MLPs (Δx and Δy) over the
/// normalized tap position. Cold start ≈ (0,0) everywhere yet trainable.
#[derive(Debug, Clone)]
pub struct TapWarp {
    dx: Mlp,
    dy: Mlp,
}

impl TapWarp {
    /// One axis's zero-output-but-trainable prior: two hidden units per input form
    /// a centred signed reader that cancels to ~0 while every input weight keeps a
    /// gradient path (identical construction to `AutocorrectGate::from_prior`).
    fn axis_prior() -> Mlp {
        let hidden = 2 * INPUTS;
        let mut w1 = vec![0.0_f32; hidden * INPUTS];
        let mut b1 = vec![0.0_f32; hidden];
        let mut w2 = vec![0.0_f32; hidden];
        for j in 0..INPUTS {
            let (pos, neg) = (2 * j, 2 * j + 1);
            w1[pos * INPUTS + j] = PRIOR_SCALE;
            b1[pos] = PRIOR_MARGIN; // centre μ_j = 0
            w2[pos] = PRIOR_WEIGHT;
            w1[neg * INPUTS + j] = -PRIOR_SCALE;
            b1[neg] = PRIOR_MARGIN;
            w2[neg] = -PRIOR_WEIGHT;
        }
        Mlp::with_weights(w1, b1, w2, 0.0, INPUTS, hidden)
    }

    #[must_use]
    pub fn from_prior() -> Self {
        Self {
            dx: Self::axis_prior(),
            dy: Self::axis_prior(),
        }
    }

    /// The learned (Δx, Δy) shift for a normalized tap, each clamped ±`WARP_BOUND`.
    #[must_use]
    pub fn warp(&self, nx: f32, ny: f32) -> (f32, f32) {
        let x = [nx, ny];
        (
            self.dx.forward(&x).clamp(-WARP_BOUND, WARP_BOUND),
            self.dy.forward(&x).clamp(-WARP_BOUND, WARP_BOUND),
        )
    }

    /// One squared-error SGD step per axis toward `(tx, ty)` (design §6 target).
    pub fn reinforce(&mut self, nx: f32, ny: f32, tx: f32, ty: f32, lr: f32) {
        let x = [nx, ny];
        let ddx = 2.0 * (self.dx.forward(&x) - tx);
        self.dx.train_step(&x, ddx, lr);
        let ddy = 2.0 * (self.dy.forward(&x) - ty);
        self.dy.train_step(&x, ddy, lr);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn cold_start_warp_is_near_zero_across_the_grid() {
        let w = TapWarp::from_prior();
        for nx in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            for ny in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                let (dx, dy) = w.warp(nx, ny);
                assert!(
                    dx.abs() < 0.05 && dy.abs() < 0.05,
                    "cold warp {dx},{dy} @ {nx},{ny}"
                );
            }
        }
    }

    #[test]
    fn warp_output_is_bounded() {
        let mut w = TapWarp::from_prior();
        // A large but FINITE over-bound target (real targets are ≤ keyboard px). A 1e6
        // target would overflow the weights to inf/NaN, and `f32::NAN.clamp(..)` is NaN
        // — so keep it realistic; the point under test is the ±WARP_BOUND clamp.
        for _ in 0..5_000 {
            w.reinforce(0.5, 0.5, 200.0, -200.0, WARP_LR);
        }
        let (dx, dy) = w.warp(0.5, 0.5);
        assert!(dx.abs() <= WARP_BOUND + 1e-3 && dy.abs() <= WARP_BOUND + 1e-3);
        assert!(
            dx.is_finite() && dy.is_finite(),
            "clamped output must stay finite"
        );
    }

    #[test]
    fn reinforce_moves_toward_a_systematic_offset_target() {
        // A stream whose target is a constant negative-x shift (cancel a +x bias).
        let mut w = TapWarp::from_prior();
        let before = w.warp(0.3, 0.3).0;
        for _ in 0..500 {
            w.reinforce(0.3, 0.3, -20.0, 0.0, WARP_LR);
        }
        let after = w.warp(0.3, 0.3).0;
        assert!(
            after < before - 1.0,
            "x-warp should move negative: {before} -> {after}"
        );
    }

    #[test]
    fn a_zero_mean_target_stream_keeps_the_warp_near_zero() {
        // Targets that average to 0 (a converged key) must not accumulate drift.
        let mut w = TapWarp::from_prior();
        for i in 0..1000 {
            let t = if i % 2 == 0 { 15.0 } else { -15.0 };
            w.reinforce(0.2, -0.4, t, 0.0, WARP_LR);
        }
        assert!(
            w.warp(0.2, -0.4).0.abs() < 5.0,
            "zero-mean target must not drift far"
        );
    }
}
