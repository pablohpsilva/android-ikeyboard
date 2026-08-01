# featherkey-neural-lm

**Its ONE job:** Own the bounded per-user `Vocab` (word ↔ index map) that a tiny on-device embedding next-word LM trains and predicts over.

## Layer

`domain` (per `[package.metadata.featherkey]` in `Cargo.toml`). Pure logic;
no I/O, no clock, no RNG, no global state of its own.

## Status

This slice (SP1) has `Vocab` — a deterministic, capacity-bounded word↔index
map with reserved `UNK`/`BOS` indices and least-frequent eviction — and
`NextWordLm`'s cold-start init, next-word inference (`score_next`,
`rank_next`, `confidence`), and online training (`observe`, in `learn.rs`).
A fresh model is uniform and asserts nothing: only the output layer
(`w2`/`b2`) is zero-initialised; the embedding table and `w1`/`b1` are
non-zero and deterministic so the model stays trainable (see the cold-start
doc comment on `model.rs`). `observe` trains the network (cross-entropy SGD
via `featherkey_nn::MlpMulti::train_step`) and the `K` context embedding
rows together, one step each per call — this is what lets a novel context
generalise from what a similar one taught the model, not just memorise exact
bigrams (see `learn/tests.rs`'s contamination guard). A full-vocab eviction
reuses the freed index for the new word (`Vocab::intern` reports which index
was freed); before that index is trained on, `observe` resets its embedding
row (back to the same deterministic cold-start value `new()` would have
given it) and, via `featherkey_nn::MlpMulti::reset_output_row`, its output
row (back to zero) — so the new word starts from the same neutral state a
genuinely fresh index would have, never wearing the evicted word's learned
vector (see `learn/tests.rs::eviction_resets_the_reused_indexs_learned_state`).
Encrypted persistence is deferred to a later task in the
neural-lm-foundation plan.

## Ports

Depends on `featherkey-nn` (the tiny MLP substrate the LM will be built on),
`featherkey-contracts` (for the `SecureStore` port and the
`Namespace::PersonalLm` key `b"lm_v1"` `NextWordLm::persist`/`load` use —
distinct from `featherkey-context`'s `b"v1"` key in the same namespace), and
`featherkey-context` (reuses its `is_learnable` predicate so
token-learnability rules have exactly one definition).

## Deferred

- **Per-query (softmax-margin) confidence.** `confidence` today is a single
  warm-up-count signal (§7 of the design doc): the number of `observe` steps
  whose context's last word is in-vocab, squashed through a saturating
  curve. A per-query sharpness signal (e.g. the max-softmax margin at
  inference time) would be a second, complementary confidence source, but is
  not built now — one honest confidence source until a second is shown to
  help (KISS).
- **Dynamic/lazy-grown net shape.** `MlpMulti`'s output layer is fixed at
  construction to the full vocabulary ceiling (`O = 2 + N`), so the model's
  footprint is a bounded constant from the moment it is created, not a
  figure it grows into. A lazily-grown, dynamically-resized net would shrink
  the early (near-empty-vocabulary) footprint further, but adds
  reallocation and a variable codec — recorded as a future optimization, not
  built now.
- **SP2 wiring.** This crate is host-testable in isolation and is not yet
  called from anywhere in the running keyboard. Sub-project 2 (a separate,
  later slice) wires it up: blending `NextWordLm`'s scores into
  `StatisticalPredictor::suggest_ranked` alongside the bigram (confidence-
  gated so the bigram leads cold), the online-`observe` call site in
  `learn.rs`, and gating that call under BR-22 (consent) / BR-26 (sensitive-
  context suppression) exactly where `context.record` is gated today.

## Serves (BRs)

**BR-10/BR-11**-adjacent: neural roadmap app #4 (embedding next-word LM),
sub-project 1. SP1 delivers the enabling model in isolation; SP2 closes
BR-10/BR-11 against the live suggestion strip.
