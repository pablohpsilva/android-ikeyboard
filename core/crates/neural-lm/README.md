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
bigrams (see `learn/tests.rs`'s contamination guard). Encrypted persistence
is deferred to a later task in the neural-lm-foundation plan.

**Deferred:** a full-vocab eviction reuses the freed index for the new word
(`Vocab::intern`) but does not yet reset that index's embedding row or
`MlpMulti` output row back to a fresh cold-start value, so a newly-evicted-in
word could start life with residual weights trained for the word it
replaced. Not exercised by any current test (the 2000-word ceiling is far
above what the SP1 test suite trains), and not a correctness hazard today —
the row just retrains from wherever it was left, same as any other SGD step
— but worth closing before eviction is expected to happen in practice.

## Ports

Depends on `featherkey-nn` (the tiny MLP substrate the LM will be built on),
`featherkey-contracts` (for the `SecureStore` port `NextWordLm` persistence
will use), and `featherkey-context` (reuses its `is_learnable` predicate so
token-learnability rules have exactly one definition).

## Serves (BRs)

**BR-11**-adjacent: neural roadmap app #4 (embedding next-word LM), sub-project 1.
