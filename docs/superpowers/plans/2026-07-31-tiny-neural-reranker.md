# Tiny Neural Re-Ranker + NN Foundation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the suggestion strip's fixed linear ranking formula with a tiny,
dependency-free neural network that learns online from the user's picks, initialised to
reproduce today's ranking exactly (no day-1 regression), persisted encrypted and purged by the
existing whole-store wipe.

**Architecture:** Two new domain crates — `featherkey-nn` (a generic, zero-dependency 1-hidden-
layer MLP: forward, backprop/SGD, linear-prior init, versioned serde) and
`featherkey-neural-ranker` (the ranking-specific policy: an 8-slot feature vector, the
cold-start prior, pairwise learning-to-rank, self-persistence under a new namespace). The
composition root (`featherkey-core`) assembles the feature vector from signals it already
computes at rank time, hands the neural scorer to a generalised `candidate-ranker::rank_by`,
and runs one SGD step per gated strip-pick off the hot path.

**Tech Stack:** Rust (workspace `core/`), pure `f32` math, no ML framework, no new
dependencies. Persistence via the existing `SecureStore` port (redb + AES-256-GCM). Design:
`docs/superpowers/specs/2026-07-31-tiny-neural-reranker-design.md`.

**Requirement:** BR-11 (prediction improves as it learns the user's habits; supplies BR-11's
first BDD scenario), supporting BR-10 and BR-9.

## Global Constraints

Every task's requirements implicitly include these (copied from the design + CLAUDE.md):

- **No new dependencies.** `featherkey-nn` has **zero** deps; `featherkey-neural-ranker` depends
  only on `featherkey-nn` + `featherkey-contracts`. Verify `cargo deny check` still passes.
- **Errors are values** — no `unwrap`/`expect`/`panic` in library code (tests may, under the
  existing `#[allow]`). Deserialisation of a bad/old blob returns `Err`, never panics.
- **File/function size:** ≤ 500 lines/file, ≤ 60 lines/fn (fitness-enforced). Split modules
  before a file grows past the cap.
- **Coverage ≥ 98% line** (`cargo llvm-cov --workspace --fail-under-lines 98`).
- **Determinism:** no `Date::now`/RNG anywhere in these crates. The prior is a pure function of
  the coefficients; there is no random weight seeding.
- **Rust core imports no Android/JNI types** (fitness-enforced).
- **`Cargo.lock` committed**; never commit a `.so`. **`CODEMAP.md` is generated** — regenerate,
  never hand-edit.
- **Layer discipline:** both new crates declare `[package.metadata.featherkey] layer = "domain"`
  and depend only on same/inner layers.
- **Feature-vector slot order is defined once**, in `featherkey-neural-ranker` (`RankFeatures`),
  and reused by both the prior and the core's assembly — never duplicated.

## Feature vector (slot order — the single contract)

`RankFeatures` → `[f32; 8]`, in this exact order (the prior's coefficient vector `a` uses the
same order):

| slot | field | prior coefficient `a[i]` | bound `|x_i| ≤` |
|---|---|---|---|
| 0 | `positional` = `-ln(1+source_rank)` | `1.0` | `ln(1+MAXRANK)` ≈ `ln(65)` ≈ 4.18 |
| 1 | `ln_momentum` = `ln(momentum.weight_of(lang))` | `LM_WEIGHT_LANG` = 1.0 | ~2.4 (weight ∈ [FLOOR,~11]) |
| 2 | `is_lexicon` (0/1) | `SOURCE_PRIOR_LEXICON` = 0.2 | 1.0 |
| 3 | `is_device` (0/1) | `SOURCE_PRIOR_DEVICE` = 0.0 | 1.0 |
| 4 | `correction_promote` (≥0) | `1.0` | ~15 |
| 5 | `correction_demote` (≥0) | `-1.0` | ~15 |
| 6 | `spatial` (raw spatial score) | `SPATIAL_WEIGHT` = 0.35 | 1.0 |
| 7 | `bias` (constant 1.0) | `0.0` | 1.0 |

**Bound `B = 20.0`** covers every slot with margin (the correction terms are the widest at
~15). The prior's linear-region constant must satisfy `C > scale·B`; the plan uses `scale = 1.0`,
`C = 500.0` — a large safety margin so every hidden unit stays in its linear region across the
whole feature domain. With `positional` already carrying its coefficient of 1, and
`correction_promote`/`demote` carrying their own weights (`CORRECTION_STICKY_WEIGHT`=1.0,
`CORRECTION_UNWANTED_WEIGHT`=0.5) inside `correction_parts`, the prior's `forward` reproduces
exactly `candidate_ranker::score + correction_adjustment + SPATIAL_WEIGHT·spatial`.

> **Note on correction weights:** slots 4/5 hold the *raw* promote/demote terms already scaled
> by their weights inside `correction_parts` (see Task 9), so `a[4]=1.0`, `a[5]=-1.0`. This keeps
> the two signals as independent, learnable inputs while the prior sums to today's formula.

---

## File Structure

- `core/crates/nn/` — **new** package `featherkey-nn`
  - `Cargo.toml`, `src/lib.rs` (`Mlp`, `forward`), `src/prior.rs` (`from_linear`),
    `src/train.rs` (`train_step`, backprop), `src/codec.rs` (`to_bytes`/`from_bytes`),
    `src/error.rs` (`NnError`).
- `core/crates/neural-ranker/` — **new** package `featherkey-neural-ranker`
  - `Cargo.toml`, `src/lib.rs` (`RankFeatures`, `INPUTS`, `NeuralRanker`, `from_prior`, `score`,
    `reinforce`), `src/persist.rs` (`persist`/`load` via `SecureStore`).
- `core/crates/contracts/src/lib.rs` — add `Namespace::RankerModel`; fix stale `PersonalLm` doc.
- `core/crates/candidate-ranker/src/lib.rs` — add `rank_by`; make `positional_score` `pub`.
- `core/crates/featherkey-core/src/lib.rs` — struct field + `new()` init + prior-coeff consts.
- `core/crates/featherkey-core/src/rank.rs` — feature assembly + neural scorer + shown-set cache.
- `core/crates/featherkey-core/src/learn.rs` — persist/restore + gated `reinforce` hook.
- `core/crates/featherkey-core/tests/` — persistence/purge + learning integration tests.
- `core/features/neural-reranker.feature` — `@BR-11` scenario.
- `core/Cargo.toml` — add both crates to `members`.

---

## Task 1: `featherkey-nn` — MLP struct + forward pass

**Files:**
- Create: `core/crates/nn/Cargo.toml`, `core/crates/nn/src/lib.rs`
- Modify: `core/Cargo.toml` (add `"crates/nn"` to members)

**Interfaces:**
- Produces: `Mlp` with fixed `INPUTS`/`HIDDEN` (generic over sizes via fields, not const
  generics — keep it simple); `Mlp::with_weights(w1, b1, w2, b2) -> Mlp`;
  `Mlp::forward(&self, x: &[f32]) -> f32`.

Manifest:
```toml
[package]
name = "featherkey-nn"
version = "0.0.0"
publish = false
edition.workspace = true
license.workspace = true
rust-version.workspace = true
description = "Tiny dependency-free neural substrate: 1-hidden-layer MLP with forward, SGD, linear-prior init, and versioned serialization."

[package.metadata.featherkey]
layer = "domain"

[lints]
workspace = true
```

- [ ] **Step 1: Write the failing test** (`src/lib.rs`, `#[cfg(test)]`)
```rust
#[test]
fn forward_computes_relu_mlp_by_hand() {
    // 2 inputs, 2 hidden, 1 output. W1 row-major [h][i].
    let mlp = Mlp::with_weights(
        vec![1.0, 0.0, 0.0, 1.0], // W1: h0=x0, h1=x1
        vec![0.0, 0.0],           // b1
        vec![2.0, -3.0],          // W2
        1.0,                      // b2
        2, 2,
    );
    // h = relu([x0, x1]) = [1, 0] for x=[1,-4]; out = 2*1 + (-3)*0 + 1 = 3
    assert!((mlp.forward(&[1.0, -4.0]) - 3.0).abs() < 1e-6);
}
```

- [ ] **Step 2: Run it, see it fail** — `cargo test -p featherkey-nn forward_computes` → FAIL
  (`Mlp` undefined).

- [ ] **Step 3: Minimal implementation**
```rust
//! Tiny dependency-free neural substrate. A 1-hidden-layer MLP with a single
//! scalar output, ReLU hidden activation, linear output. Pure math: no I/O, no
//! Android types, errors are values (see `error`/`codec`).
#[derive(Debug, Clone, PartialEq)]
pub struct Mlp {
    w1: Vec<f32>, // [hidden * inputs], row-major by hidden unit
    b1: Vec<f32>, // [hidden]
    w2: Vec<f32>, // [hidden]
    b2: f32,
    inputs: usize,
    hidden: usize,
}
impl Mlp {
    #[must_use]
    pub fn with_weights(w1: Vec<f32>, b1: Vec<f32>, w2: Vec<f32>, b2: f32,
                        inputs: usize, hidden: usize) -> Self {
        Self { w1, b1, w2, b2, inputs, hidden }
    }
    #[must_use]
    pub fn inputs(&self) -> usize { self.inputs }
    /// Forward pass: `x` (len == inputs) → scalar score.
    #[must_use]
    pub fn forward(&self, x: &[f32]) -> f32 {
        let (h, _pre) = self.hidden_activations(x);
        let mut out = self.b2;
        for j in 0..self.hidden { out += self.w2[j] * h[j]; }
        out
    }
    /// Hidden activations and pre-activations (pre reused by backprop).
    fn hidden_activations(&self, x: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let mut h = vec![0.0f32; self.hidden];
        let mut pre = vec![0.0f32; self.hidden];
        for j in 0..self.hidden {
            let mut z = self.b1[j];
            for i in 0..self.inputs { z += self.w1[j * self.inputs + i] * x[i]; }
            pre[j] = z;
            h[j] = if z > 0.0 { z } else { 0.0 };
        }
        (h, pre)
    }
}
```

- [ ] **Step 4: Run test to verify it passes** — `cargo test -p featherkey-nn forward` → PASS.

- [ ] **Step 5: Add determinism + guard tests, then commit**
```rust
#[test] fn forward_is_deterministic() {
    let m = Mlp::with_weights(vec![0.5,0.5], vec![0.1], vec![2.0], 0.0, 2, 1);
    assert_eq!(m.forward(&[1.0,1.0]), m.forward(&[1.0,1.0]));
}
```
```bash
git add core/crates/nn core/Cargo.toml && git commit -m "feat(nn): MLP struct + ReLU forward pass"
```

---

## Task 2: `featherkey-nn` — linear-prior init (`from_linear`)

The cold-start construction: reproduce an arbitrary linear function `a·x + bias` exactly, with
**hidden == inputs** and every weight non-zero (so all units are trainable from step 1). Unit
`j` handles input `j`: `h_j = ReLU(s·x_j + C)`, output weight `a_j/s`, output bias
`bias − (C/s)·Σ a_j`. For inputs bounded by `B` and `C > s·B`, every unit is in its linear
region, so `forward == a·x + bias`.

**Files:** Create `core/crates/nn/src/prior.rs`; Modify `src/lib.rs` (add `mod prior;`).

**Interfaces:**
- Produces: `Mlp::from_linear(a: &[f32], bias: f32, scale: f32, offset_c: f32) -> Mlp`
  (hidden == `a.len()`).

- [ ] **Step 1: Failing test** (`src/prior.rs`)
```rust
use super::Mlp;
#[test]
fn from_linear_reproduces_the_linear_function_including_negative_outputs() {
    let a = [1.0, -2.0, 0.5];
    let mlp = Mlp::from_linear(&a, 0.7, 1.0, 100.0); // C=100 >> B
    for x in [[0.0,0.0,0.0], [3.0,-1.0,2.0], [-4.0,5.0,-3.0]] {
        let want = a[0]*x[0] + a[1]*x[1] + a[2]*x[2] + 0.7;
        assert!((mlp.forward(&x) - want).abs() < 1e-3, "x={x:?}");
    }
}
#[test]
fn from_linear_leaves_every_output_weight_nonzero() {
    let mlp = Mlp::from_linear(&[1.0, 0.0, 0.35], 0.0, 1.0, 100.0);
    assert!(mlp.w2_iter().all(|w| w.abs() > 0.0)); // even a[i]=0 → w2=0/s=0? see note
}
```
> Implementation note: for `a[i] == 0`, `w2_j = 0` would make unit `j` untrainable. Set a
> **floor**: `w2_j = a_j/s` if `|a_j|>ε` else a small non-zero `η` (e.g. 1e-3) with a matching
> `w1` so the unit contributes ~0 at init but has gradient. Adjust the second test to assert
> `|w2_j| > 0` and the first test's tolerance already absorbs the `η` term (η·(s·x_j+C) folded
> into bias is not constant — so instead: for `a_j==0` set `w1_j=0`, `b1_j=C`, `w2_j=η`, and add
> `−η·C` to `b2`; then unit output is `η·C` constant, cancelled exactly, gradient to `w1_j`
> flows). Encode this in the implementation and keep the exactness test.

- [ ] **Step 2: Run, see it fail.**
- [ ] **Step 3: Implement `from_linear`** per the construction + the `a_j==0` handling above.
- [ ] **Step 4: Run tests → PASS.**
- [ ] **Step 5: Commit** — `feat(nn): from_linear prior reproducing a bounded linear function`.

---

## Task 3: `featherkey-nn` — backprop + SGD `train_step`

**Files:** Create `core/crates/nn/src/train.rs`; Modify `src/lib.rs` (`mod train;`, expose pre-acts).

**Interfaces:**
- Produces: `Mlp::train_step(&mut self, x: &[f32], d_output: f32, lr: f32)` — backpropagates a
  supplied gradient of the loss w.r.t. the scalar output (`d_output = ∂L/∂out`) and applies one
  SGD update to all weights.

- [ ] **Step 1: Failing tests**
```rust
#[test]
fn gradient_matches_finite_difference() {
    let base = Mlp::from_linear(&[0.3,-0.2], 0.1, 1.0, 50.0);
    let x = [1.5, -0.5];
    // dL/dout = 1 => dL/dparam = dout/dparam. Compare analytic step direction to FD.
    let eps = 1e-3;
    // Perturb b2: out increases by exactly eps => FD grad ~ 1.0.
    let mut up = base.clone(); up.nudge_b2(eps);
    let fd = (up.forward(&x) - base.forward(&x)) / eps;
    assert!((fd - 1.0).abs() < 1e-2);
}
#[test]
fn train_step_reduces_squared_error_on_a_toy_target() {
    let mut m = Mlp::from_linear(&[0.0,0.0], 0.0, 1.0, 50.0);
    let x = [1.0, 2.0]; let target = 5.0;
    let loss0 = (m.forward(&x) - target).powi(2);
    for _ in 0..200 { let d = 2.0*(m.forward(&x) - target); m.train_step(&x, d, 0.01); }
    let loss1 = (m.forward(&x) - target).powi(2);
    assert!(loss1 < loss0 * 0.01, "loss {loss0}->{loss1}");
}
```

- [ ] **Step 2: Run, see fail.**
- [ ] **Step 3: Implement** backprop (output layer grads, ReLU derivative gate, input layer
  grads) + SGD update; add `nudge_b2` test helper under `#[cfg(test)]`.
- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** — `feat(nn): backprop + SGD train_step`.

---

## Task 4: `featherkey-nn` — versioned serialize / deserialize

**Files:** Create `core/crates/nn/src/codec.rs`, `core/crates/nn/src/error.rs`; Modify `src/lib.rs`.

**Interfaces:**
- Produces: `Mlp::to_bytes(&self) -> Vec<u8>`; `Mlp::from_bytes(&[u8]) -> Result<Mlp, NnError>`;
  `enum NnError { Blob }` (`Debug, Clone, PartialEq, Eq`, `Display`).
- Blob layout: magic `b"FKNN"` + `u16` version (`1`) + `u16` inputs + `u16` hidden + f32
  little-endian weights in `[w1.., b1.., w2.., b2]` order.

- [ ] **Step 1: Failing tests**
```rust
#[test] fn bytes_round_trip() {
    let m = Mlp::from_linear(&[1.0,-2.0,0.5], 0.3, 1.0, 100.0);
    assert_eq!(Mlp::from_bytes(&m.to_bytes()).unwrap(), m);
}
#[test] fn from_bytes_rejects_bad_magic() {
    assert_eq!(Mlp::from_bytes(b"XXXX\x01\x00").unwrap_err(), NnError::Blob);
}
#[test] fn from_bytes_rejects_wrong_version() {
    let mut b = Mlp::from_linear(&[1.0], 0.0, 1.0, 50.0).to_bytes();
    b[4] = 0xFF; // corrupt version
    assert_eq!(Mlp::from_bytes(&b).unwrap_err(), NnError::Blob);
}
#[test] fn from_bytes_rejects_truncated_blob() {
    assert_eq!(Mlp::from_bytes(b"FKNN").unwrap_err(), NnError::Blob);
}
```

- [ ] **Step 2: Run, see fail.**
- [ ] **Step 3: Implement** `to_bytes`/`from_bytes` with strict length + shape checks (a blob
  whose declared shape doesn't match its byte length → `Err(Blob)`); `NnError` + `Display`.
  **No panics** — bounds-check every slice read.
- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** — `feat(nn): versioned serialize/deserialize with error-as-value guards`.

---

## Task 5: `featherkey-neural-ranker` — RankFeatures, prior, score

**Files:**
- Create: `core/crates/neural-ranker/Cargo.toml`, `core/crates/neural-ranker/src/lib.rs`
- Modify: `core/Cargo.toml` (add `"crates/neural-ranker"`).

Manifest deps: `featherkey-nn = { path = "../nn" }`, `featherkey-contracts = { path =
"../contracts" }`; dev-deps: `featherkey-candidate-ranker = { path = "../candidate-ranker" }`,
`featherkey-language-momentum = { path = "../language-momentum" }`, `proptest = "1.11.0"`.

**Interfaces:**
- Produces: `pub const INPUTS: usize = 8;`
- `pub struct RankFeatures { positional, ln_momentum, is_lexicon, is_device,
  correction_promote, correction_demote, spatial: f32 }` + `RankFeatures::to_array(&self) ->
  [f32; INPUTS]` (slot 7 = bias 1.0).
- `pub struct NeuralRanker { mlp: Mlp }`
- `NeuralRanker::from_prior(coeffs: &[f32; INPUTS]) -> Self` (builds `Mlp::from_linear`
  with hidden==INPUTS, `scale=1.0`, `offset_c=500.0`).
- `NeuralRanker::score(&self, f: &RankFeatures) -> f64`.

- [ ] **Step 1: Failing tests**
```rust
#[test]
fn from_prior_reproduces_the_linear_score() {
    let coeffs = [1.0, 1.0, 0.2, 0.0, 1.0, -1.0, 0.35, 0.0];
    let r = NeuralRanker::from_prior(&coeffs);
    let f = RankFeatures { positional: -1.1, ln_momentum: 0.4, is_lexicon: 1.0,
        is_device: 0.0, correction_promote: 0.0, correction_demote: 0.0, spatial: 0.0 };
    let want = coeffs[0]*f.positional + coeffs[1]*f.ln_momentum + coeffs[2]*1.0;
    assert!((r.score(&f) - want as f64).abs() < 1e-3);
}
#[test]
fn cold_start_order_matches_candidate_ranker() {
    // Build candidates, momentum; derive coeffs from candidate-ranker's public consts
    // (LM_WEIGHT_LANG, SOURCE_PRIOR_LEXICON/DEVICE, positional coeff = 1) with
    // correction/spatial coeffs zeroed (no such signal in this corpus). Assert the
    // neural top-k order equals candidate_ranker::rank order over the corpus.
    // (Full corpus + assertion written here; uses dev-deps candidate-ranker + momentum.)
}
```

- [ ] **Step 2: Run, see fail.**
- [ ] **Step 3: Implement** `RankFeatures`, `to_array`, `INPUTS`, `NeuralRanker::from_prior`,
  `score` (calls `mlp.forward`).
- [ ] **Step 4: Run → PASS** (both, incl. the no-regression order test).
- [ ] **Step 5: Commit** — `feat(neural-ranker): RankFeatures + cold-start prior + score`.

---

## Task 6: `featherkey-neural-ranker` — pairwise reinforce + self-persistence

**Files:** Modify `src/lib.rs`; Create `core/crates/neural-ranker/src/persist.rs`.

**Interfaces:**
- Produces:
  - `NeuralRanker::reinforce(&mut self, shown: &[RankFeatures], chosen: usize, lr: f32)` —
    for each `j != chosen`, one pairwise logistic step nudging `score(chosen) > score(j)`:
    `d = σ(score(j) − score(chosen))`; `train_step(chosen_x, −d, lr)` and
    `train_step(j_x, +d, lr)`. Bounded to `shown.len()−1` pairs. No-op if `chosen` out of range
    or `shown.len() < 2`.
  - `NeuralRanker::persist(&self, store: &impl SecureStore) -> Result<(), StoreError>`
    (`Namespace::RankerModel`, blob key `b"v1"`).
  - `NeuralRanker::load(store: &impl SecureStore, prior: &[f32; INPUTS]) ->
    Result<Self, StoreError>` — absent blob **or** an `NnError` from `from_bytes` → `from_prior`
    (never fails on a corrupt/old model; corruption falls back to the prior, not an error).

- [ ] **Step 1: Failing tests**
```rust
#[test]
fn repeatedly_choosing_a_lower_word_promotes_it() {
    let coeffs = [1.0,1.0,0.2,0.0,1.0,-1.0,0.35,0.0];
    let mut r = NeuralRanker::from_prior(&coeffs);
    let strong = RankFeatures{ positional:0.0, ..zero() };      // rank 0
    let weak   = RankFeatures{ positional:-1.4, ..zero() };     // rank ~3
    assert!(r.score(&strong) > r.score(&weak));
    for _ in 0..300 { r.reinforce(&[strong.clone(), weak.clone()], 1, 0.05); }
    assert!(r.score(&weak) > r.score(&strong), "weak should have overtaken");
}
#[test]
fn a_single_reinforce_does_not_unseat_a_strong_default() {
    let mut r = NeuralRanker::from_prior(&COEFFS);
    let strong = RankFeatures{ positional:0.0, ..zero() };
    let weak   = RankFeatures{ positional:-1.4, ..zero() };
    r.reinforce(&[strong.clone(), weak.clone()], 1, 0.05);
    assert!(r.score(&strong) > r.score(&weak));
}
#[test]
fn persist_then_load_is_identity() { /* in-memory FakeStore; round-trip scores equal */ }
#[test]
fn load_falls_back_to_prior_on_corrupt_blob() {
    // store a junk blob under RankerModel; load returns from_prior (no Err), scores == prior.
}
```

- [ ] **Step 2: Run, see fail.**
- [ ] **Step 3: Implement** `reinforce` (logistic pairwise), `persist`/`load`, a `#[cfg(test)]`
  in-memory `FakeStore` (or reuse an existing test store helper).
- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** — `feat(neural-ranker): pairwise reinforce + encrypted self-persistence`.

---

## Task 7: `contracts` — `Namespace::RankerModel`

**Files:** Modify `core/crates/contracts/src/lib.rs`.

- [ ] **Step 1: Failing test** (append to the namespace tests)
```rust
#[test] fn ranker_model_namespace_key_is_stable() {
    assert_eq!(Namespace::RankerModel.as_str(), "ranker_model");
}
```
Also extend the existing "all namespaces" enumeration test to include `RankerModel`.

- [ ] **Step 2: Run, see fail** (variant missing).
- [ ] **Step 3: Implement** — add the variant + `as_str` arm + doc-comment; **fix the stale
  `PersonalLm` doc-comment** ("Reserved… not currently written" → "next-word bigram model,
  written by `context`").
- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** — `feat(contracts): add RankerModel namespace; fix stale PersonalLm doc`.

---

## Task 8: `candidate-ranker` — `rank_by` generalisation + public `positional_score`

**Files:** Modify `core/crates/candidate-ranker/src/lib.rs`.

**Interfaces:**
- Produces: `pub fn rank_by(cands: &[Candidate], k: usize, scorer: impl Fn(&Candidate) -> f64)
  -> Vec<RankedCandidate>` (the existing dedup/best-wins/order/top-k, over an arbitrary scorer);
  `pub fn positional_score(rank: u32) -> f64`.
- `rank_with_bias` becomes `rank_by(cands, k, |c| score(c, momentum) + bias(&c.word))`; `rank`
  unchanged in behaviour.

- [ ] **Step 1: Failing test**
```rust
#[test]
fn rank_by_with_the_default_scorer_equals_rank() {
    let mom = Momentum::new("en", &["en".into(), "es".into()]);
    let cands = vec![c("hi","en",0), c("ho","es",1), c("he","en",2)];
    assert_eq!(rank(&cands, &mom, 3),
               rank_by(&cands, 3, |x| score(x, &mom)));
}
#[test] fn positional_score_is_public_and_monotone() {
    assert!(positional_score(0) > positional_score(5));
}
```

- [ ] **Step 2: Run, see fail.**
- [ ] **Step 3: Implement** `rank_by` (move the body of `rank_with_bias` into it), make
  `positional_score` `pub`, delegate `rank`/`rank_with_bias`.
- [ ] **Step 4: Run → PASS**, and the **entire existing candidate-ranker suite stays green**
  (the delegation must be byte-identical — the proptest crossover test still passes).
- [ ] **Step 5: Commit** — `refactor(candidate-ranker): extract rank_by; expose positional_score`.

---

## Task 9: core — split `correction_adjustment` into promote/demote parts

**Files:** Modify `core/crates/featherkey-core/src/rank.rs`.

**Interfaces:**
- Produces: `fn correction_parts(&self, prefix: &str, word: &str) -> (f64, f64)` returning
  `(promote, demote)` both ≥ 0 (already weighted by `CORRECTION_STICKY_WEIGHT` /
  `CORRECTION_UNWANTED_WEIGHT`). `correction_adjustment` becomes `let (p,d)=...; p - d`.

- [ ] **Step 1: Failing test** (in `rank.rs` tests)
```rust
#[test]
fn correction_parts_sum_to_the_adjustment() {
    // build a core, note 3 picks + 2 unwanted for a word; assert
    // parts.0 - parts.1 == correction_adjustment(prefix, word), both terms > 0.
}
```

- [ ] **Step 2: Run, see fail.**
- [ ] **Step 3: Implement** `correction_parts`; refactor `correction_adjustment` to delegate.
- [ ] **Step 4: Run → PASS**; existing correction-ranking tests stay green (identity preserved).
- [ ] **Step 5: Commit** — `refactor(core): expose correction promote/demote parts`.

---

## Task 10: core — hold `NeuralRanker`, wire persist/restore, prior coefficients

**Files:** Modify `core/crates/featherkey-core/src/lib.rs` (struct field, `new()`,
`PRIOR_COEFFS`), `core/crates/featherkey-core/src/learn.rs` (persist/restore),
`core/crates/featherkey-core/Cargo.toml` (add `featherkey-neural-ranker`,
`featherkey-nn` if needed).

**Interfaces:**
- Consumes: `NeuralRanker::from_prior`, `persist`, `load`; `candidate_ranker` public consts for
  the coefficient assembly.
- Produces: `const PRIOR_COEFFS: [f32; INPUTS]` in core, assembled from
  `candidate_ranker::{LM_WEIGHT_LANG, SOURCE_PRIOR_LEXICON, SOURCE_PRIOR_DEVICE}` + `1.0`
  (positional) + `1.0`/`-1.0` (correction promote/demote, pre-weighted in features) +
  `SPATIAL_WEIGHT` + `0.0` (bias). Field `neural_ranker: NeuralRanker` initialised
  `NeuralRanker::from_prior(&PRIOR_COEFFS)` in `new()`.

- [ ] **Step 1: Failing tests** (`core/crates/featherkey-core/tests/neural_persistence.rs`)
```rust
#[test]
fn persist_then_restore_preserves_the_ranker() {
    // train via a public seam (Task 12) or reinforce through observe_strip_pick,
    // persist to a temp store, restore into a fresh core, assert rank_suggestions
    // order is identical.
}
#[test]
fn restore_from_empty_store_yields_the_prior() {
    // fresh temp store, restore, assert rank_suggestions order == a prior-core's order
    // (the purge / first-run proof).
}
```

- [ ] **Step 2: Run, see fail** (field/persist missing).
- [ ] **Step 3: Implement** — add field + `PRIOR_COEFFS` + `new()` init; in `learn.rs` add
  `self.neural_ranker.persist(store)?;` to `persist()` and
  `self.neural_ranker = NeuralRanker::load(store, &PRIOR_COEFFS)?;` to `restore()`.
- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** — `feat(core): hold NeuralRanker; persist/restore under RankerModel`.

---

## Task 11: core — assemble features + swap `rank_suggestions` to the neural scorer

**Files:** Modify `core/crates/featherkey-core/src/rank.rs`.

**Interfaces:**
- Consumes: `NeuralRanker::score`, `RankFeatures`, `candidate_ranker::{rank_by,
  positional_score}`, `correction_parts`.
- Produces: `fn rank_features(&self, cand: &Candidate, prefix: &str, spatial: &[(String,i32)])
  -> RankFeatures`; a `RankSnapshot { prefix: String, shown: Vec<(String, RankFeatures)> }`
  cached on the core as `last_ranked: Option<RankSnapshot>` (bounded to one snapshot).

- [ ] **Step 1: Failing/guard test** — the **existing** `rank.rs` ordering tests
  (`rank_suggestions_orders_by_bundled_rank_when_nothing_learned`,
  `..._lets_context_beat_bundled_rank`, `..._appends_device_candidates_under_momentum`, etc.)
  must **still pass** after the swap: with the prior ranker (no training), the neural scorer
  reproduces today's order. Add one new test:
```rust
#[test]
fn rank_suggestions_matches_the_legacy_linear_order_before_training() {
    // Construct a core, call rank_suggestions, assert the order equals the order the
    // old score+bias path produced (captured as an expected list).
}
```

- [ ] **Step 2: Run** — confirm the guard test fails only where intended and existing tests are
  the safety net (they should pass once the swap is correct).
- [ ] **Step 3: Implement** — replace the `rank_with_bias(&cands, &momentum, k, |word| ...)`
  call with `rank_by(&cands, k, |c| self.neural_ranker.score(&self.rank_features(c, prefix,
  &spatial)))`; assemble `RankFeatures` (positional via `positional_score(c.source_rank)`,
  `ln_momentum` via `momentum.weight_of(&c.lang).ln()`, source flags, `correction_parts`,
  spatial lookup); cache `last_ranked`. Keep `guarantee_fold_variant` after it, unchanged.
- [ ] **Step 4: Run → PASS** (new guard + all existing rank tests).
- [ ] **Step 5: Commit** — `feat(core): rank via the neural scorer; cache the shown set`.

---

## Task 12: core — gated online training on strip-pick

**Files:** Modify `core/crates/featherkey-core/src/learn.rs`; test
`core/crates/featherkey-core/tests/neural_learning.rs`.

**Interfaces:**
- Produces: `fn reinforce_from_pick(&mut self, prefix: &str, chosen: &str)` (private) — if
  `last_ranked` matches `prefix.to_lowercase()` and `chosen` is in the shown set, call
  `self.neural_ranker.reinforce(shown_features, chosen_idx, RANKER_LR)`; else no-op.
  `const RANKER_LR: f32 = 0.05`. Called from the **already-gated** `observe_strip_pick` (after
  `note_pick`) and from `learn_word` when the committed word is in the current snapshot.

- [ ] **Step 1: Failing tests**
```rust
#[test]
fn strip_picks_teach_the_ranker_to_promote_the_chosen_word() {
    // rank_suggestions(prefix "te") shows [test, team, tea]; repeatedly
    // observe_strip_pick("te","tea", ordinary_field) + re-rank; after N rounds
    // "tea" ranks ahead of "test".
}
#[test]
fn training_is_suppressed_in_a_sensitive_field() {
    // same sequence with a sensitive field: ranking order is unchanged (gate bites).
}
#[test]
fn training_respects_consent_off() { /* learningEnabled=false path: no change */ }
```

- [ ] **Step 2: Run, see fail.**
- [ ] **Step 3: Implement** `reinforce_from_pick`; call it inside `observe_strip_pick` (past the
  existing `should_suppress` guard) and `learn_word`. **No new FFI** — the shell's existing
  `observe_strip_pick`/`learn_word` calls now also train.
- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** — `feat(core): train the neural ranker from gated strip-picks`.

---

## Task 13: BDD `@BR-11` scenario, traceability, CODEMAP, full gate

**Files:** Create `core/features/neural-reranker.feature`; regenerate `CODEMAP.md`.

- [ ] **Step 1: Write the `@BR-11` scenario**
```gherkin
Feature: The suggestion strip learns which word I mean
  @BR-11
  Scenario: repeatedly choosing a lower-ranked completion promotes it, and clearing data forgets it
    Given the strip offers "test", "team" and "tea" for the prefix "te"
    When I choose "tea" from the strip several times in an ordinary field
    Then "tea" is ranked ahead of "test" for "te"
    When I clear my learned data
    Then "test" is ranked ahead of "tea" for "te" again
```
Map the steps to the core (`rank_suggestions` + `observe_strip_pick` + restore-from-empty).

- [ ] **Step 2: Run `python3 core/tools/bdd_check.py`** — `@BR-11` now traces to BR-11.
- [ ] **Step 3: Regenerate the index** — `python3 core/tools/codemap.py`.
- [ ] **Step 4: Run the full gate** — `bash core/tools/ci-local.sh` — all gates PASS
  (fmt, clippy `-D warnings`, tests, fitness, bdd, codemap `--check`, bindings `--check`
  (unchanged — no FFI), coverage ≥ 98%, cargo-deny **no new deps**).
- [ ] **Step 5: Commit** — `test(core): @BR-11 neural re-ranker scenario; regenerate CODEMAP`.

---

## Definition of Done (whole feature — IMPLEMENTATION_PLAN §3.2)

All tests green · coverage ≥ 98% line · fitness exit 0 (≤500 lines/file, ≤60 lines/fn) ·
clippy `-D warnings` clean · public API matches the design · `@BR-11` scenario present and
traceable · CODEMAP regenerated · **no new dependencies** (`cargo deny check` clean) · no
panics on the hot path · cold-start reproduces today's ranking (Task 5/11) · encrypted +
purged-by-wipe verified (Task 10) · training gated in sensitive/consent-off (Task 12).

## Rollback

Each task is a single commit. The feature is inert until Task 11 swaps the scorer; reverting
Tasks 11–12 restores the exact legacy linear ranking (the prior *is* that formula, so even
Task 11 alone is behaviour-neutral before any training). The two new crates are additive —
reverting Tasks 1–8 removes them with no effect on the rest of the workspace.

## Audit log
_(Plan gate — `/r-u-sure` — appended on each run.)_
