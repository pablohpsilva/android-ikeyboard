//! The per-key accumulator: an incremental mean of the observed `(dx, dy)` tap
//! offsets plus the Welford co-moments that its 2x2 covariance is derived from.
//!
//! This module owns the *numerics* of learning one key — the O(1) fold and the
//! finiteness invariants that keep a single bad sample from poisoning what has
//! already been learned (BR-46, SEDD §5.5 r3). [`crate::TouchModel`] owns the
//! per-key map, the public API and persistence; `codec` encodes these fields.

/// The running mean of the `(dx, dy)` offsets seen for a single key, together
/// with the Welford co-moments needed to derive their 2x2 covariance.
///
/// Kept crate-private: callers observe and read through [`crate::TouchModel`],
/// never a bare per-key accumulator. `count` is `u64` and saturates, so even an
/// unbounded stream of taps can never overflow or panic (BR-46).
///
/// `m2xx`/`m2yy`/`m2xy` are the running sums of squared/cross deviations from
/// the mean (Welford's `M2`). Population covariance is `M2 / count`; they are
/// zero for a fresh key and stay zero through the first observation (a single
/// point has no spread), matching a legacy `v1` blob that carried no spread.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Mean {
    pub(crate) dx: f32,
    pub(crate) dy: f32,
    pub(crate) count: u64,
    pub(crate) m2xx: f32,
    pub(crate) m2yy: f32,
    pub(crate) m2xy: f32,
}

/// The covariance of a key with no measurable spread: unseen, or tapped once.
pub(crate) const ZERO_COVARIANCE: [[f32; 2]; 2] = [[0.0, 0.0], [0.0, 0.0]];

impl Mean {
    /// Fold one finite sample into the mean in O(1) using Welford's incremental
    /// update: `mean += (sample - mean) / n`. Callers guarantee the *inputs* are
    /// finite; this method additionally guards the *result*.
    ///
    /// The candidate mean is computed first and committed only if it is finite.
    /// Even with finite inputs the intermediate `sample - mean` can overflow to
    /// infinity (e.g. a near-`MAX` mean minus a near-`-MAX` sample), which would
    /// poison the stored offset. When that happens the accumulator is left
    /// entirely unchanged and `false` is returned so the caller can reject the
    /// observation.
    #[must_use]
    pub(crate) fn push(&mut self, dx: f32, dy: f32) -> bool {
        // Saturating so a pathological tap count can never wrap to zero and
        // divide-by-zero; at saturation the step size is ~0 and the mean holds.
        let count = self.count.saturating_add(1);
        let n = count as f32;
        let ndx = self.dx + (dx - self.dx) / n;
        let ndy = self.dy + (dy - self.dy) / n;
        if !ndx.is_finite() || !ndy.is_finite() {
            return false;
        }
        // Welford's online covariance co-moment update: fold the sample using the
        // *pre-update* mean for the first deviation and the *post-update* mean for
        // the second (the standard numerically-stable form). On the first
        // observation `dx - ndx == 0`, so the co-moments stay zero — a single
        // point has no spread. Guard the candidates for finiteness exactly like
        // the mean: a rejected fold leaves every accumulator untouched, so no
        // single sample can poison the stored covariance.
        let nm2xx = self.m2xx + (dx - self.dx) * (dx - ndx);
        let nm2yy = self.m2yy + (dy - self.dy) * (dy - ndy);
        let nm2xy = self.m2xy + (dx - self.dx) * (dy - ndy);
        if !nm2xx.is_finite() || !nm2yy.is_finite() || !nm2xy.is_finite() {
            return false;
        }
        self.dx = ndx;
        self.dy = ndy;
        self.count = count;
        self.m2xx = nm2xx;
        self.m2yy = nm2yy;
        self.m2xy = nm2xy;
        true
    }

    /// This key's 2x2 **population covariance**, the Welford co-moments divided
    /// by the observation count. Symmetric by construction, and
    /// [`ZERO_COVARIANCE`] until a second observation gives the samples any
    /// measurable spread.
    #[must_use]
    pub(crate) fn covariance(&self) -> [[f32; 2]; 2] {
        if self.count < 2 {
            return ZERO_COVARIANCE;
        }
        let n = self.count as f32;
        let cxy = self.m2xy / n;
        [[self.m2xx / n, cxy], [cxy, self.m2yy / n]]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use featherkey_kernel::KeyId;

    use crate::{TouchModel, TouchModelError};

    #[test]
    fn covariance_is_zero_until_two_observations() {
        let mut m = TouchModel::unbiased();
        assert_eq!(m.covariance(KeyId('a')), [[0.0, 0.0], [0.0, 0.0]]);
        m.observe(KeyId('a'), 1.0, 1.0).unwrap();
        assert_eq!(m.covariance(KeyId('a')), [[0.0, 0.0], [0.0, 0.0]]);
    }

    #[test]
    fn covariance_tracks_spread() {
        let mut m = TouchModel::unbiased();
        for (dx, dy) in [(2.0, 0.0), (-2.0, 0.0), (0.0, 2.0), (0.0, -2.0)] {
            m.observe(KeyId('a'), dx, dy).unwrap();
        }
        let cov = m.covariance(KeyId('a'));
        assert!(cov[0][0] > 0.0 && cov[1][1] > 0.0);
        assert!(cov[0][1].abs() < 1e-4); // uncorrelated axes
                                         // The covariance matrix is symmetric.
        assert_eq!(cov[0][1], cov[1][0]);
    }

    #[test]
    fn covariance_captures_positive_correlation() {
        let mut m = TouchModel::unbiased();
        // A user whose x and y offsets move together produces positive off-diagonal.
        for (dx, dy) in [(-2.0, -2.0), (-1.0, -1.0), (1.0, 1.0), (2.0, 2.0)] {
            m.observe(KeyId('a'), dx, dy).unwrap();
        }
        let cov = m.covariance(KeyId('a'));
        assert!(cov[0][1] > 0.0, "off-diagonal was {}", cov[0][1]);
        assert_eq!(cov[0][1], cov[1][0]);
    }

    #[test]
    fn covariance_stays_zero_for_an_unseen_key() {
        let mut m = TouchModel::unbiased();
        m.observe(KeyId('a'), 1.0, 1.0).unwrap();
        assert_eq!(m.covariance(KeyId('z')), [[0.0, 0.0], [0.0, 0.0]]);
    }

    #[test]
    fn a_rejected_observation_leaves_covariance_untouched() {
        let mut m = TouchModel::unbiased();
        for (dx, dy) in [(2.0, 0.0), (-2.0, 0.0)] {
            m.observe(KeyId('a'), dx, dy).unwrap();
        }
        let before = m.covariance(KeyId('a'));
        assert_eq!(
            m.observe(KeyId('a'), f32::NAN, 0.0),
            Err(TouchModelError::NonFiniteOffset)
        );
        assert_eq!(m.covariance(KeyId('a')), before);
    }
}
