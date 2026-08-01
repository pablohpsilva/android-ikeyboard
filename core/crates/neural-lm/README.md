# featherkey-neural-lm

**Its ONE job:** Own the bounded per-user `Vocab` (word ↔ index map) that a tiny on-device embedding next-word LM trains and predicts over.

## Layer

`domain` (per `[package.metadata.featherkey]` in `Cargo.toml`). Pure logic;
no I/O, no clock, no RNG, no global state of its own.

## Status

This slice (SP1) has `Vocab` — a deterministic, capacity-bounded word↔index
map with reserved `UNK`/`BOS` indices and least-frequent eviction — and
`NextWordLm`'s cold-start init plus next-word inference (`score_next`,
`rank_next`, `confidence`). A fresh model is uniform and asserts nothing:
only the output layer (`w2`/`b2`) is zero-initialised; the embedding table
and `w1`/`b1` are non-zero and deterministic so the model stays trainable
(see the cold-start doc comment on `model.rs`). Online training (`observe`)
and encrypted persistence are deferred to later tasks in the
neural-lm-foundation plan.

## Ports

Depends on `featherkey-nn` (the tiny MLP substrate the LM will be built on),
`featherkey-contracts` (for the `SecureStore` port `NextWordLm` persistence
will use), and `featherkey-context` (reuses its `is_learnable` predicate so
token-learnability rules have exactly one definition).

## Serves (BRs)

**BR-11**-adjacent: neural roadmap app #4 (embedding next-word LM), sub-project 1.
