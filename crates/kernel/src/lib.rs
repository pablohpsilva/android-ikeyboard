//! Shared value objects and error types crossing module boundaries.
//!
//! This crate is the root of the dependency DAG: it has **no dependencies** and
//! contains **no logic** beyond trivial constructors/accessors on plain data
//! (SEDD §5.2, §5.5). Every other core crate may depend on `kernel`; `kernel`
//! depends on nothing.

#![no_std]

/// A touch location on the keyboard surface, in surface-local coordinates.
///
/// Units are device-independent pixels; the origin is the top-left of the
/// keyboard view. Coordinates are `f32` because touch input is sub-pixel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchPoint {
    pub x: f32,
    pub y: f32,
}

impl TouchPoint {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// The identity of a key: the character it commits when pressed.
///
/// This is deliberately minimal for the walking skeleton. Non-character keys
/// (shift, backspace, layout switches) get their own variant when
/// `layout-engine` grows beyond the tracer bullet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyId(pub char);

impl KeyId {
    #[must_use]
    pub const fn ch(self) -> char {
        self.0
    }
}

/// A normalized confidence score in the inclusive range `[0.0, 1.0]`.
///
/// Construction clamps out-of-range inputs rather than panicking (SEDD §5.5
/// rule 3: errors/edge cases are values, not panics).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Confidence(f32);

impl Confidence {
    /// Clamp `value` into `[0.0, 1.0]`. `NaN` maps to `0.0`.
    #[must_use]
    pub fn new(value: f32) -> Self {
        if value.is_nan() {
            return Self(0.0);
        }
        Self(value.clamp(0.0, 1.0))
    }

    #[must_use]
    pub const fn value(self) -> f32 {
        self.0
    }
}

/// Errors returned across core-module boundaries.
///
/// Core functions return `Result<_, CoreError>`; panics are reserved for truly
/// unreachable states and are caught at the FFI seam by `crash-guard`
/// (SEDD §5.5 rule 3, EP-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoreError {
    /// A decode was requested against a layout with no keys.
    EmptyLayout,
}

impl core::fmt::Display for CoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CoreError::EmptyLayout => f.write_str("layout contains no keys"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_point_stores_coordinates() {
        let p = TouchPoint::new(3.5, -1.0);
        assert_eq!(p.x, 3.5);
        assert_eq!(p.y, -1.0);
        assert_eq!(p, TouchPoint::new(3.5, -1.0));
    }

    #[test]
    fn key_id_exposes_its_char() {
        let k = KeyId('f');
        assert_eq!(k.ch(), 'f');
        assert_eq!(k, KeyId('f'));
    }

    #[test]
    fn confidence_clamps_into_unit_range() {
        assert_eq!(Confidence::new(0.5).value(), 0.5);
        assert_eq!(Confidence::new(-2.0).value(), 0.0);
        assert_eq!(Confidence::new(9.0).value(), 1.0);
    }

    #[test]
    fn confidence_treats_nan_as_zero() {
        assert_eq!(Confidence::new(f32::NAN).value(), 0.0);
    }

    #[test]
    fn core_error_displays_human_message() {
        // Exercises the Display arm so diagnostics never show a debug blob.
        extern crate alloc;
        let msg = alloc::format!("{}", CoreError::EmptyLayout);
        assert_eq!(msg, "layout contains no keys");
    }
}
