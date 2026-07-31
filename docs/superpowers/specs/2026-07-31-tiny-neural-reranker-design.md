# Tiny Neural Re-Ranker + NN Foundation — Design (BR-69)

**Status:** Design phase. Slice 1 of the neural roadmap (foundation + first application).
**Date:** 2026-07-31
**Requirement:** **BR-69** (new; add to `BUSINESS_REQUIREMENTS.md`), traces **OBJ-1**
(typing accuracy that *improves over time*), **OBJ-3** (private by design), **OBJ-7**
(tiny footprint). Realises **smartness-roadmap Tier-3 item 7** ("tiny on-device model
for next-word/correction"), starting with the cheapest, lowest-risk application.

---

## 1. Problem

The suggestion strip is ranked by a **fixed, hand-tuned linear formula**
(`candidate-ranker::score` = `positional_score(source_rank) + LM_WEIGHT_LANG·ln(momentum)
+ source_prior`, plus an additive correction bias). The weights are constants chosen by
the author, identical for every user, and cannot capture *interactions* between signals
(e.g. "trust the bigram/context signal more when language momentum is decisive"). The user
wants the keyboard to become **smarter, context-aware, and personalised over time** — while
staying **fast**, **on-device**, **encrypted**, and **purgeable**.

This slice replaces the fixed linear ranking formula with a **tiny neural network** whose
weights are **learned online from the user's own accept/pick behaviour**, initialised so a
fresh install ranks *exactly as today* and only improves from there.

## 2. Scope

**In scope (this spec):**
- A reusable, dependency-free tiny-NN substrate crate (`featherkey-nn`).
- A learned re-ranking policy crate (`featherkey-neural-ranker`) built on it.
- Wiring, persistence (encrypted), and online training in the composition root.

**Out of scope (named roadmap follow-ups, each its own spec→plan→build cycle):**
- Application 2: neural autocorrect confidence gate.
- Application 3: neural tap decoder.
- Application 4: neural next-word language model (Tier-3 "big swing").

## 3. What already exists (CODEMAP consulted — CLAUDE.md §2)

The "smarter / context-aware / learns / encrypted / purgeable" stack is **largely already
built**; this feature *upgrades the ranking policy*, it does not rebuild learning.

| Capability | Where it lives today | This feature's relationship |
|---|---|---|
| Merge + rank candidates (linear, momentum) | `featherkey-candidate-ranker` (`rank`, `rank_with_bias`, `score`) | **Extend**: add a pure `rank_by(cands, k, scorer)` that owns dedup/order/top-k over *any* scorer; existing fns delegate to it, byte-identical. |
| Bigram next-word context | `featherkey-context` (persists under `Namespace::PersonalLm`) | **Consume** as an input feature (context score). Unchanged. |
| Correction signals (strip-pick prefs, unwanted words) | `featherkey-corrections`; core `correction_adjustment` (promote − demote) | **Consume** as two input features. Unchanged. |
| Learned vocabulary / user dict | `featherkey-personalization` | **Consume** `is_known` as an input feature. Unchanged. |
| Language momentum | `featherkey-language-momentum` (`Momentum::weight_of`) | **Consume** as an input feature. Unchanged. |
| Encryption + persistence | `featherkey-secure-store` (redb + AES-256-GCM), `SecureStore` port (`put`/`get`) | **Reuse**: weights persist under a new namespace. No port change (no `delete` needed). |
| Sensitive-field gate | `featherkey-sensitive-context` (BR-26), core observe gate | **Reuse**: training is gated identically. |
| Purge ("clear learned data") | Android `SettingsActivity.clearLearnedData()` deletes the whole `featherkey.redb` (+ legacy TSVs) | **Reuse as-is**: a whole-store wipe removes the new namespace too. No new purge code. |

**Decision (CLAUDE.md decision table):** the learned *policy* is a **different responsibility**
from `candidate-ranker`'s "pure merge/order" job, and a substrate will be shared by three
future apps → **two new crates**, depending on existing crates rather than duplicating them.
`Namespace::PersonalLm` is already taken by `context` (the contracts doc-comment saying it is
"not written by any crate" is stale — fix that comment) → the net needs its **own** namespace.

## 4. Architecture

```
featherkey-core (composition)
  ├─ loads/persists weights via SecureStore (Namespace::RankerModel)   [encrypted]
  ├─ rank_suggestions → featherkey-neural-ranker::score  (as the scorer)
  │                         └─ featherkey-nn::Mlp::forward
  │                     → featherkey-candidate-ranker::rank_by (dedup/order/top-k)
  └─ on commit (off hot path, gated) → featherkey-neural-ranker::train
                                          └─ featherkey-nn::Mlp SGD step
```

### 4.1 `featherkey-nn` (new, domain, zero deps)

The tiny substrate. Pure math, no I/O, no `SecureStore`, no Android types.

- `struct Mlp` — one hidden layer: `W1 [I×H]`, `b1 [H]`, `W2 [H×1]`, `b2 [1]`
  (`I ≈ 10` inputs, `H = 8` hidden). ~100–150 `f32` params.
- `Mlp::forward(&self, x: &[f32]) -> f32` — matvec → +bias → ReLU → matvec → +bias.
- `Mlp::train_step(&mut self, x: &[f32], grad_of_output: f32, lr: f32)` — backprop the
  supplied output-gradient through the net and apply SGD. (The *ranking loss* and its
  output-gradient are computed by `featherkey-neural-ranker`; `nn` stays task-agnostic.)
- `Mlp::from_prior(prior: &LinearPrior) -> Mlp` — deterministic init so `forward` reproduces
  a supplied linear function of the inputs (see §6).
- `Mlp::to_bytes(&self) -> Vec<u8>` / `Mlp::from_bytes(&[u8]) -> Result<Mlp, NnError>` —
  versioned blob (magic + `u16` version + `u16` input-count + `u16` hidden-count + f32
  little-endian weights). A blob with the wrong magic/version/shape → `Err(NnError::Blob)`;
  the caller falls back to the prior. **Errors are values; no panics/unwrap/expect.**

Determinism: no `Date`/RNG. Init is a pure function of the prior; there is no random weight
seeding (the prior fully determines the starting point).

### 4.2 `featherkey-neural-ranker` (new, domain)

Depends on `featherkey-nn`, `featherkey-contracts` (`Candidate`, `RankedCandidate`,
`Source`), `featherkey-language-momentum` (`Momentum`).

- `struct Features` / `fn features(cand, momentum, signals) -> [f32; I]` — build the input
  vector from signals the core already has at rank time (§5). No new hot-path work.
- `struct NeuralRanker { mlp: Mlp }`
  - `NeuralRanker::from_prior()` — the cold-start prior (reproduces `candidate-ranker::score`).
  - `NeuralRanker::score(&self, cand, momentum, signals) -> f64` — the scorer closure the core
    hands to `candidate-ranker::rank_by`.
  - `NeuralRanker::train(&mut self, shown: &[TrainCand], chosen_idx: usize, lr: f32)` —
    pairwise learning-to-rank: for each candidate ranked *above* the chosen one, take one SGD
    step nudging `score(chosen) > score(other)` (logistic pairwise loss). Bounded work
    (≤ `k` pairs). Returns nothing; mutation is in place.
  - `to_bytes`/`from_bytes` delegate to `nn` (persistence blob = the `Mlp` blob).

### 4.3 `featherkey-candidate-ranker` (extend)

- Add `pub fn rank_by(cands, k, scorer: impl Fn(&Candidate) -> f64) -> Vec<RankedCandidate>` —
  the existing dedup/best-wins/order/top-k logic, over an arbitrary scorer.
- `rank_with_bias(c, m, k, bias)` becomes `rank_by(c, k, |x| score(x, m) + bias(&x.word))`;
  `rank` unchanged. **Behaviour byte-identical** — a property test asserts
  `rank_by(c, k, |x| score(x,m)) == rank(c, m, k)`.

### 4.4 `featherkey-contracts` (extend)

- Add `Namespace::RankerModel` → `"ranker_model"`. Fix the stale `PersonalLm` doc-comment.
- Add `RankerModel` to the `Namespace` enumeration test list.

### 4.5 `featherkey-core` (wire)

- **Open:** load `RankerModel` blob → `NeuralRanker::from_bytes`, or `from_prior()` if absent
  or `Err` (corrupt/old). Held in the core's owned state alongside context/corrections.
- **Rank:** `rank_suggestions` computes the per-candidate `signals` it already computes today
  (positional/momentum/source/correction/dict-rank/context/is-known) and calls
  `candidate-ranker::rank_by(cands, k, |c| neural.score(c, &momentum, signals_for(c)))`.
  The fixed `score`+`bias` path is superseded by the neural scorer when active.
- **Train:** the core caches the last ranked candidate set (with features) per lowercased
  prefix (it already produces this set). On the matching commit/strip-pick — in the existing
  `learn_word` / `observe_strip_pick` off-hot-path location, behind the existing
  `observe gate` (`learningEnabled && !field.isSensitive()`) — it calls `neural.train(...)`
  then persists the blob via `SecureStore`. **No new FFI**; training piggybacks on existing
  observe entry points.

No new FFI surface expected this slice → **no UniFFI binding / `.so` change**. (If the plan
finds a signal the shell must pass that the core lacks, that becomes an explicit FFI sub-task;
the current signals are all core-internal.)

## 5. The feature vector (~10 inputs, all already computed at rank time)

| # | Feature | Source (already available) |
|---|---|---|
| 1 | `positional_score(source_rank)` | `candidate-ranker::positional_score` |
| 2 | `ln(momentum.weight_of(lang))` | `Momentum::weight_of` |
| 3 | source == Lexicon (0/1) | `Candidate::source` |
| 4 | source == Device (0/1) | `Candidate::source` |
| 5 | correction promote | core `correction_adjustment` (promote part) |
| 6 | correction demote | core `correction_adjustment` (demote part) |
| 7 | dict-rank / frequency (normalised) | pack `dict_rank` snapshot |
| 8 | bigram-context score | `Context::next_counts` |
| 9 | is-known / learned word (0/1) | `Personalization::is_known` |
| 10 | bias term (constant 1.0) | — |

All are bounded, cheap scalars computed once per candidate per rank (as today). No feature
requires a new scan or a vocabulary clone (BR-46).

## 6. Cold-start guarantee — no day-1 regression (load-bearing invariant)

`NeuralRanker::from_prior()` initialises the `Mlp` so `forward(features(cand)) ==
candidate-ranker::score(cand, momentum)` (within f32 tolerance) for the linear terms:
the hidden layer passes the score's linear combination through (ReLU with a bias offset that
keeps the operating range on the linear side, or a linear passthrough unit), and the output
weights equal the current constants (`LM_WEIGHT_LANG`, `SOURCE_PRIOR_LEXICON/DEVICE`, the
positional coefficient = 1, correction weights = the current sticky/unwanted weights).

**Gate test:** on a fixed candidate corpus, `NeuralRanker::from_prior()` produces the *same
top-k order* as `candidate-ranker::rank` **before any training**. This is the proof that a
fresh install (or a post-wipe reload) never regresses today's behaviour.

## 7. Learning (online, off hot path, gated)

- **Signal:** a commit/strip-pick tells us *this* candidate was the right one out of the shown
  set. Pairwise: for each candidate the model ranked above the chosen one, apply one SGD step
  minimising a logistic pairwise loss `−ln σ(score(chosen) − score(other))`. Bounded to ≤ `k`
  pairs; learning rate small and fixed.
- **When:** in the existing off-hot-path observe location (never on the input thread).
- **Gate:** `learningEnabled && !field.isSensitive()` (BR-26) — password/OTP fields never
  train. Inference is naturally a no-op there too (no suggestions are shown).
- **No hardcoded X→Y tables** (roadmap hard constraint): everything is derived from the SGD
  updates on real user signals.

## 8. Encryption & purge

- **Encrypted:** weights persist through `SecureStore::put(Namespace::RankerModel, …)` →
  AES-256-GCM, same as every other personal datum. Never written in plaintext, never leaves
  the device (BR-8/BR-13).
- **Purgeable:** `SettingsActivity.clearLearnedData()` already deletes the whole
  `featherkey.redb`, so the `RankerModel` table is wiped with everything else. On the next
  open the net re-inits to the prior (§6). **No new purge code, no `SecureStore::delete`.**
  Granular per-model reset is deferred (YAGNI).

## 9. Performance / footprint (BR-1/BR-3/BR-4/OBJ-7)

- Forward pass: `I·H + H` ≈ 88 MACs per candidate × a handful of candidates → < 10 µs; well
  inside the O(1)-per-input budget.
- Training: one forward + backward per commit, off the hot path.
- Blob: ~100–150 `f32` ≈ 600 bytes encrypted.
- **Zero new dependencies** → no `.so` growth, no `deny.toml` (license/advisory) exposure,
  no coverage cliff from vendored ML code.

## 10. Testing (TDD/BDD are entry conditions — CLAUDE.md §3)

- **`featherkey-nn`:** forward correctness (known weights → known output); finite-difference
  gradient check; `train_step` reduces loss on a toy target; `to_bytes`/`from_bytes` round-trip;
  `from_bytes` rejects bad magic / wrong version / wrong shape (`Err`, no panic); determinism.
- **`featherkey-neural-ranker`:** `features` shape/values; **cold-start reproduces
  `candidate-ranker` top-k order** (§6 gate); a repeatedly-chosen lower-ranked word's score
  rises above its rivals after N `train` calls; `train` on a single example does not explode
  a strong default (bounded lr).
- **`featherkey-candidate-ranker`:** `rank_by` == `rank`/`rank_with_bias` (delegation identity,
  property test); existing suite stays green.
- **`featherkey-core`:** commit-sequence → later `rank_suggestions` reflects learning;
  gating no-op in sensitive field / consent off (control test proving the gate bites);
  persist → reload → identical scores; **post-wipe reload → prior** (purge proof).
- **BDD `@BR-69`** in `core/features/`: *"the strip learns to rank the word I keep choosing
  higher, and forgets it when I clear my data."*
- **Definition of Done** (IMPLEMENTATION_PLAN §3.2): all tests green · coverage ≥ 98% line ·
  fitness exit 0 (≤500 lines/file, ≤60 lines/fn) · clippy `-D warnings` · `@BR-69` scenario ·
  traceability rows · CODEMAP regenerated · no panics on the hot path.

## 11. Global constraints (bind every task)

- Rust core imports **no Android/JNI types** (fitness-enforced).
- **Errors are values** — no `unwrap`/`expect`/`panic` in library code.
- ≤ **500 lines/file**, ≤ **60 lines/fn**.
- **Coverage ≥ 98% line.**
- `deny.toml`: permissive licenses only, no wildcards — **this slice adds no dependencies.**
- `Cargo.lock` committed; native `.so` never committed.
- Determinism: no `Date::now`/RNG in core logic (init is a pure function of the prior).
- `CODEMAP.md` is generated — regenerate, never hand-edit.

## 12. Alternatives rejected

- **Heavyweight ML framework** (candle/tract/onnx/torch): fails the license allowlist and/or
  the ≥98% coverage + fitness caps, bloats the `.so`, and burns battery. Ruled out by the
  repo's own gates and OBJ-7.
- **Re-ranker inside `candidate-ranker`** (one crate): blurs that crate's "pure merge/order"
  responsibility and leaves apps 2–4 without a shared substrate. Rejected in favour of two
  crates.
- **Linear/logistic-only model:** simpler, but cannot learn feature interactions and is barely
  "a neural network"; user chose the 1-hidden-layer MLP.
- **Granular per-model purge / `SecureStore::delete`:** unnecessary — the whole-store wipe
  already satisfies "everything purgeable". Deferred.
- **Cloud/federated personalisation:** conflicts with BR-13 (on-device only). Rejected (also
  rejected in the smartness roadmap).

## Audit log
_(Design gate — `/r-u-sure` — appended on each run.)_
