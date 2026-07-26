# featherkey-kernel

**Its ONE job:** Define shared value objects and error types that cross module boundaries — no logic, no dependencies.

**Layer:** foundation (`[package.metadata.featherkey] layer = "foundation"`). It is the root of the dependency DAG: every other core crate may depend on it; it depends on nothing.

**Ports:** None. This crate neither implements nor offers any port trait. It has zero dependencies — no `contracts`, no `kernel` (it *is* the root), no third-party crates (`[dependencies]` is empty, enforced by `tools/fitness/check.py`).

## Public API

- `TouchPoint` — a surface-local touch location (`x`, `y` as `f32`, device-independent pixels, origin top-left). `const fn new`.
- `KeyId(char)` — the identity of a key as the character it commits. `const fn ch`. Deliberately minimal for the walking skeleton; non-character keys (shift, backspace, layout switches) are **deferred to v1.x** and get their own variant when `layout-engine` grows.
- `Confidence(f32)` — a normalized score in `[0.0, 1.0]`.
- `CoreError` — `#[non_exhaustive]` enum of errors crossing core-module boundaries. Currently one variant, `EmptyLayout`; implements `Display`.

## Invariants

- **`#![no_std]`** — the crate compiles without the standard library.
- **Zero dependencies / purity** — no I/O, no allocation, no logic beyond trivial constructors and accessors on plain data (SEDD §5.2, §5.5).
- **No panics; edge cases are values** — `Confidence::new` *clamps* out-of-range input into `[0.0, 1.0]` and maps `NaN` to `0.0` rather than panicking (SEDD §5.5 rule 3).
- **Human-readable errors** — `CoreError` implements `Display` so diagnostics never surface a debug blob.

**Serves (BRs):** (all) — shared kernel used across every core module.

**Tests:** Inline `#[cfg(test)]` module in `src/lib.rs` covering `TouchPoint` storage, `KeyId` char access, `Confidence` clamping and `NaN` handling, and the `CoreError` `Display` arm. No `tests/` directory and no proptests.
