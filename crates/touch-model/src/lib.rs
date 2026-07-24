//! Per-user adaptive tap-geometry model (the sole writer of the tap-geometry
//! data domain, ADR-14).
//!
//! This crate owns **one** thing: the incremental learning of where a given
//! user actually taps relative to each key's centre. `input-decoder` reads an
//! immutable snapshot of this model to bias its geometry (ADR-15); it never
//! mutates it. Nothing here performs I/O, crypto, or persistence — that is
//! `secure-store`'s job (SEDD §5.4). The model is a pure, deterministic
//! function of the sequence of observations fed to it.
//!
//! The learning rule is an **incremental running mean** of the per-key touch
//! offset `(dx, dy)`. A fresh model is *unbiased*: every key's offset is
//! `(0.0, 0.0)`, which is exactly the neutral input the Wave-2 decoder defaults
//! to (ADR-15) so the walking-skeleton behaviour is preserved until real taps
//! are learned. Closes BR-7 (learn the user's typing style) and BR-46 (the
//! update is O(1) and allocation-free per tap, keeping the fast-typing path
//! non-blocking).

use std::collections::HashMap;

use featherkey_kernel::KeyId;

/// Why an observation was rejected. Errors are values, never panics on the hot
/// path (SEDD §5.5 rule 3): a bad sample is dropped and reported, it never
/// corrupts the learned mean or unwinds the input thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TouchModelError {
    /// The observation could not be folded in while keeping the key's mean
    /// finite. Either the incoming `dx`/`dy` was `NaN`/infinite, or folding an
    /// otherwise-finite sample would have driven the *accumulated* running mean
    /// non-finite (e.g. an intermediate overflow to infinity). Storing such a
    /// mean would poison every future offset for that key, so the update is
    /// refused and the prior mean is left unchanged.
    NonFiniteOffset,
}

impl std::fmt::Display for TouchModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TouchModelError::NonFiniteOffset => f.write_str("touch offset was not finite"),
        }
    }
}

impl std::error::Error for TouchModelError {}

/// The running mean of the `(dx, dy)` offsets seen for a single key.
///
/// Kept private: callers observe and read through [`TouchModel`], never a bare
/// per-key accumulator. `count` is `u64` and saturates, so even an unbounded
/// stream of taps can never overflow or panic (BR-46).
#[derive(Debug, Clone, Copy, Default)]
struct Mean {
    dx: f32,
    dy: f32,
    count: u64,
}

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
    fn push(&mut self, dx: f32, dy: f32) -> bool {
        // Saturating so a pathological tap count can never wrap to zero and
        // divide-by-zero; at saturation the step size is ~0 and the mean holds.
        let count = self.count.saturating_add(1);
        let n = count as f32;
        let ndx = self.dx + (dx - self.dx) / n;
        let ndy = self.dy + (dy - self.dy) / n;
        if !ndx.is_finite() || !ndy.is_finite() {
            return false;
        }
        self.dx = ndx;
        self.dy = ndy;
        self.count = count;
        true
    }
}

/// A per-user model of tap geometry: for each key, the learned mean offset of
/// the user's taps from that key's centre.
///
/// Construct with [`TouchModel::unbiased`] (aliased by [`Default`]). Feed real
/// taps with [`observe`](TouchModel::observe) and read the learned bias with
/// [`offset`](TouchModel::offset).
#[derive(Debug, Clone, Default)]
pub struct TouchModel {
    means: HashMap<KeyId, Mean>,
}

impl TouchModel {
    /// A neutral model that has learned nothing: every key reports offset
    /// `(0.0, 0.0)`. This is the default model the Wave-2 decoder decodes
    /// against (ADR-15), so an untrained keyboard behaves exactly like the
    /// unbiased nearest-key skeleton.
    #[must_use]
    pub fn unbiased() -> Self {
        Self {
            means: HashMap::new(),
        }
    }

    /// `true` while no key has any learned bias yet (a fresh or reset model).
    #[must_use]
    pub fn is_unbiased(&self) -> bool {
        self.means.is_empty()
    }

    /// Fold one observed touch offset for `key` into that key's running mean.
    ///
    /// `dx`/`dy` are the offset of the actual touch from the key's centre, in
    /// the same surface-local pixels as [`featherkey_kernel::TouchPoint`]. The
    /// update is O(1) and allocation-free after the key's first observation,
    /// keeping the fast-typing path non-blocking (BR-46).
    ///
    /// # Errors
    /// [`TouchModelError::NonFiniteOffset`] if `dx` or `dy` is `NaN`/infinite,
    /// **or** if folding this (finite) sample would drive the key's accumulated
    /// running mean non-finite. In either case the model is left unchanged so no
    /// single sample can poison the learned mean.
    pub fn observe(&mut self, key: KeyId, dx: f32, dy: f32) -> Result<(), TouchModelError> {
        if !dx.is_finite() || !dy.is_finite() {
            return Err(TouchModelError::NonFiniteOffset);
        }
        // `or_default` only materialises an entry for an unseen key, whose first
        // fold (`mean = sample`) is finite whenever the sample is, so a rejected
        // update never leaves a poisoned entry behind. An existing key that
        // rejects keeps its prior mean untouched (`push` did not mutate).
        if self.means.entry(key).or_default().push(dx, dy) {
            Ok(())
        } else {
            Err(TouchModelError::NonFiniteOffset)
        }
    }

    /// The learned bias for `key`: the mean `(dx, dy)` of every offset observed
    /// for it, or `(0.0, 0.0)` if the key has never been observed (an unbiased
    /// key). The decoder subtracts this to re-centre the user's taps.
    #[must_use]
    pub fn offset(&self, key: KeyId) -> (f32, f32) {
        self.means.get(&key).map_or((0.0, 0.0), |m| (m.dx, m.dy))
    }

    /// How many valid observations have been folded into `key`'s mean. Zero for
    /// an unseen key. Exposed so consumers can weight a learned offset by its
    /// sample size (a one-tap mean is weaker evidence than a hundred-tap one).
    #[must_use]
    pub fn observations(&self, key: KeyId) -> u64 {
        self.means.get(&key).map_or(0, |m| m.count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: KeyId = KeyId('a');
    const B: KeyId = KeyId('b');

    #[test]
    fn unbiased_model_reports_zero_offset_for_every_key() {
        let model = TouchModel::unbiased();
        assert!(model.is_unbiased());
        assert_eq!(model.offset(A), (0.0, 0.0));
        assert_eq!(model.offset(KeyId('z')), (0.0, 0.0));
        assert_eq!(model.observations(A), 0);
    }

    #[test]
    fn default_matches_unbiased() {
        let d = TouchModel::default();
        assert!(d.is_unbiased());
        assert_eq!(d.offset(A), (0.0, 0.0));
    }

    #[test]
    fn a_single_observation_becomes_the_offset() {
        let mut model = TouchModel::unbiased();
        assert_eq!(model.observe(A, 2.0, -3.0), Ok(()));
        assert!(!model.is_unbiased());
        assert_eq!(model.offset(A), (2.0, -3.0));
        assert_eq!(model.observations(A), 1);
    }

    #[test]
    fn repeated_observations_average_toward_the_mean() {
        let mut model = TouchModel::unbiased();
        // Mean of {4, 6} is 5 in x; mean of {2, -2} is 0 in y.
        assert_eq!(model.observe(A, 4.0, 2.0), Ok(()));
        assert_eq!(model.observe(A, 6.0, -2.0), Ok(()));
        let (dx, dy) = model.offset(A);
        assert!((dx - 5.0).abs() < 1e-6, "dx was {dx}");
        assert!(dy.abs() < 1e-6, "dy was {dy}");
        assert_eq!(model.observations(A), 2);
    }

    #[test]
    fn the_running_mean_converges_to_the_true_offset() {
        let mut model = TouchModel::unbiased();
        // A user who consistently taps 3px right / 1px low of 'a'.
        for _ in 0..100 {
            assert_eq!(model.observe(A, 3.0, 1.0), Ok(()));
        }
        let (dx, dy) = model.offset(A);
        assert!((dx - 3.0).abs() < 1e-4, "dx was {dx}");
        assert!((dy - 1.0).abs() < 1e-4, "dy was {dy}");
        assert_eq!(model.observations(A), 100);
    }

    #[test]
    fn keys_learn_independently() {
        let mut model = TouchModel::unbiased();
        assert_eq!(model.observe(A, 1.0, 1.0), Ok(()));
        assert_eq!(model.observe(B, -5.0, 4.0), Ok(()));
        assert_eq!(model.offset(A), (1.0, 1.0));
        assert_eq!(model.offset(B), (-5.0, 4.0));
        // Learning one key never perturbs another.
        assert_eq!(model.offset(KeyId('c')), (0.0, 0.0));
    }

    #[test]
    fn nan_offset_is_rejected_without_mutating_the_model() {
        let mut model = TouchModel::unbiased();
        assert_eq!(model.observe(A, 2.0, 2.0), Ok(()));
        assert_eq!(
            model.observe(A, f32::NAN, 0.0),
            Err(TouchModelError::NonFiniteOffset)
        );
        // The good sample survives; the bad one changed nothing.
        assert_eq!(model.offset(A), (2.0, 2.0));
        assert_eq!(model.observations(A), 1);
    }

    #[test]
    fn infinite_offset_is_rejected_on_either_axis() {
        let mut model = TouchModel::unbiased();
        assert_eq!(
            model.observe(A, f32::INFINITY, 0.0),
            Err(TouchModelError::NonFiniteOffset)
        );
        assert_eq!(
            model.observe(A, 0.0, f32::NEG_INFINITY),
            Err(TouchModelError::NonFiniteOffset)
        );
        // A rejected first observation leaves the key entirely unseen.
        assert!(model.is_unbiased());
        assert_eq!(model.observations(A), 0);
    }

    #[test]
    fn accumulated_mean_drift_to_non_finite_is_rejected() {
        let mut model = TouchModel::unbiased();
        // Two finite samples whose *difference* overflows f32 to infinity: the
        // inputs pass the finiteness gate but the fold would poison the mean.
        let big = 3.0e38_f32;
        assert!(big.is_finite() && (-big).is_finite());
        assert_eq!(model.observe(A, -big, -big), Ok(()));
        let before = model.offset(A);
        assert!(before.0.is_finite() && before.1.is_finite());

        // The drifting update is refused, not stored.
        assert_eq!(
            model.observe(A, big, big),
            Err(TouchModelError::NonFiniteOffset)
        );
        // Prior mean is untouched and still finite/usable.
        assert_eq!(model.offset(A), before);
        let (dx, dy) = model.offset(A);
        assert!(dx.is_finite() && dy.is_finite());
        assert_eq!(model.observations(A), 1);

        // The model as a whole stays usable: further sane learning still lands.
        assert_eq!(model.observe(B, 1.0, 1.0), Ok(()));
        assert_eq!(model.offset(B), (1.0, 1.0));
        assert_eq!(model.observe(A, -big, -big), Ok(()));
        assert!(model.offset(A).0.is_finite() && model.offset(A).1.is_finite());
    }

    #[test]
    fn is_deterministic_for_a_fixed_observation_sequence() {
        let feed = |m: &mut TouchModel| {
            let _ = m.observe(A, 1.0, 2.0);
            let _ = m.observe(A, 3.0, 4.0);
            let _ = m.observe(B, -1.0, 0.5);
        };
        let mut a = TouchModel::unbiased();
        let mut b = TouchModel::unbiased();
        feed(&mut a);
        feed(&mut b);
        assert_eq!(a.offset(A), b.offset(A));
        assert_eq!(a.offset(B), b.offset(B));
    }

    #[test]
    fn error_displays_a_human_message() {
        let msg = format!("{}", TouchModelError::NonFiniteOffset);
        assert_eq!(msg, "touch offset was not finite");
    }
}
