# featherkey-touch-model

**Its ONE job:** Maintain the per-user adaptive tap-distribution model — the sole writer of tap geometry (ADR-14).

## Layer

`domain` (per `[package.metadata.featherkey] layer` in `Cargo.toml`).

## Ports

Implements and offers no `contracts` port traits. It depends only on `featherkey-kernel` (for `KeyId`); there is no dependency on `contracts`. It performs no I/O, crypto, or persistence — persistence is `secure-store`'s job (SEDD §5.4) — and reads happen through an immutable snapshot consumed by `input-decoder` (ADR-15), which never mutates the model.

## Invariants

- **Sole writer of tap geometry** — the only crate that mutates the learned tap-offset domain (ADR-14).
- **Pure & deterministic** — the model is a deterministic function of the observation sequence fed to it; no I/O, no crypto, no hidden state.
- **No-poison on bad input** — a non-finite `dx`/`dy`, or a fold that would drive a key's accumulated running mean non-finite, is rejected as `TouchModelError::NonFiniteOffset` and leaves the model unchanged. Errors are values; the hot path never panics (SEDD §5.5).
- **O(1), allocation-free per tap** — each `observe` folds one sample via Welford's incremental mean after the key's first observation, keeping the fast-typing path non-blocking (BR-46).
- **Overflow-safe count** — per-key observation count is a saturating `u64`, so an unbounded tap stream can never wrap or divide-by-zero.
- **Unbiased default** — a fresh model reports `(0.0, 0.0)` for every key, exactly the neutral input the Wave-2 decoder defaults to (ADR-15).

## Serves (BRs)

BR-7 (learn the user's typing style), BR-46 (O(1), allocation-free per-tap update).

## Tests

Inline `#[cfg(test)]` unit tests in `src/lib.rs` covering unbiased defaults, running-mean convergence, per-key independence, non-finite rejection, accumulated-drift rejection, and determinism, plus integration tests in `tests/learning_improves_targeting.rs` (consistent-bias learning and the bad-sample-never-corrupts-the-fast-path guard). No proptests yet — property-based coverage of these invariants is deferred to v1.x.
