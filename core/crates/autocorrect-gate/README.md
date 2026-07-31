# featherkey-autocorrect-gate

**Its ONE job:** Decide whether to trust an autocorrect — a tiny per-user neural gate over the structural features of one correction decision (`GateFeatures`).

## Layer

`domain` (per `[package.metadata.featherkey]` in `Cargo.toml`). Pure logic; no I/O, no clock, no global state of its own. Persistence is I/O reached only through the injected `SecureStore` port (below).

## Ports

Depends on `featherkey-nn` (the tiny MLP substrate the gate is built on), `featherkey-contracts` (`SecureStore` port + `Namespace::AutocorrectGate`, the learned-data namespace this crate owns as sole writer).

## What it does

Holds `GateFeatures`, the five-slot feature vector describing one correction decision — `edit_distance`, `winner_confidence`, `dict_rank_norm`, `typed_len_norm`, `momentum_weight` — and `to_array`, which serializes it in slot order for the net.

`AutocorrectGate` wraps a tiny `featherkey_nn::Mlp` and exposes:

- `from_prior()` — cold start: a feature-sensitive centred-pair prior (two hidden units per feature, signed readers either side of a per-feature centre) whose cold-start residual is ~0 for any realistic feature vector, so autocorrect behaves as its base+floor policy until training moves the weights. The centred design means reinforcing one correction concentrates its gradient on the units *that* correction excited, leaving a differently-shaped correction almost untouched (no global collateral).
- `residual(&GateFeatures) -> f64` — the learned nudge on the apply threshold, clamped to `±RESIDUAL_BOUND` (1.5) so the gate can only nudge, never overturn, the no-clobber veto (which runs first, upstream in `featherkey-autocorrect`).
- `reinforce(&GateFeatures, target: f32, lr: f32)` — one pointwise SGD step toward `target` (the real-world outcome: revert → negative, kept/reached → positive).
- `persist(&impl SecureStore)` / `load(&impl SecureStore)` — encrypted round-trip of the whole model as one blob under `Namespace::AutocorrectGate`. `load` never errors on a missing, corrupt, or wrong-input-shape blob: all three degrade silently to `from_prior()` (today's base+floor policy), so a format change or damaged record cannot break autocorrect. Only a backend `StoreError` from the store's own `get`/`put` propagates.

## Invariants

- **Slot order is the contract:** `to_array`'s element order must match the field order documented on `GateFeatures`; the MLP's input layer and the training loop both depend on this order being stable.
- **Pure math, gated I/O:** no clock, no RNG — `residual`/`reinforce` are deterministic given their inputs, matching `featherkey-neural-ranker`'s design; the only I/O (`persist`/`load`) flows through the injected `SecureStore`, never touched directly.
- **Bounded residual:** `residual` is always clamped to `±RESIDUAL_BOUND`, even against a hand-built extreme model — the gate can shrink or grow the apply confidence but never itself apply a correction the no-clobber veto rejected.
- **Load never fails on data, only on the store:** absent, corrupt, or shape-mismatched blobs are not `Err` — they fall back to `from_prior()`. This crate adds no persistence error of its own.

## Limitations / Deferred

- **Small global bias shift:** the prior's centred design keeps collateral small but not zero — reinforcing (or reverting) one correction shifts a shared global bias term slightly, so an unrelated correction's residual can move a little (bounded; proven in `reverting_one_correction_does_not_suppress_another`). Feature-similar corrections co-suppress *by design* (that is the point of the centred readers), so this is expected, not a defect.
- **The REACHED counterfactual is low-weight/noisy:** the shell (`CorrectionDetector` in `apps/android/ime-service`) surfaces a withheld correction's target word so a later manual landing on it can train the gate toward "apply." That signal's in-field reach window is only loosely bounded — an intervening edit that isn't itself a checked manual-word commit (a swipe, a symbol/emoji, a whole-word delete-retype, a newline) can let the note outlive the correction it described. The shell bounds this as tightly as its event model allows without adding new device-side state; a residual noise floor on this one signal is accepted rather than engineered away here, since this crate's `reinforce` already treats every training call as one small, bounded nudge.

## Serves (BRs)

BR-15 (learned per-user autocorrect aggressiveness), behind BR-12's no-clobber veto, BR-22/BR-26 (consent + sensitive-field gating — enforced upstream, in `featherkey-core`/the shell), BR-46 (off the decode hot path), BR-13 (fully on-device).

## Tests

Inline `#[cfg(test)]` modules: `src/lib.rs` (feature serialization order, cold-start residual, revert-suppresses-one-not-another, residual bound, reinforce moves toward target — 6 tests) and `src/persist.rs` (round-trip through the store, absent/corrupt blob falls back to prior — 2 tests). 8 tests total.
