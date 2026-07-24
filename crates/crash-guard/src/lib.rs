//! Isolate panics behind a safe boundary so a single failure never unwinds past
//! the seam and takes the keyboard down with it (BR-29, BR-30, BR-31).
//!
//! This is the **host-testable core half** of `crash-guard`. It wraps
//! [`std::panic::catch_unwind`] into two ergonomic entry points:
//!
//! * [`guard`] — run a closure and, if it panics, return a caller-supplied
//!   `fallback` instead of unwinding (the "safe-mode" recovery of BR-29/30).
//! * [`guard_result`] — the same isolation, but surfacing the failure as a
//!   [`GuardError`] value so callers can log/telemetry it (errors are values,
//!   SEDD §5.5 r3).
//!
//! A guarded closure that panics is *caught*: control returns normally and the
//! panic never propagates past the guard. This crate is the one place in the
//! workspace where `catch_unwind` is legitimate — that is its single
//! responsibility (SEDD §5.2). The FFI-seam wiring and the watchdog that
//! restarts a wedged surface are Wave 5; this half is exercised entirely on the
//! host.

use std::any::Any;
use std::fmt;
use std::panic::{catch_unwind, UnwindSafe};

/// The reason a guarded closure did not produce a value.
///
/// Returned by [`guard_result`]. Errors are values, never panics on the hot
/// path (SEDD §5.5 r3); the panic is converted here into an inspectable variant.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GuardError {
    /// The guarded closure panicked and was caught at the boundary.
    ///
    /// The panic message is preserved when the payload was a `&str` or
    /// `String` (the common `panic!("...")` shapes); for any other payload type
    /// the message is `None` because it cannot be rendered as text.
    Panicked(Option<String>),
}

impl fmt::Display for GuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuardError::Panicked(Some(msg)) => write!(f, "guarded closure panicked: {msg}"),
            GuardError::Panicked(None) => f.write_str("guarded closure panicked"),
        }
    }
}

impl std::error::Error for GuardError {}

/// Run `f`, returning its value; if `f` panics, return `fallback` instead.
///
/// The panic is caught at this boundary and never unwinds past it, so a failure
/// deep inside a decode/predict step degrades to `fallback` rather than tearing
/// down the caller (BR-29, BR-30).
pub fn guard<T>(f: impl FnOnce() -> T + UnwindSafe, fallback: T) -> T {
    guard_result(f).unwrap_or(fallback)
}

/// Run `f`, returning `Ok(value)`; if `f` panics, catch it at the boundary and
/// return [`GuardError::Panicked`] carrying the panic message when available.
///
/// # Errors
/// Returns [`GuardError::Panicked`] if `f` unwinds. This function itself never
/// panics and never lets a panic escape.
pub fn guard_result<T>(f: impl FnOnce() -> T + UnwindSafe) -> Result<T, GuardError> {
    catch_unwind(f).map_err(|payload| GuardError::Panicked(payload_message(&*payload)))
}

/// Best-effort extraction of a human-readable message from a caught panic
/// payload. Covers the two payload types the standard `panic!` macro produces
/// (`&'static str` and `String`); anything else yields `None`.
fn payload_message(payload: &(dyn Any + Send)) -> Option<String> {
    if let Some(s) = payload.downcast_ref::<&str>() {
        Some((*s).to_owned())
    } else {
        payload.downcast_ref::<String>().cloned()
    }
}

#[cfg(test)]
// These tests deliberately trigger panics to prove the guard catches them; the
// workspace `panic` lint (aimed at production hot paths) does not apply here.
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use std::panic;
    use std::sync::Once;

    /// Silence the default panic hook for this test binary so the expected,
    /// deliberately-triggered panics don't spam stderr. Installed once; a real
    /// unexpected panic still fails the test via the caught `Err`/assert.
    fn silence_panic_hook() {
        static HOOK: Once = Once::new();
        HOOK.call_once(|| panic::set_hook(Box::new(|_| {})));
    }

    #[test]
    fn guard_returns_the_value_on_the_normal_path() {
        assert_eq!(guard(|| 7, -1), 7);
    }

    #[test]
    fn guard_returns_the_fallback_when_the_closure_panics() {
        silence_panic_hook();
        // The panic is caught: control reaches the assertion, nothing unwinds.
        let out = guard(|| panic!("boom"), 42);
        assert_eq!(out, 42);
    }

    #[test]
    fn guard_result_is_ok_on_the_normal_path() {
        assert_eq!(guard_result(|| "value"), Ok("value"));
    }

    #[test]
    fn guard_result_reports_a_str_panic_message() {
        silence_panic_hook();
        let err = guard_result(|| -> i32 { panic!("kaboom") });
        assert_eq!(err, Err(GuardError::Panicked(Some("kaboom".to_owned()))));
    }

    #[test]
    fn guard_result_reports_a_string_panic_payload() {
        silence_panic_hook();
        // A `String` payload (as produced by `panic_any(String)`) exercises the
        // owned-string downcast branch, distinct from the `&str` branch above.
        let err = guard_result(|| -> i32 { panic::panic_any(String::from("code 500")) });
        assert_eq!(err, Err(GuardError::Panicked(Some("code 500".to_owned()))));
    }

    #[test]
    fn guard_result_handles_a_non_string_panic_payload() {
        silence_panic_hook();
        // `panic_any` with a non-string type: no text is recoverable => None.
        let err = guard_result(|| -> i32 { panic::panic_any(500u32) });
        assert_eq!(err, Err(GuardError::Panicked(None)));
    }

    #[test]
    fn guard_error_displays_with_and_without_a_message() {
        let with = GuardError::Panicked(Some("oops".to_owned()));
        let without = GuardError::Panicked(None);
        assert_eq!(with.to_string(), "guarded closure panicked: oops");
        assert_eq!(without.to_string(), "guarded closure panicked");
    }

    #[test]
    fn guard_error_is_a_std_error() {
        // Confirms the error is usable through the `std::error::Error` trait
        // object callers log against.
        let err = GuardError::Panicked(None);
        let dyn_err: &dyn std::error::Error = &err;
        assert_eq!(dyn_err.to_string(), "guarded closure panicked");
    }
}
