//! The tap-distance *metric*: where a key effectively sits for this user, and
//! how an offset from that centre is weighted.
//!
//! Both pieces are pure functions of the injected [`TouchModel`](
//! featherkey_touch_model::TouchModel) snapshot and are evaluated once per key
//! per decode, never per tap in an inner loop and never with a `sqrt` (BR-46).
//! Together they define the geometry `decode` ranks against; with an unbiased
//! model both collapse to the identity and reproduce plain nearest-key
//! squared-Euclidean decoding byte for byte (ADR-15).

use featherkey_kernel::TouchPoint;

/// Shift a key's geometric centre by this user's learned offset for it. With an
/// unbiased model the offset is `(0.0, 0.0)` and the centre is returned as-is,
/// so decoding is byte-for-byte the plain nearest-key result (ADR-15).
pub(crate) fn effective_center(center: TouchPoint, offset: (f32, f32)) -> TouchPoint {
    TouchPoint::new(center.x + offset.0, center.y + offset.1)
}

/// A key's precomputed inverse covariance `Σ⁻¹`, symmetric by construction and
/// stored as its three distinct entries (`[[a, b], [b, d]]`). Applied as the
/// Mahalanobis quadratic form `dᵀ Σ⁻¹ d`, it scales a tap offset by how tightly
/// this user hits the key along each axis: a direction the user spreads over
/// (large variance) is penalized less than one they hit consistently.
///
/// The **identity** `[[1,0],[1,0]]` reduces the quadratic form to plain
/// squared-Euclidean `dx² + dy²`, so an unseen / low-count / non-invertible key
/// decodes byte-for-byte as the pre-Mahalanobis decoder did (ADR-15 invariant).
#[derive(Debug, Clone, Copy)]
pub(crate) struct InvCov {
    a: f32,
    b: f32,
    d: f32,
}

impl InvCov {
    /// `Σ⁻¹ = I`: the quadratic form collapses to squared-Euclidean.
    pub(crate) const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        d: 1.0,
    };

    /// Turn a 2x2 population covariance into a **scale-normalized, shrunk** `Σ⁻¹`,
    /// once per key at build time (never per tap, never a `sqrt`).
    ///
    /// Two regularizations make this robust — and are the fix for the "a
    /// consistently-tapped key rejects an on-key tap, so the tap resolves to a
    /// neighbour" decode bug:
    ///
    /// 1. **Shrinkage toward isotropic.** The covariance is blended with an
    ///    isotropic prior of the same mean variance (`Σ' = (1-λ)Σ + λσ̄²I`). This
    ///    bounds the condition number, so no single axis' variance can collapse to
    ///    ~0 and turn the key into a hypersensitive needle whose `Σ⁻¹` explodes.
    /// 2. **Scale normalization.** `Σ⁻¹` is scaled by the mean variance `σ̄²`, so an
    ///    isotropic covariance yields *exactly* the identity — matching the plain
    ///    squared-Euclidean scale used by unlearned keys (ADR-15 invariant). Absolute
    ///    tap tightness (in px²) therefore never competes on a different scale than
    ///    the Euclidean fallback; the covariance contributes only bounded
    ///    *anisotropy* (a direction the user spreads over is penalized less).
    ///
    /// A zero / non-finite mean variance or a non-positive / non-finite determinant
    /// falls back to the identity. Callers still pass the identity directly for keys
    /// with fewer than two observations.
    pub(crate) fn from_covariance(cov: [[f32; 2]; 2]) -> Self {
        /// Blend weight of the isotropic prior; bounds the condition number so a
        /// near-degenerate covariance can never produce a needle-sharp key.
        const SHRINK: f32 = 0.5;
        let (sxx, sxy, syy) = (cov[0][0], cov[0][1], cov[1][1]);
        let mean_var = 0.5 * (sxx + syy);
        if !mean_var.is_finite() || mean_var <= 0.0 {
            return Self::IDENTITY;
        }
        // Σ' = (1-λ)Σ + λ·σ̄²·I — shrink toward the isotropic prior.
        let sxx_s = (1.0 - SHRINK) * sxx + SHRINK * mean_var;
        let syy_s = (1.0 - SHRINK) * syy + SHRINK * mean_var;
        let sxy_s = (1.0 - SHRINK) * sxy;
        let det = sxx_s * syy_s - sxy_s * sxy_s;
        if !det.is_finite() || det <= 0.0 {
            return Self::IDENTITY;
        }
        // Σ⁻¹ · σ̄²: normalize so an isotropic Σ maps to the identity.
        let scale = mean_var / det;
        Self {
            a: syy_s * scale,
            b: -sxy_s * scale,
            d: sxx_s * scale,
        }
    }

    /// The Mahalanobis squared distance `dᵀ Σ⁻¹ d` for offset `(dx, dy)`. With
    /// the identity this is exactly `dx² + dy²`. No `sqrt` — this is the ranking
    /// magnitude; the confidence step still takes the root downstream.
    pub(crate) fn quadratic(&self, dx: f32, dy: f32) -> f32 {
        self.a * dx * dx + 2.0 * self.b * dx * dy + self.d * dy * dy
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Unit check on the inverse-covariance quadratic form directly: a non-
    /// invertible covariance falls back to the identity (plain squared-Euclidean),
    /// and a wide-x covariance makes an x-offset cheaper than an equal y-offset.
    /// (The zero-covariance/unseen-key identity reduction is the `observations<2`
    /// guard in `decode`, covered by the byte-for-byte regression test.)
    #[test]
    fn inv_cov_falls_back_to_identity_and_weights_anisotropically() {
        // Non-finite covariance => non-finite determinant => identity fallback.
        let id = InvCov::from_covariance([[f32::NAN, 0.0], [0.0, 0.0]]);
        assert_eq!(id.quadratic(3.0, 4.0), 3.0 * 3.0 + 4.0 * 4.0);

        // Wide x, tight y: an x-offset costs far less than an equal y-offset.
        let anis = InvCov::from_covariance([[100.0, 0.0], [0.0, 0.0]]);
        assert!(anis.quadratic(10.0, 0.0) < anis.quadratic(0.0, 10.0));
    }

    /// An unbiased key keeps its geometric centre exactly (ADR-15).
    #[test]
    fn effective_center_is_the_identity_for_an_unbiased_key() {
        let c = TouchPoint::new(10.0, 20.0);
        assert_eq!(effective_center(c, (0.0, 0.0)), c);
        assert_eq!(
            effective_center(c, (1.5, -2.5)),
            TouchPoint::new(11.5, 17.5)
        );
    }
}
