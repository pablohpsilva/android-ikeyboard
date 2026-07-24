# crash-guard

**Its ONE job:** Isolate panics at the FFI seam and provide safe-mode fallback.

**Layer:** adapter (`[package.metadata.featherkey] layer = "adapter"`).

**Ports:** none. This crate declares no dependencies at all — not even `kernel` or `contracts`. It builds only on `std` (`std::panic::catch_unwind`) and implements/offers no port trait from `contracts`.

## What it does today

This is the **host-testable core half** of `crash-guard`. It wraps `std::panic::catch_unwind` into two entry points:

- `guard(f, fallback)` — run a closure; if it panics, return the caller-supplied `fallback` instead of unwinding.
- `guard_result(f)` — the same isolation, surfacing a caught panic as a `GuardError::Panicked(Option<String>)` value (message preserved for `&str`/`String` payloads, `None` otherwise).

`GuardError` is `Debug + Clone + PartialEq + Eq`, implements `Display`, and implements `std::error::Error`.

**Deferred to a later wave (v1.x):** the actual FFI-seam wiring and the watchdog that restarts a wedged surface are not in this crate yet. Today's code is exercised entirely on the host; it provides the panic-isolation primitive that the seam will use.

## Invariants

- **Panic containment:** a panic inside a guarded closure is caught at the boundary and never propagates past the guard; control returns normally.
- **Errors as values:** `guard_result` converts a panic into an inspectable `GuardError` rather than re-raising it; the guard functions never panic themselves.
- **Single responsibility:** this crate is the one sanctioned place in the workspace for `catch_unwind`.

## Serves (BRs)

BR-29, BR-30, BR-31.

## Tests

Inline `#[cfg(test)]` module in `src/lib.rs` covers the normal path, the fallback-on-panic path, and all three payload branches (`&str`, `String`, non-string), plus `Display` and the `std::error::Error` trait object. No `tests/` directory and no proptests at this stage.
