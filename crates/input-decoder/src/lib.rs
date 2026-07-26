//! The accuracy engine: touch coordinates + key geometry -> intended key.
//!
//! `decode` is a **pure, read-only** function of geometry and the per-user
//! touch model (SEDD §5.4 boundary invariants): it never mutates learned state
//! and never persists, so the hot path carries no write/crypto cost. The
//! per-user adaptive [`TouchModel`] that biases these distances lives in the
//! separate `touch-model` crate and is injected as an immutable snapshot
//! (ADR-15). An *unbiased* model reproduces the walking-skeleton's uniform
//! nearest-key decoding exactly; a learned model re-centres each key on where
//! this user actually taps it (BR-6, BR-7, BR-46).

use featherkey_kernel::{Confidence, CoreError, KeyId, TouchPoint};
use featherkey_layout_engine::Layout;
use featherkey_touch_model::TouchModel;

/// A ranked set of candidate keys for a single touch, best first.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyCandidates {
    ranked: Vec<(KeyId, Confidence)>,
}

impl KeyCandidates {
    /// The most likely key, or `None` if there were no candidates.
    #[must_use]
    pub fn best(&self) -> Option<KeyId> {
        self.ranked.first().map(|(id, _)| *id)
    }

    /// All candidates, best first, as `(key, confidence)` pairs.
    #[must_use]
    pub fn ranked(&self) -> &[(KeyId, Confidence)] {
        &self.ranked
    }
}

/// The contract every decoder implements (SEDD §5.4). Stable across the
/// statistical MVP decoder and any future model-biased decoder.
pub trait InputDecoder {
    /// Decode a touch against a layout into ranked key candidates, biasing key
    /// geometry with the per-user `model` (ADR-15 / SEDD §5.4). Each key's
    /// effective centre is shifted by that key's learned offset before distance
    /// is measured, so a user who consistently taps off-centre still resolves to
    /// the intended key. An unbiased `model` leaves every centre unchanged and
    /// reproduces plain nearest-key decoding.
    ///
    /// # Errors
    /// Returns [`CoreError::EmptyLayout`] if `layout` has no keys.
    fn decode(
        &self,
        touch: TouchPoint,
        layout: &Layout,
        model: &TouchModel,
    ) -> Result<KeyCandidates, CoreError>;
}

/// Unbiased nearest-key decoder: ranks keys by ascending distance from the
/// touch to each key's center. Confidence is the touched key's share of an
/// inverse-distance weighting, so a tap dead-center on one key approaches 1.0
/// and a tap equidistant between two keys approaches 0.5.
#[derive(Debug, Clone, Copy, Default)]
pub struct NearestKeyDecoder;

impl NearestKeyDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

/// Squared Euclidean distance. Superseded on the ranking path by the
/// Mahalanobis quadratic form ([`InvCov::quadratic`]) — with the identity that
/// quadratic *is* this — but retained as the reference the byte-for-byte
/// regression guard measures against, hence test-only.
#[cfg(test)]
fn distance_sq(a: TouchPoint, b: TouchPoint) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

/// Pre-computed denominator for inverse-distance confidence shares, built once
/// per decode so each key's confidence is O(1) instead of re-scanning every
/// distance per candidate (which made `decode` O(n²) on the keystroke hot path).
///
/// Inverse-distance weighting (`w(d) = 1/d`) is undefined at `d == 0`, so exact
/// hits are handled explicitly: if any key sits on the touch, only the on-touch
/// keys carry confidence and they split it evenly (a lone dead-centre hit ⇒ 1.0,
/// two coincident keys ⇒ 0.5 each, every other key ⇒ 0.0). With no exact hit
/// every weight is finite and positive, so `total_weight > 0`.
#[derive(Debug, Clone, Copy)]
struct ShareBasis {
    /// How many keys sit exactly on the touch (`dist == 0`).
    zeros: usize,
    /// Sum of `1/d` over all keys — only meaningful when `zeros == 0`.
    total_weight: f32,
}

impl ShareBasis {
    fn new(all_dists: &[f32]) -> Self {
        let zeros = all_dists.iter().filter(|d| **d == 0.0).count();
        let total_weight = if zeros == 0 {
            all_dists.iter().map(|&d| 1.0 / d).sum()
        } else {
            0.0
        };
        Self {
            zeros,
            total_weight,
        }
    }

    /// One key's share of the weighting — its true proximity to the touch,
    /// not a placeholder derived from the winner. O(1).
    fn share(&self, dist: f32) -> Confidence {
        if self.zeros > 0 {
            let share = if dist == 0.0 {
                1.0 / self.zeros as f32
            } else {
                0.0
            };
            return Confidence::new(share);
        }
        Confidence::new((1.0 / dist) / self.total_weight)
    }
}

/// Shift a key's geometric centre by this user's learned offset for it. With an
/// unbiased model the offset is `(0.0, 0.0)` and the centre is returned as-is,
/// so decoding is byte-for-byte the plain nearest-key result (ADR-15).
fn effective_center(center: TouchPoint, offset: (f32, f32)) -> TouchPoint {
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
struct InvCov {
    a: f32,
    b: f32,
    d: f32,
}

impl InvCov {
    /// `Σ⁻¹ = I`: the quadratic form collapses to squared-Euclidean.
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        d: 1.0,
    };

    /// Invert a 2x2 population covariance for use as `Σ⁻¹`, once per key at
    /// build time (never per tap, never a `sqrt`). The diagonal is regularized
    /// with a small ε so a zero or rank-deficient covariance is still finite,
    /// and any non-positive / non-finite determinant falls back to the identity
    /// (→ squared-Euclidean). Callers pass the identity directly for keys with
    /// fewer than two observations so those stay byte-for-byte unchanged.
    fn from_covariance(cov: [[f32; 2]; 2]) -> Self {
        const EPS: f32 = 1e-3;
        let a = cov[0][0] + EPS;
        let b = cov[0][1];
        let d = cov[1][1] + EPS;
        let det = a * d - b * b;
        if !det.is_finite() || det <= 0.0 {
            return Self::IDENTITY;
        }
        let inv_det = 1.0 / det;
        Self {
            a: d * inv_det,
            b: -b * inv_det,
            d: a * inv_det,
        }
    }

    /// The Mahalanobis squared distance `dᵀ Σ⁻¹ d` for offset `(dx, dy)`. With
    /// the identity this is exactly `dx² + dy²`. No `sqrt` — this is the ranking
    /// magnitude; the confidence step still takes the root downstream.
    fn quadratic(&self, dx: f32, dy: f32) -> f32 {
        self.a * dx * dx + 2.0 * self.b * dx * dy + self.d * dy * dy
    }
}

impl InputDecoder for NearestKeyDecoder {
    fn decode(
        &self,
        touch: TouchPoint,
        layout: &Layout,
        model: &TouchModel,
    ) -> Result<KeyCandidates, CoreError> {
        if layout.is_empty() {
            return Err(CoreError::EmptyLayout);
        }

        // Mahalanobis squared distance from the touch to every key's model-
        // biased center, paired with its id. The learned offset re-centres each
        // key on where this user actually taps it (BR-7), and the per-key
        // inverse covariance weights the offset by how consistently they hit
        // that key along each axis. The inverse covariance is computed once per
        // key here (never per tap in an inner loop, never a `sqrt`); a key with
        // fewer than two observations uses the identity, so an unbiased model
        // leaves centers untouched and reduces to plain squared-Euclidean.
        let mut scored: Vec<(KeyId, f32)> = layout
            .keys()
            .iter()
            .map(|k| {
                let center = effective_center(k.center(), model.offset(k.id));
                let inv_cov = if model.observations(k.id) < 2 {
                    InvCov::IDENTITY
                } else {
                    InvCov::from_covariance(model.covariance(k.id))
                };
                let dx = touch.x - center.x;
                let dy = touch.y - center.y;
                (k.id, inv_cov.quadratic(dx, dy))
            })
            .collect();

        // Ascending by distance => best first. total_cmp gives a total order
        // over f32 without unwrap (SEDD §5.5 rule 3: no panics on the path).
        scored.sort_by(|a, b| a.1.total_cmp(&b.1));

        let dists: Vec<f32> = scored.iter().map(|(_, d)| d.sqrt()).collect();

        // Every candidate gets its *own* inverse-distance share, so confidences
        // reflect true proximity: a key twice as far carries half the weight,
        // not a synthetic `best/(i+1)` placeholder. Ranking is unchanged (still
        // ascending distance), so a smaller distance always yields the larger
        // share and the order the tracer bullet asserts is preserved.
        let basis = ShareBasis::new(&dists);
        let ranked = scored
            .iter()
            .zip(dists.iter())
            .map(|((id, _), &d)| (*id, basis.share(d)))
            .collect();

        Ok(KeyCandidates { ranked })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use featherkey_layout_engine::Layout;
    use featherkey_touch_model::TouchModel;

    #[test]
    fn empty_layout_is_an_error_not_a_panic() {
        let decoder = NearestKeyDecoder::new();
        let err = decoder.decode(
            TouchPoint::new(0.0, 0.0),
            &Layout::default(),
            &TouchModel::unbiased(),
        );
        assert_eq!(err, Err(CoreError::EmptyLayout));
    }

    #[test]
    fn dead_center_tap_picks_that_key_with_full_confidence() {
        let decoder = NearestKeyDecoder::new();
        let layout = Layout::qwerty_tracer_row();
        // Center of 'e' (third key) is (250, 60).
        let out = decoder
            .decode(
                TouchPoint::new(250.0, 60.0),
                &layout,
                &TouchModel::unbiased(),
            )
            .unwrap();
        assert_eq!(out.best(), Some(KeyId('e')));
        assert_eq!(out.ranked()[0].1.value(), 1.0);
    }

    #[test]
    fn off_center_tap_still_resolves_to_nearest_key() {
        let decoder = NearestKeyDecoder::new();
        let layout = Layout::qwerty_tracer_row();
        // Slightly right of 'e' center but still inside 'e'.
        let out = decoder
            .decode(
                TouchPoint::new(270.0, 60.0),
                &layout,
                &TouchModel::unbiased(),
            )
            .unwrap();
        assert_eq!(out.best(), Some(KeyId('e')));
        let conf = out.ranked()[0].1.value();
        assert!(conf > 0.5 && conf < 1.0, "confidence was {conf}");
    }

    #[test]
    fn candidates_are_ranked_by_proximity() {
        let decoder = NearestKeyDecoder::new();
        let layout = Layout::qwerty_tracer_row();
        // Between 'w'(150) and 'e'(250) centers, nearer 'e'.
        let out = decoder
            .decode(
                TouchPoint::new(230.0, 60.0),
                &layout,
                &TouchModel::unbiased(),
            )
            .unwrap();
        let ids: Vec<char> = out.ranked().iter().map(|(k, _)| k.ch()).collect();
        assert_eq!(ids[0], 'e');
        assert_eq!(ids[1], 'w');
        // Confidence is strictly decreasing across the ranked list.
        assert!(out.ranked()[0].1.value() > out.ranked()[1].1.value());
    }

    #[test]
    fn confidence_collapses_to_zero_when_another_key_coincides_with_the_touch() {
        // A key at distance 5 while another key sits on the touch (distance 0):
        // the on-touch key takes the confidence, so this one collapses to 0 —
        // no panic, no divide-by-zero.
        assert_eq!(ShareBasis::new(&[0.0, 0.0]).share(5.0).value(), 0.0);
    }

    /// The core of this change: a non-best candidate's confidence is its *real*
    /// inverse-distance share, not the old `best/(i+1)` placeholder. A key four
    /// times farther than the winner must carry ≈ one quarter of its confidence
    /// (`share ∝ 1/dist`), which is materially below what the placeholder — half
    /// the winner's, for the second candidate — would have produced.
    #[test]
    fn non_best_confidence_reflects_true_distance_not_a_placeholder() {
        let decoder = NearestKeyDecoder::new();
        let layout = Layout::qwerty_tracer_row();
        // At x=230: 'e'(250) is 20px away, 'w'(150) is 80px away — 4x farther.
        let out = decoder
            .decode(
                TouchPoint::new(230.0, 60.0),
                &layout,
                &TouchModel::unbiased(),
            )
            .unwrap();
        let best = out.ranked()[0].1.value();
        let second = out.ranked()[1].1.value();
        // Real inverse-distance: 4x the distance ⇒ ~1/4 the confidence.
        assert!(
            (second - best / 4.0).abs() < 1e-4,
            "expected ~best/4, got {second} (best {best})"
        );
        // The old placeholder would have made the second candidate best/2;
        // prove we are materially below that synthetic value.
        assert!(
            best / 2.0 - second > 0.1,
            "second {second} is not materially below the best/2 placeholder"
        );
    }

    /// A tap that lands closer to a neighbour's centre than to the intended
    /// key's centre is *mis*-decoded by the unbiased model, but *correctly*
    /// decoded once the model has learned that this user taps the intended key
    /// off-centre by exactly that much (BR-7 targeting, BR-6/BR-46).
    #[test]
    fn a_learned_offset_reclaims_a_tap_that_would_otherwise_miss() {
        let decoder = NearestKeyDecoder::new();
        let layout = Layout::qwerty_tracer_row();
        // 'e' center is x=250, 'r' center is x=350. A tap at x=310 is nearer
        // 'r' (40px) than 'e' (60px): the unbiased decoder commits 'r'.
        let touch = TouchPoint::new(310.0, 60.0);
        let unbiased = decoder
            .decode(touch, &layout, &TouchModel::unbiased())
            .unwrap();
        assert_eq!(unbiased.best(), Some(KeyId('r')));

        // This user habitually taps 'e' 60px to its right. Teach the model.
        let mut model = TouchModel::unbiased();
        for _ in 0..8 {
            model.observe(KeyId('e'), 60.0, 0.0).unwrap();
        }
        // 'e' effective center is now x=310 — dead on the tap — so it wins.
        let biased = decoder.decode(touch, &layout, &model).unwrap();
        assert_eq!(biased.best(), Some(KeyId('e')));
        // Landing exactly on the biased center yields full confidence.
        assert_eq!(biased.ranked()[0].1.value(), 1.0);
    }

    /// The biased centre also shifts a *vertical* offset, and a learned bias on
    /// one key must not perturb decoding of an unrelated one.
    #[test]
    fn a_learned_offset_is_per_key_and_two_dimensional() {
        let decoder = NearestKeyDecoder::new();
        let layout = Layout::qwerty_tracer_row();
        let mut model = TouchModel::unbiased();
        // User taps 'w' low and slightly left; 'e' is never observed.
        model.observe(KeyId('w'), -30.0, 40.0).unwrap();

        // 'w' center (150,60) -> effective (120,100). A tap there lands on 'w'.
        let out = decoder
            .decode(TouchPoint::new(120.0, 100.0), &layout, &model)
            .unwrap();
        assert_eq!(out.best(), Some(KeyId('w')));
        assert_eq!(out.ranked()[0].1.value(), 1.0);

        // 'e' has no learned bias: a dead-center tap on it is unaffected.
        let e = decoder
            .decode(TouchPoint::new(250.0, 60.0), &layout, &model)
            .unwrap();
        assert_eq!(e.best(), Some(KeyId('e')));
        assert_eq!(e.ranked()[0].1.value(), 1.0);
    }

    /// Regression guard on the ADR-15 invariant: an unbiased model must produce
    /// the *identical* candidate set to plain nearest-key decoding for the same
    /// touch — same order, same confidences.
    #[test]
    fn unbiased_model_is_byte_for_byte_plain_nearest_key() {
        let decoder = NearestKeyDecoder::new();
        let layout = Layout::qwerty_tracer_row();
        let touch = TouchPoint::new(230.0, 60.0);
        let a = decoder
            .decode(touch, &layout, &TouchModel::unbiased())
            .unwrap();
        let b = decoder
            .decode(touch, &layout, &TouchModel::default())
            .unwrap();
        assert_eq!(a, b);
    }

    /// The "today" decoder, re-implemented inline as pure squared-Euclidean +
    /// inverse-distance shares. This is the pre-Mahalanobis reference the byte-
    /// for-byte guard measures against, independent of the new code path.
    fn reference_squared_euclidean(touch: TouchPoint, layout: &Layout) -> KeyCandidates {
        let mut scored: Vec<(KeyId, f32)> = layout
            .keys()
            .iter()
            .map(|k| (k.id, distance_sq(touch, k.center())))
            .collect();
        scored.sort_by(|a, b| a.1.total_cmp(&b.1));
        let dists: Vec<f32> = scored.iter().map(|(_, d)| d.sqrt()).collect();
        let basis = ShareBasis::new(&dists);
        let ranked = scored
            .iter()
            .zip(dists.iter())
            .map(|((id, _), &d)| (*id, basis.share(d)))
            .collect();
        KeyCandidates { ranked }
    }

    /// CRITICAL regression guard: with a zero-covariance (unbiased) model the
    /// Mahalanobis quadratic form must reduce to plain squared-Euclidean, so
    /// `decode` is byte-for-byte identical to the pre-change decoder — same
    /// order, same `f32` confidences — across a spread of touch positions.
    #[test]
    fn zero_covariance_model_reduces_to_squared_euclidean_byte_for_byte() {
        let decoder = NearestKeyDecoder::new();
        let layout = Layout::qwerty_tracer_row();
        for touch in [
            TouchPoint::new(230.0, 60.0),
            TouchPoint::new(250.0, 60.0),
            TouchPoint::new(70.0, 33.0),
            TouchPoint::new(410.0, 90.0),
            TouchPoint::new(0.0, 0.0),
        ] {
            let got = decoder
                .decode(touch, &layout, &TouchModel::unbiased())
                .unwrap();
            let want = reference_squared_euclidean(touch, &layout);
            assert_eq!(got, want, "mismatch at {touch:?}");
        }
    }

    /// Anisotropy: a key learned with a wide *horizontal* spread (and ~zero
    /// mean) must penalize a horizontal tap offset far less than an equal-
    /// magnitude vertical one — the whole point of covariance weighting. Under
    /// the old isotropic squared-Euclidean the two offsets score equally, so
    /// this fails until the Mahalanobis form lands.
    #[test]
    fn wide_x_covariance_penalizes_x_offset_less_than_equal_y_offset() {
        let decoder = NearestKeyDecoder::new();
        let layout = Layout::qwerty_tracer_row();
        // Teach 'e' a wide horizontal spread with a ~centered mean by feeding
        // symmetric +/-40px horizontal offsets: var(dx) large, var(dy) ~0.
        let mut model = TouchModel::unbiased();
        for _ in 0..8 {
            model.observe(KeyId('e'), 40.0, 0.0).unwrap();
            model.observe(KeyId('e'), -40.0, 0.0).unwrap();
        }
        // Same-magnitude offset from 'e' centre (250,60): once in x, once in y.
        let x_tap = decoder
            .decode(TouchPoint::new(270.0, 60.0), &layout, &model)
            .unwrap();
        let y_tap = decoder
            .decode(TouchPoint::new(250.0, 80.0), &layout, &model)
            .unwrap();
        let e_conf = |c: &KeyCandidates| {
            c.ranked()
                .iter()
                .find(|(k, _)| *k == KeyId('e'))
                .map(|(_, v)| v.value())
                .unwrap()
        };
        let x_conf = e_conf(&x_tap);
        let y_conf = e_conf(&y_tap);
        assert!(
            x_conf > y_conf,
            "wide-x covariance: x-offset conf {x_conf} should exceed y-offset conf {y_conf}"
        );
    }

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
}
