//! Opt-in, content-free local diagnostics ring buffer (SEDD §5.2, BR-60).
//!
//! This crate records **what happened, never what was typed**. An event is a
//! fixed [`DiagnosticCode`] plus a timestamp obtained through the injected
//! [`Clock`] port — there is no `String`, no key char, no user text anywhere in
//! the type, so the buffer is structurally incapable of leaking content
//! (SEDD §5.4 boundary invariant). It is a *ring* buffer: once full, recording a
//! new event drops the oldest, bounding memory without ever failing on the hot
//! path.
//!
//! BR-61 (redacted export) is a v1.x concern and is intentionally **not**
//! implemented here; this crate's sole responsibility is the in-memory buffer.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

use featherkey_contracts::Clock;

/// A content-free diagnostic event code. Each variant names a *category* of
/// occurrence; none carries user text, coordinates, or any other content
/// (BR-60). New categories are added over time, hence `#[non_exhaustive]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DiagnosticCode {
    /// The keyboard core finished initializing.
    Startup,
    /// The active layout changed (e.g. alphabetic ↔ symbols).
    LayoutSwitched,
    /// A decode returned an error value rather than candidates.
    DecodeError,
    /// A secure-store write returned an error.
    StoreWriteFailed,
    /// A secure-store read returned an error.
    StoreReadFailed,
    /// Learning/prediction was suppressed for a sensitive field (BR-26).
    SensitiveFieldSuppressed,
    /// A clipboard entry passed its expiry and was dropped.
    ClipboardExpired,
}

/// A single recorded event: a [`DiagnosticCode`] and the millisecond timestamp
/// at which it was recorded. Deliberately `Copy` — it holds only plain scalars,
/// which is the compile-time proof that no owned text can ever live here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticEvent {
    code: DiagnosticCode,
    at_millis: u64,
}

impl DiagnosticEvent {
    /// The category of what occurred.
    #[must_use]
    pub const fn code(self) -> DiagnosticCode {
        self.code
    }

    /// Milliseconds (per the injected [`Clock`]) at which it was recorded.
    #[must_use]
    pub const fn at_millis(self) -> u64 {
        self.at_millis
    }
}

/// Errors constructing a [`Diagnostics`] buffer. Errors are values, never panics
/// (SEDD §5.5 rule 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticsError {
    /// A ring buffer must hold at least one event; `capacity` was zero.
    ZeroCapacity,
}

impl fmt::Display for DiagnosticsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticsError::ZeroCapacity => f.write_str("diagnostics capacity must be non-zero"),
        }
    }
}

/// A fixed-capacity ring buffer of content-free [`DiagnosticEvent`]s.
///
/// Timestamps come from the injected `C: Clock` port, keeping the buffer
/// deterministic and host-testable (no wall-clock reads). When full, the oldest
/// event is overwritten so recording is O(1) and allocation-free after
/// construction — no failure mode on the hot path.
#[derive(Debug, Clone)]
pub struct Diagnostics<C: Clock> {
    clock: C,
    buf: Vec<DiagnosticEvent>,
    capacity: usize,
    /// Index of the oldest event once `buf` is full; otherwise unused (0).
    head: usize,
}

impl<C: Clock> Diagnostics<C> {
    /// Create a buffer holding up to `capacity` most-recent events.
    ///
    /// # Errors
    /// [`DiagnosticsError::ZeroCapacity`] if `capacity` is 0.
    pub fn new(capacity: usize, clock: C) -> Result<Self, DiagnosticsError> {
        if capacity == 0 {
            return Err(DiagnosticsError::ZeroCapacity);
        }
        Ok(Self {
            clock,
            buf: Vec::with_capacity(capacity),
            capacity,
            head: 0,
        })
    }

    /// The maximum number of events retained.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// The number of events currently retained (`<= capacity`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// `true` if no events have been recorded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Record `code`, stamping it with the current [`Clock`] time. When the
    /// buffer is full the oldest event is dropped. Infallible by construction.
    pub fn record(&mut self, code: DiagnosticCode) {
        let event = DiagnosticEvent {
            code,
            at_millis: self.clock.now_millis(),
        };
        if self.buf.len() < self.capacity {
            self.buf.push(event);
        } else {
            // `head < capacity == buf.len()`, so the index is always in bounds.
            self.buf[self.head] = event;
            self.head = (self.head + 1) % self.capacity;
        }
    }

    /// A copy of the retained events, oldest first.
    #[must_use]
    pub fn snapshot(&self) -> Vec<DiagnosticEvent> {
        let mut out = Vec::with_capacity(self.buf.len());
        if self.buf.len() < self.capacity {
            out.extend_from_slice(&self.buf);
        } else {
            // Full: the oldest lives at `head`; splice the two halves in order.
            out.extend_from_slice(&self.buf[self.head..]);
            out.extend_from_slice(&self.buf[..self.head]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    /// A clock that returns a preset time and advances by one on each read, so
    /// tests can prove timestamps come from the injected port and are ordered.
    #[derive(Debug)]
    struct AdvancingClock(Cell<u64>);

    impl AdvancingClock {
        fn starting_at(t: u64) -> Self {
            Self(Cell::new(t))
        }
    }

    impl Clock for AdvancingClock {
        fn now_millis(&self) -> u64 {
            let t = self.0.get();
            self.0.set(t + 1);
            t
        }
    }

    #[derive(Debug)]
    struct FixedClock(u64);
    impl Clock for FixedClock {
        fn now_millis(&self) -> u64 {
            self.0
        }
    }

    fn codes(d: &Diagnostics<impl Clock>) -> Vec<DiagnosticCode> {
        d.snapshot().iter().map(|e| e.code()).collect()
    }

    #[test]
    fn zero_capacity_is_an_error_not_a_panic() {
        let err = Diagnostics::new(0, FixedClock(0));
        assert_eq!(err.err(), Some(DiagnosticsError::ZeroCapacity));
    }

    #[test]
    fn error_displays_human_message() {
        assert_eq!(
            alloc::format!("{}", DiagnosticsError::ZeroCapacity),
            "diagnostics capacity must be non-zero"
        );
    }

    #[test]
    fn fresh_buffer_is_empty() {
        let d = Diagnostics::new(3, FixedClock(0)).unwrap();
        assert!(d.is_empty());
        assert_eq!(d.len(), 0);
        assert_eq!(d.capacity(), 3);
        assert!(d.snapshot().is_empty());
    }

    #[test]
    fn records_below_capacity_in_order_with_clock_timestamps() {
        let mut d = Diagnostics::new(4, AdvancingClock::starting_at(100)).unwrap();
        d.record(DiagnosticCode::Startup);
        d.record(DiagnosticCode::LayoutSwitched);

        assert!(!d.is_empty());
        assert_eq!(d.len(), 2);
        let snap = d.snapshot();
        assert_eq!(
            snap.iter().map(|e| e.code()).collect::<Vec<_>>(),
            [DiagnosticCode::Startup, DiagnosticCode::LayoutSwitched]
        );
        // Timestamps are injected by the Clock port and strictly increasing.
        assert_eq!(snap[0].at_millis(), 100);
        assert_eq!(snap[1].at_millis(), 101);
    }

    #[test]
    fn wraps_at_capacity_dropping_the_oldest() {
        let mut d = Diagnostics::new(3, FixedClock(7)).unwrap();
        for code in [
            DiagnosticCode::Startup,
            DiagnosticCode::LayoutSwitched,
            DiagnosticCode::DecodeError,
            DiagnosticCode::StoreWriteFailed,
            DiagnosticCode::StoreReadFailed,
        ] {
            d.record(code);
        }
        // Never exceeds capacity; only the 3 most recent survive, oldest first.
        assert_eq!(d.len(), 3);
        assert_eq!(
            codes(&d),
            [
                DiagnosticCode::DecodeError,
                DiagnosticCode::StoreWriteFailed,
                DiagnosticCode::StoreReadFailed,
            ]
        );
    }

    #[test]
    fn exactly_full_then_one_more_advances_the_head_correctly() {
        // Exercises the full-but-head==0 boundary and a subsequent overwrite.
        let mut d = Diagnostics::new(2, FixedClock(0)).unwrap();
        d.record(DiagnosticCode::Startup);
        d.record(DiagnosticCode::LayoutSwitched);
        assert_eq!(
            codes(&d),
            [DiagnosticCode::Startup, DiagnosticCode::LayoutSwitched]
        );
        d.record(DiagnosticCode::DecodeError);
        assert_eq!(
            codes(&d),
            [DiagnosticCode::LayoutSwitched, DiagnosticCode::DecodeError]
        );
    }

    #[test]
    fn capacity_one_keeps_only_the_latest() {
        let mut d = Diagnostics::new(1, FixedClock(0)).unwrap();
        d.record(DiagnosticCode::SensitiveFieldSuppressed);
        d.record(DiagnosticCode::ClipboardExpired);
        assert_eq!(d.len(), 1);
        assert_eq!(codes(&d), [DiagnosticCode::ClipboardExpired]);
    }

    #[test]
    fn events_are_content_free_and_therefore_copy() {
        // A `String` is not `Copy`; that this compiles is the structural proof
        // that a DiagnosticEvent carries no owned user text (BR-60).
        fn assert_copy<T: Copy>(_: &T) {}
        let e = DiagnosticEvent {
            code: DiagnosticCode::Startup,
            at_millis: 1,
        };
        assert_copy(&e);
        let clone = e;
        assert_eq!(e, clone);
        assert_eq!(e.code(), DiagnosticCode::Startup);
    }
}
