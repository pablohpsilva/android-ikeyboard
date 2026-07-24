# featherkey-sensitive-context

**Its ONE job:** Decide whether the current editor field is sensitive and must therefore suppress learning and prediction (the BR-26 gate).

## Layer

`domain` (per `[package.metadata.featherkey]` in `Cargo.toml`). Pure logic, `#![no_std]`, no I/O and no state.

## Ports

Consumes the driven port `SensitiveContextSource` from the `contracts` crate — the shell supplies the reading; this crate turns it into a suppress/allow verdict. It does not define or offer any port of its own.

Dependencies (from `Cargo.toml`): `featherkey-contracts` only.

## API

- `SensitivityPolicy` — a zero-sized, `Copy` gate. `new()` / `default()` are equivalent; there is nothing to configure.
- `should_suppress(&self, src: &dyn SensitiveContextSource) -> bool` — returns `true` when the field is sensitive. It currently delegates directly to `src.is_sensitive()`.

## Invariants

- **Purity / no-std:** stateless by construction; the verdict depends only on the source, so the same source always yields the same result (referential transparency).
- **E-2 ordering:** the composition root MUST call this gate *before* any learner or predictor runs. Suppression is decided up front, so a sensitive field never reaches the learner or predictor — there is no code path that persists a keystroke typed into a password field. Enforcing the call site is the composition root's responsibility, not this crate's.

## Deferred to v1.x

Any richer classification (per-field-type policy, OTP-specific handling, configurable rules) is deferred. Today the gate is a straight pass-through of the source's `is_sensitive()` reading.

## Serves (BRs)

- BR-26 — sensitive fields structurally cannot be learned.

## Tests

Inline `#[cfg(test)]` unit tests in `src/lib.rs` (sensitive vs. ordinary source, `new`/`default` agreement) and a cross-boundary acceptance test in `tests/br26_gate.rs` (the executable form of `features/sensitive-context.feature`). No proptests.
