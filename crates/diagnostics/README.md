# featherkey-diagnostics

**Its ONE job:** Maintain the opt-in, content-free local diagnostics ring buffer — recording *what happened, never what was typed*.

## Layer

`domain` (per `[package.metadata.featherkey] layer`).

## Ports

Consumes the `Clock` port trait from the `contracts` crate: timestamps come from an injected `C: Clock` rather than a wall-clock read, keeping the buffer deterministic and host-testable. This crate implements no port trait of its own.

**Dependencies:** `featherkey-contracts` only.

## Invariants

- **Content-free by construction.** An event is a fixed `DiagnosticCode` plus a `u64` timestamp. `DiagnosticEvent` is `Copy` — it holds only scalars, which is the compile-time proof that no owned user text (`String`, key chars, coordinates) can ever live in the buffer (BR-60, SEDD §5.4 boundary).
- **Bounded memory.** A fixed-capacity ring buffer: once full, `record` overwrites the oldest event. Capacity never exceeded.
- **Infallible hot path.** `record` is O(1) and allocation-free after construction — no failure mode while recording.
- **Errors are values, never panics.** `new(0, ..)` returns `DiagnosticsError::ZeroCapacity` rather than panicking (SEDD §5.5).
- **Injected time.** Timestamps are read only through the `Clock` port; the crate never touches the system clock.

## Deferred to v1.x

- **Redacted export (BR-61)** is intentionally **not** implemented here. This crate's sole responsibility is the in-memory buffer; export is a v1.x concern.

## Serves (BRs)

BR-60, BR-61.

## Tests

Inline `#[cfg(test)]` module in `src/lib.rs` covering zero-capacity errors, in-order recording with injected-clock timestamps, wrap/overwrite behaviour at the capacity boundary, and a compile-time `Copy` proof that events carry no owned text. No `tests/` directory and no proptests.
