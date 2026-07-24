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

/// Squared Euclidean distance. Squared is enough for ranking and avoids a
/// `sqrt`; the confidence step takes the root only where the magnitude matters.
fn distance_sq(a: TouchPoint, b: TouchPoint) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

/// Convert per-key distances into a confidence for the best key via
/// inverse-distance weighting. An exact hit (distance 0) yields confidence 1.0.
fn confidence_for_best(best_dist: f32, all_dists: &[f32]) -> Confidence {
    if best_dist == 0.0 {
        return Confidence::new(1.0);
    }
    // `best_dist` is the smallest distance (callers pass sorted distances) and
    // is > 0 here, so every weight is finite and positive and `total` is always
    // > 0 — no divide-by-zero guard is reachable. If a *non-best* key coincides
    // with the touch (distance 0 => infinite weight), `total` is infinite and
    // the best key's share collapses toward 0, which is the correct outcome.
    let weight = |d: f32| 1.0 / d;
    let total: f32 = all_dists.iter().copied().map(weight).sum();
    Confidence::new(weight(best_dist) / total)
}

/// Shift a key's geometric centre by this user's learned offset for it. With an
/// unbiased model the offset is `(0.0, 0.0)` and the centre is returned as-is,
/// so decoding is byte-for-byte the plain nearest-key result (ADR-15).
fn effective_center(center: TouchPoint, offset: (f32, f32)) -> TouchPoint {
    TouchPoint::new(center.x + offset.0, center.y + offset.1)
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

        // Distance from the touch to every key's model-biased center, paired
        // with its id. The learned offset re-centres each key on where this user
        // actually taps it (BR-7); an unbiased model leaves centers untouched.
        let mut scored: Vec<(KeyId, f32)> = layout
            .keys()
            .iter()
            .map(|k| {
                let center = effective_center(k.center(), model.offset(k.id));
                (k.id, distance_sq(touch, center))
            })
            .collect();

        // Ascending by distance => best first. total_cmp gives a total order
        // over f32 without unwrap (SEDD §5.5 rule 3: no panics on the path).
        scored.sort_by(|a, b| a.1.total_cmp(&b.1));

        let dists: Vec<f32> = scored.iter().map(|(_, d)| d.sqrt()).collect();
        let best_conf = confidence_for_best(dists[0], &dists);

        // Remaining candidates get a monotonically lower placeholder confidence
        // derived from the best; exact per-candidate scoring arrives with the
        // touch-model. Ordering is what the tracer bullet asserts.
        let ranked = scored
            .iter()
            .enumerate()
            .map(|(i, (id, _))| {
                let c = if i == 0 { best_conf.value() } else { best_conf.value() / (i as f32 + 1.0) };
                (*id, Confidence::new(c))
            })
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
            .decode(TouchPoint::new(250.0, 60.0), &layout, &TouchModel::unbiased())
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
            .decode(TouchPoint::new(270.0, 60.0), &layout, &TouchModel::unbiased())
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
            .decode(TouchPoint::new(230.0, 60.0), &layout, &TouchModel::unbiased())
            .unwrap();
        let ids: Vec<char> = out.ranked().iter().map(|(k, _)| k.ch()).collect();
        assert_eq!(ids[0], 'e');
        assert_eq!(ids[1], 'w');
        // Confidence is strictly decreasing across the ranked list.
        assert!(out.ranked()[0].1.value() > out.ranked()[1].1.value());
    }

    #[test]
    fn confidence_collapses_to_zero_when_another_key_coincides_with_the_touch() {
        // A non-best key at distance 0 gives infinite total weight, so the best
        // key's share collapses to 0 — no panic, no divide-by-zero.
        assert_eq!(confidence_for_best(5.0, &[0.0, 0.0]).value(), 0.0);
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
        let a = decoder.decode(touch, &layout, &TouchModel::unbiased()).unwrap();
        let b = decoder.decode(touch, &layout, &TouchModel::default()).unwrap();
        assert_eq!(a, b);
    }
}
