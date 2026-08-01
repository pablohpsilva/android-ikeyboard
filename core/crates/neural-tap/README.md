# featherkey-neural-tap

**Its ONE job:** Learn a per-user coordinate warp — a bounded `(Δx, Δy)` pixel shift over a normalized tap position `(nx, ny)` in `[-1,1]` — that generalizes a person's systematic tap bias across keys, rather than per-key.

## Layer

`domain` (per `[package.metadata.featherkey]` in `Cargo.toml`). Pure logic; no I/O, no clock, no RNG, no global state of its own.

## Ports

Depends on `featherkey-nn` (the tiny MLP substrate the warp is built on) and `featherkey-contracts` (for the `SecureStore` port and the learned-data `Namespace` this crate will own as sole writer, once persistence lands — see Deferred).

## What it does

`TapWarp` wraps two independent scalar `featherkey_nn::Mlp`s — one per axis (`dx`, `dy`) — over the same 2-input position vector `[nx, ny]`:

- `from_prior() -> Self` — cold start: each axis gets a centred signed-pair prior (two hidden units per input, symmetric positive/negative readers around centre `0.0`, identical construction to `AutocorrectGate::from_prior`). The prior's output is ≈0 across the whole `[-1,1]²` grid (proven within `< 0.05` for a 5×5 sample grid including the corners), yet every input weight sits on a live gradient path — a `TapWarp` is trainable from the very first `reinforce` call, not frozen until some threshold.
- `warp(&self, nx: f32, ny: f32) -> (f32, f32)` — the learned `(Δx, Δy)` for a normalized tap, each axis independently clamped to `±WARP_BOUND` (40 logical px) so a warp can nudge a tap toward the intended key but never fling it across the keyboard.
- `reinforce(&mut self, nx: f32, ny: f32, tx: f32, ty: f32, lr: f32)` — one squared-error SGD step per axis toward a target shift `(tx, ty)`. The target and its derivation (what "the intended shift was" means for a given tap/commit) are a caller concern; this crate only takes the target as given and moves toward it.

## Invariants

- **Bounded output:** `warp` always clamps to `±WARP_BOUND`, even against a hand-trained model driven far outside that range (proven in `warp_output_is_bounded` by 5,000 reinforcement steps toward an out-of-range target) — including a finiteness check, since a real per-axis MLP can be pushed to `inf`/`NaN` under extreme uncapped training and `f32::NAN.clamp(..)` would silently pass through NaN.
- **Cold start ≈ zero, not frozen:** the prior's near-zero output is a starting *value*, not a locked one — `axis_prior`'s per-input weight pair keeps a gradient path open on every input from the first `reinforce` call (mirrors `AutocorrectGate::from_prior`'s rationale).
- **No unbounded drift under a converged (zero-mean) target stream:** alternating positive/negative targets that average to zero keep the warp near zero rather than accumulating in one direction (proven in `a_zero_mean_target_stream_keeps_the_warp_near_zero`).
- **Pure math:** `warp`/`reinforce` are deterministic given their inputs and the current weights — no clock, no RNG, no I/O in this crate today.

## Limitations / Deferred

- **Persistence is not in this crate yet.** `TapWarp` today has no `persist`/`load` — encrypted round-trip through the injected `SecureStore` (the pattern `featherkey-autocorrect-gate` uses) is the next slice of this feature. `featherkey-contracts` is already a dependency in anticipation of that port; it is currently unused by `src/lib.rs` on its own.
- **The correction/target signal is deferred by design.** This crate defines what a warp *is* and how it trains given `(tx, ty)`; it says nothing about how a caller derives that target from an actual tap-vs-committed-key observation. That derivation (and any per-key vs. cross-key aggregation policy) belongs to the consumer that will wire `TapWarp` into the tap-decode path, not to this substrate.

## Serves (BRs)

BR-11-family neural roadmap, app #3 (tap-warp) — per-user coordinate-bias correction as a generalization of the tap-decoder's per-key covariance model, fully on-device (BR-13).

## Tests

Inline `#[cfg(test)]` module in `src/lib.rs`: cold-start near-zero across a 5×5 grid, bounded+finite output under an extreme training stream, movement toward a systematic offset target, and no drift under a zero-mean target stream. 4 tests, 100% line coverage (`cargo llvm-cov -p featherkey-neural-tap --summary-only`).
