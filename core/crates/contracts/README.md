# featherkey-contracts

**Its ONE job:** Define the port traits (driven & driving) that domain crates depend on instead of adapters — no logic, no dependencies beyond `kernel` (ADR-12).

**Layer:** `port` (`[package.metadata.featherkey] layer = "port"`). A port-layer crate: it may depend only on the foundation.

## Ports

This crate *offers* the port traits; adapters and domain crates elsewhere implement or consume them. It does not implement any port itself.

- **Driven ports** (the app needs an adapter to satisfy these):
  - `SecureStore` — the sole persist/encrypt boundary for personal data, keyed by `Namespace`; returns `StoreError` values, never panics.
  - `SensitiveContextSource` — reports whether the current editor field is sensitive, so learning/prediction can be suppressed (BR-26).
  - `Clock` — injected monotonic millisecond time source.
- **Driving ports** (behaviour the app offers):
  - `Predictor` — completions / next-word predictions over a `TypingContext`, yielding ranked `Suggestions`.
  - `AutoCorrect` — decides whether/how to correct a `Token`, yielding a `Correction`.

Supporting value types: `Namespace`, `StoreError`, `TypingContext`, `Suggestion`, `Suggestions`, `Token`, `Correction`.

**Dependencies:** `featherkey-kernel` only.

**Deferred to v1.x:** a `Personalization` port (over a `TypingEvent` type) is intentionally not defined here; it is added alongside the crate that introduces its types, rather than seeded as a placeholder.

## Invariants

- **`#![no_std]`** (uses `alloc` only) — pure trait/type definitions, no I/O and no logic.
- **Errors are values** — `SecureStore` returns `Result<_, StoreError>`; the contract never panics.
- **Stable, distinct namespace keys** — each `Namespace::as_str()` is a fixed, collision-free storage table name.
- **No-clobber intent** — `Correction::applied == false` means the typed token is returned unchanged (BR-12); the guarantee is verified where the trait is implemented (`autocorrect` crate), not here.

**Serves (BRs):** all.

**Tests:** inline `#[cfg(test)]` in `src/lib.rs` only — stub adapters prove every port is implementable and exercise the value types' derived traits. No `tests/` directory and no property tests in this crate.
