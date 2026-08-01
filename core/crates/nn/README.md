# featherkey-nn

**Its ONE job:** Tiny, dependency-free neural substrate — 1-hidden-layer MLP
math (forward, SGD backprop, deterministic prior init, versioned codec) for
the neural-roadmap apps. Pure math: no I/O, no clock, no RNG, no Android
types; errors are values (`NnError`), never panics.

## Layer

`domain` (per `[package.metadata.featherkey]` in `Cargo.toml`). A leaf crate:
depends on nothing else in the workspace, so it cannot introduce a cycle.

## What it does

Two sibling MLP types share the crate rather than one generalizing the
other — the split exists because their shipped callers need incompatible
output shapes and codecs, and forcing them into one type would break the
three apps that already depend on `Mlp`'s exact scalar shape/blob format:

- **`Mlp`** — single scalar output, ReLU hidden, linear output. `forward(x)
  -> f32`; `train_step(x, d_output, lr)` backprops a caller-supplied output
  gradient and applies one SGD step to every parameter in place;
  `from_linear(a, bias, scale, offset_c)` cold-starts a net whose `forward`
  reproduces an arbitrary bounded linear function exactly, while keeping
  every weight non-zero so all units stay trainable from step one; a
  versioned little-endian codec (magic `FKNN`) round-trips it through
  `to_bytes`/`from_bytes`, rejecting a wrong-magic/version/shape blob as
  `NnError::Blob` rather than mis-parsing it. Backing math for the neural
  roadmap's scalar apps (re-ranker, autocorrect gate, tap-warp).
- **`MlpMulti`** — 1-hidden-layer, ReLU hidden, `outputs` **linear output
  heads** (a multi-class/softmax classifier substrate). `forward(x) ->
  Vec<f32>` (`outputs` raw logits); `Self::softmax(logits) -> Vec<f32>` is
  numerically stable (subtracts the max logit before `exp`, falls back to a
  uniform distribution rather than `NaN`/panic if the sum is zero or
  non-finite — covers degenerate/empty input too); `train_step(x, target,
  lr) -> Result<(f32, Vec<f32>), NnError>` is one cross-entropy-loss SGD
  step against `target`, updating every parameter (`w1`/`b1`/`w2`/`b2`) in
  place and **returning `(loss, dL/dinput)`** — the gradient with respect to
  the *input* vector, of length `inputs`. `target >= outputs` returns
  `Err(NnError::Shape)`, never a panic. `reset_output_row(class)` zeroes one
  output head's row/bias back to its cold-start (all-zero) state — a no-op,
  not a panic, if `class` is out of range — for a caller (e.g. a bounded
  vocabulary) that reuses a freed class index for a new item and wants it to
  start neutral rather than inherit the previous occupant's learned weights.
  A distinct versioned codec (magic `FKNM`, vs. `Mlp`'s `FKNN`, so the two
  blob types can never be confused) round-trips inputs/hidden/outputs shape
  alongside the weights. Backing math for `featherkey-neural-lm`'s
  `NextWordLm` (a softmax classifier over a bounded vocabulary).

**Why `train_step` returns `dL/dinput` on `MlpMulti` but not on `Mlp`.**
`Mlp`'s callers feed fixed, non-trainable features, so nothing upstream
needs the input gradient. `MlpMulti`'s only caller (`featherkey-neural-lm`)
feeds a vector assembled from **trainable embedding rows** — those rows can
only learn if the net hands back the gradient with respect to its input, so
the caller can apply it upstream. This is the load-bearing part of the
`MlpMulti` contract.

## Invariants

- **Errors are values, never panics.** `forward`/`hidden_activations` on
  both types are truncation-safe (zipped-slice iteration silently truncates
  a length mismatch rather than indexing out of bounds); `train_step`
  reports an invalid `target` as `Err(NnError::Shape)` (`MlpMulti`) or is
  unconditional (`Mlp`, which has no target index to validate); a codec
  rejects a wrong-magic/version/truncated/shape-mismatched blob as
  `Err(NnError::Blob)`.
- **Deterministic.** No RNG anywhere in the crate (zero new deps); prior
  init (`Mlp::from_linear`) and cold-start callers in dependent crates are
  reproducible pure functions of their inputs.
- **`MlpMulti`'s output shape is fixed at construction** and never resizes;
  its codec round-trips exactly that shape or rejects the blob.
- **Backprop deltas are computed against pre-update weights** on both types
  (snapshotted before any parameter is mutated), matching standard
  backpropagation and keeping the two implementations' discipline
  consistent.

## Deferred

- **Negative sampling / hierarchical softmax for `MlpMulti`.** The current
  design is a full dense softmax over `outputs`. At the recommended
  `V ≈ 2000` classes that is ~64k MACs per forward pass — well under the
  1 ms latency budget — so no sparse-output approximation is built. If a
  future caller's output vocabulary needs to grow well past that footprint
  budget, revisit with negative sampling or a hierarchical softmax rather
  than paying for it now (YAGNI).

## Serves (BRs)

**BR-7** (tap-warp, via `Mlp`), **BR-11** (re-ranker via `Mlp`; next-word LM
via `MlpMulti`), **BR-12** (autocorrect gate, via `Mlp`), **BR-10/BR-11**
(next-word LM, via `MlpMulti`, in `featherkey-neural-lm`). This crate itself
is pure substrate — it is not tagged to a BR directly, but every neural
roadmap app depends on it.

## Tests

Inline `#[cfg(test)]` unit tests alongside each module (`lib.rs` for `Mlp`
forward/backprop, `codec.rs`, `train.rs`, `prior.rs`; `multi.rs` for
`MlpMulti` forward/softmax/`reset_output_row`, `multi_codec.rs`,
`multi_train.rs` for the cross-entropy step and its finite-difference
input-gradient check). Run via `cargo test -p featherkey-nn`.
