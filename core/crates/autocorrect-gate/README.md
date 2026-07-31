# featherkey-autocorrect-gate

**Its ONE job:** Decide whether to trust an autocorrect — a tiny per-user neural gate over the structural features of one correction decision (`GateFeatures`).

## Layer

`domain` (per `[package.metadata.featherkey]` in `Cargo.toml`). Pure logic; no I/O, no persistence, no clock, no global state of its own.

## Ports

Depends on `featherkey-nn` (the tiny MLP substrate this crate's gate will be built on) and `featherkey-contracts` (port traits; `Namespace::AutocorrectGate` is the learned-data namespace this crate's persistence will eventually own).

## What it does

Today: holds `GateFeatures`, the five-slot feature vector describing one correction decision — `edit_distance`, `winner_confidence`, `dict_rank_norm`, `typed_len_norm`, `momentum_weight` — and `to_array`, which serializes it in slot order for a neural net to consume.

This is the substrate for the neural autocorrect gate; the model (a bounded MLP residual) and its persistence are added in later increments.

## Invariants

- **Slot order is the contract:** `to_array`'s element order must match the field order documented on `GateFeatures`; any consumer (the future MLP, its training loop) depends on this order being stable.
- **Pure math:** no I/O, no clock, no RNG — deterministic given its inputs, matching `featherkey-neural-ranker`'s design.

## Serves (BRs)

BR-15 (learned per-user autocorrect aggressiveness), behind BR-12's no-clobber veto, BR-22/BR-26 (consent + sensitive-field gating), BR-46 (off the decode hot path), BR-13 (fully on-device).

## Tests

Inline `#[cfg(test)]` module in `src/lib.rs` (feature serialization order).
