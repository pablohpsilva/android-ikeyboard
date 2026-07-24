//! Behavioural slice for the tap-geometry model — the executable form of the
//! scenarios in `features/touch-model.feature`.
//!
//! Traces to BR-7 (the keyboard learns the user's typing style) and BR-46 (the
//! per-tap update is O(1) and non-blocking, so learning never threatens the
//! fast-typing path). `touch-model` is the sole writer of this data domain
//! (ADR-14); the decoder only ever reads an offset it produces (ADR-15).

use featherkey_kernel::KeyId;
use featherkey_touch_model::{TouchModel, TouchModelError};

/// BR-7: a consistent tap bias is learned, so the model's offset points the
/// decoder back at the key the user actually means.
#[test]
fn the_model_learns_a_consistent_tap_bias() {
    let key = KeyId('e');
    let mut model = TouchModel::unbiased();

    // Wave-2 default (ADR-15): before any tap, the model is neutral.
    assert_eq!(model.offset(key), (0.0, 0.0));

    // The user habitually taps a little low-and-right of 'e'.
    for _ in 0..50 {
        assert_eq!(model.observe(key, 4.0, 6.0), Ok(()));
    }

    let (dx, dy) = model.offset(key);
    assert!(
        (dx - 4.0).abs() < 1e-3,
        "learned dx {dx} should approach 4.0"
    );
    assert!(
        (dy - 6.0).abs() < 1e-3,
        "learned dy {dy} should approach 6.0"
    );
    assert_eq!(model.observations(key), 50);
}

/// BR-46: even a hostile sample can never unwind the input thread — a
/// non-finite offset is refused as a value, and the previously learned bias is
/// untouched.
#[test]
fn a_bad_sample_never_corrupts_the_fast_path() {
    let key = KeyId('t');
    let mut model = TouchModel::unbiased();
    assert_eq!(model.observe(key, 2.0, 2.0), Ok(()));

    assert_eq!(
        model.observe(key, f32::NAN, 1.0),
        Err(TouchModelError::NonFiniteOffset)
    );

    // The good learning survives; nothing was dropped or reordered.
    assert_eq!(model.offset(key), (2.0, 2.0));
    assert_eq!(model.observations(key), 1);
}
