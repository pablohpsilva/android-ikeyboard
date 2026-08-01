# Neural next-word LM — Sub-project 1 (LM foundation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan
> task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a tiny, dependency-free, on-device embedding next-word LM as a
host-testable crate in isolation — a multi-output MLP substrate, a bounded
per-user vocabulary, and the LM itself (cold-start, online learning, encrypted
persistence) — **not yet wired** into the live suggestion strip.

**Architecture:** New `featherkey-nn::MlpMulti` (softmax classifier, additive —
`Mlp` untouched) + new domain crate `featherkey-neural-lm` (`Vocab` + `NextWordLm`
= embeddings over `MlpMulti`). Composes with the existing `featherkey-context`
bigram at SP2; SP1 delivers the model only.

**Tech Stack:** Rust (host-testable core, no Android/JNI), `featherkey-nn`
substrate, `featherkey-contracts` `SecureStore` port, encrypted redb via
`Namespace::PersonalLm`. Zero new third-party dependencies.

**Design:** `docs/superpowers/specs/2026-08-01-neural-lm-foundation-design.md`
(read §§3–9, 13 before starting; this plan implements it verbatim).

## Global Constraints

Every task's requirements implicitly include all of these (copied verbatim from
the design + CLAUDE.md; exact values are load-bearing):

- **Errors are values.** No `unwrap`/`expect`/`panic`/array-index-panic in library
  code (tests may use `unwrap`/`expect` under the existing `#[cfg(test)] #[allow(...)]`).
- **Zero new dependencies.** No `rand`, no crate additions. Determinism comes from
  index-seeded init, not `rand`.
- **No FFI, no Kotlin, no Android types in SP1.** Nothing in the running keyboard
  calls this code yet. The Rust core never imports Android/JNI (fitness-enforced).
- **File/function size:** ≤ 500 lines/file, ≤ 60 lines/function
  (`fitness/check.py`). Split tests into a sibling `tests.rs` / `*/tests.rs` when a
  file nears the cap (as `neural-tap` did).
- **TDD/BDD order:** the `@BR-10`/`@BR-11` Gherkin (Task 1) and each task's failing
  unit tests are written and **seen to fail** before implementation.
- **Coverage ≥ 98% line**, `fitness/check.py` exit 0, `bdd_check.py` green,
  `codemap.py --check` clean. Full gate: `bash core/tools/ci-local.sh`.
- **Cold-start init split (design §7), load-bearing:** zero **output** layer
  (`w2`,`b2`); **non-zero deterministic** `w1`/`b1` and embedding rows. Never zero
  the whole net (dead-ReLU trap).
- **Fixed net shape (design §4):** `MlpMulti` `O = 2 + N` set at construction;
  reserved indices `0`=`<unk>`, `1`=`<bos>` are never training targets and never
  emitted.
- **Persistence:** encrypted, `Namespace::PersonalLm`, key `b"lm_v1"` (distinct
  from the bigram's `b"v1"`); `load` returns a cold-start model on
  absent/corrupt/wrong-shape, never `Err` on user-data state.
- **Recommended dims (pin in Task 6/7, adjustable):** `k=2`, `D=16`, `H=32`,
  `N=2000`, `LM_LR=0.05`, `WARMUP_HALF=50`.
- **No AI attribution** in any commit/comment.

---

## File Structure

- `core/features/neural_lm.feature` — **create** (Task 1)
- `core/crates/nn/src/multi.rs` — **create**: `MlpMulti` struct, `forward`, `softmax` (Task 2)
- `core/crates/nn/src/multi_train.rs` — **create**: `MlpMulti::train_step` (Task 3)
- `core/crates/nn/src/multi_codec.rs` — **create**: `MlpMulti` `to_bytes`/`from_bytes` (Task 4)
- `core/crates/nn/src/error.rs` — **modify**: add `NnError::Shape` (Task 3)
- `core/crates/nn/src/lib.rs` — **modify**: `mod multi/multi_train/multi_codec;`, re-export `MlpMulti` (Tasks 2–4)
- `core/crates/nn/README.md` — **modify**: document `MlpMulti` + Deferred (Task 10)
- `core/crates/context/src/lib.rs` — **modify**: promote `is_learnable`/`is_storable`/`MIN_TOKEN_CHARS` to `pub` (Task 5)
- `core/crates/neural-lm/` — **create** crate (Cargo.toml, README.md, src/) (Task 6)
- `core/crates/neural-lm/src/vocab.rs` — **create**: `Vocab` (Task 6)
- `core/crates/neural-lm/src/lib.rs` — **create**: `NextWordLm` + cold-start + inference (Task 7)
- `core/crates/neural-lm/src/learn.rs` — **create**: `observe` + embedding update (Task 8)
- `core/crates/neural-lm/src/persist.rs` — **create**: encrypted persist/load (Task 9)
- `core/crates/neural-lm/src/tests.rs` (+ `src/*/tests.rs` as needed) — **create** (Tasks 6–9)
- `core/Cargo.toml` — **modify**: add `crates/neural-lm` member (Task 6)
- `SOFTWARE_ENGINEERING.md` — **modify**: ADR-3 amendment + BR-10/BR-11 traceability (Task 10)

---

## Task 1: BDD scenarios (behaviour first)

**Files:**
- Create: `core/features/neural_lm.feature`

**Interfaces:**
- Produces: the `@BR-10`/`@BR-11` scenarios the later tasks' unit tests realise.

- [ ] **Step 1: Write the feature file** (design §12, four scenarios)

```gherkin
@BR-11
Feature: On-device neural next-word language model (foundation)
  A tiny per-user embedding LM learns which word follows a short context,
  generalising across similar contexts, cold-starting harmlessly, and
  surviving persistence. (Sub-project 1: the model in isolation, not yet
  wired to the live suggestion strip.)

  @BR-11
  Scenario: Learns a two-word context the bigram cannot
    Given the model has repeatedly seen "going to work" and "walking to school"
    When I have typed "going to"
    Then it ranks "work" above "school"
    And after "walking to" it ranks "school" above "work"

  @BR-11
  Scenario: Generalises across similar contexts via embeddings
    Given the model has learned "the cat", "a cat" and "the dog"
    When I type "a"
    Then "dog" is surfaced as a candidate after "a"

  @BR-10
  Scenario: A cold model asserts nothing
    Given a fresh model that has learned nothing
    When I ask for the next words after any context
    Then its confidence is zero
    And its ranking is the deterministic uniform tie-order

  @BR-11
  Scenario: Learning survives persistence
    Given a trained model persisted and reloaded through a secure store
    Then its rankings and confidence are unchanged
    And an absent or corrupt stored blob reloads as a cold-start model
```

- [ ] **Step 2: Verify traceability tooling sees the tags**

Run: `cd core && python3 tools/bdd_check.py`
Expected: no *new* failure attributable to `neural_lm.feature` beyond BR rows
Task 10 will close (if `bdd_check` requires the traceability row first, note the
failure and let Task 10 resolve it — do not fake a row here).

- [ ] **Step 3: Commit**

```bash
git add core/features/neural_lm.feature
git commit -m "test(neural-lm): @BR-10/@BR-11 next-word LM foundation scenarios"
```

**Definition of Done:** feature file present, four scenarios tagged, committed.
**Rollback:** delete the feature file.

---

## Task 2: `featherkey-nn::MlpMulti` — forward + stable softmax

**Files:**
- Create: `core/crates/nn/src/multi.rs`
- Modify: `core/crates/nn/src/lib.rs` (add `mod multi;`, `pub use multi::MlpMulti;`)

**Interfaces:**
- Consumes: nothing new (leaf crate).
- Produces: `MlpMulti::with_weights(w1,b1,w2,b2,inputs,hidden,outputs)`,
  `inputs()/hidden()/outputs()`, `forward(&[f32]) -> Vec<f32>` (len `outputs`),
  `MlpMulti::softmax(&[f32]) -> Vec<f32>`. `w2` is `[outputs*hidden]` row-major by
  output; `b2` is `[outputs]`.

- [ ] **Step 1: Write failing tests** in `multi.rs` `#[cfg(test)]`

```rust
#[test]
fn forward_computes_multi_output_by_hand() {
    // 2 inputs, 2 hidden, 2 outputs. W1 row-major [h][i]; W2 row-major [o][h].
    let m = MlpMulti::with_weights(
        vec![1.0, 0.0, 0.0, 1.0], // W1: h0=x0, h1=x1
        vec![0.0, 0.0],           // b1
        vec![1.0, 0.0, 0.0, 2.0], // W2: o0=h0, o1=2*h1
        vec![0.5, -1.0],          // b2
        2, 2, 2,
    );
    // x=[3,-4] -> h=relu([3,-4])=[3,0] -> out=[1*3+0.5, 2*0-1.0]=[3.5,-1.0]
    let o = m.forward(&[3.0, -4.0]);
    assert!((o[0] - 3.5).abs() < 1e-6 && (o[1] + 1.0).abs() < 1e-6);
}

#[test]
fn softmax_sums_to_one_and_is_stable_on_large_logits() {
    let p = MlpMulti::softmax(&[1000.0, 1000.0, 1000.0]);
    let sum: f32 = p.iter().sum();
    assert!((sum - 1.0).abs() < 1e-5);
    assert!(p.iter().all(|x| x.is_finite()));
    assert!((p[0] - 1.0 / 3.0).abs() < 1e-5);
}

#[test]
fn softmax_degenerate_input_falls_back_to_uniform() {
    // Empty / zero-length logits: no panic, no NaN (covers the fallback branch).
    assert!(MlpMulti::softmax(&[]).is_empty());
    let p = MlpMulti::softmax(&[f32::NEG_INFINITY, f32::NEG_INFINITY]);
    assert!(p.iter().all(|x| x.is_finite()) && (p.iter().sum::<f32>() - 1.0).abs() < 1e-5);
}

#[test]
fn forward_is_truncation_safe_on_short_input() {
    let m = MlpMulti::with_weights(vec![1.0, 1.0], vec![0.0], vec![1.0], vec![0.0], 2, 1, 1);
    // Too-short input must not panic (mirrors Mlp::forward).
    let _ = m.forward(&[1.0]);
}
```

- [ ] **Step 2: Run — see them fail** (`MlpMulti` undefined)

Run: `cd core && cargo test -p featherkey-nn multi`
Expected: FAIL (unresolved `MlpMulti`).

- [ ] **Step 3: Implement `MlpMulti` forward + softmax**

Derive `#[derive(Clone, PartialEq, Debug)]` on `MlpMulti` (Task 3's
finite-difference test clones it; Task 4's codec test compares it). Fields
`w1,b1,w2,b2: Vec<f32>`, `inputs,hidden,outputs: usize`. Reuse the
`hidden_activations` pattern (ReLU, zip-truncation, no panic). `forward`: for each
output `o`, `b2[o] + Σ_j w2[o*hidden+j]·h[j]`. `softmax`: subtract `max` before
`exp`, divide by the sum; if the sum is `0`/non-finite, return a uniform vector
(no `NaN`, no panic).

- [ ] **Step 4: Run — green**

Run: `cd core && cargo test -p featherkey-nn multi`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core/crates/nn/src/multi.rs core/crates/nn/src/lib.rs
git commit -m "feat(nn): MlpMulti forward + numerically-stable softmax"
```

**DoD:** tests green; `Mlp` untouched; no panic paths; file < 500 lines.
**Rollback:** remove `mod multi;`/`pub use` and delete `multi.rs`.

---

## Task 3: `MlpMulti::train_step` — cross-entropy + input gradient

**Files:**
- Create: `core/crates/nn/src/multi_train.rs`
- Modify: `core/crates/nn/src/error.rs` (add `NnError::Shape`), `lib.rs` (`mod multi_train;`)

**Interfaces:**
- Produces: `MlpMulti::train_step(&mut self, x: &[f32], target: usize, lr: f32) -> Result<(f32, Vec<f32>), NnError>`
  returning `(cross_entropy_loss, dL/dinput)` (input grad length `inputs`); updates
  `w1/b1/w2/b2` in place. `target >= outputs` → `Err(NnError::Shape)`.

- [ ] **Step 1: Write failing tests** in `multi_train.rs`

```rust
#[test]
fn repeated_steps_drive_argmax_to_target() {
    let mut m = MlpMulti::with_weights(
        vec![0.2, -0.1, 0.05, 0.3], vec![0.1, -0.2],
        vec![0.0; 6], vec![0.0; 3], 2, 2, 3,
    );
    let x = [0.5, -0.3];
    for _ in 0..500 { let _ = m.train_step(&x, 2, 0.1).unwrap(); }
    let o = m.forward(&x);
    let argmax = (0..3).max_by(|a, b| o[*a].total_cmp(&o[*b])).unwrap();
    assert_eq!(argmax, 2);
}

#[test]
fn input_gradient_matches_finite_difference() {
    let mut m = MlpMulti::with_weights(
        vec![0.3, -0.2, 0.1, 0.4], vec![0.05, -0.1],
        vec![0.2, 0.1, -0.3, 0.15, 0.0, 0.25], vec![0.0, 0.0, 0.0], 2, 2, 3,
    );
    let x = [0.4, -0.6];
    let (_loss, grad) = m.clone().train_step(&x, 1, 0.0).unwrap(); // lr=0 -> no mutation
    let eps = 1e-3_f32;
    for i in 0..2 {
        let mut xp = x; xp[i] += eps;
        let mut xm = x; xm[i] -= eps;
        let num = (ce_loss(&m, &xp, 1) - ce_loss(&m, &xm, 1)) / (2.0 * eps);
        assert!((grad[i] - num).abs() < 1e-2, "grad[{i}]={} num={num}", grad[i]);
    }
}

#[test]
fn target_out_of_range_is_error_not_panic() {
    let mut m = MlpMulti::with_weights(vec![1.0], vec![0.0], vec![0.0, 0.0], vec![0.0, 0.0], 1, 1, 2);
    assert_eq!(m.train_step(&[1.0], 2, 0.1).unwrap_err(), NnError::Shape);
}
```
(`ce_loss` is a test helper: `-ln(softmax(forward(x))[target])`.)

- [ ] **Step 2: Run — see them fail**

Run: `cd core && cargo test -p featherkey-nn multi_train`
Expected: FAIL (no `train_step`, no `NnError::Shape`).

- [ ] **Step 3: Implement.** Add `NnError::Shape` (+ its `Display` arm + the enum
  test). `train_step`: guard `target >= outputs` → `Err(Shape)`; `(h,pre) =
  hidden_activations(x)`; `p = softmax(forward-from-h)`; `loss = -ln(p[target])`
  (clamp the arg of `ln` away from 0); `dlogit = p; dlogit[target] -= 1.0`; output
  layer `∂L/∂w2[o*H+j] = dlogit[o]·h[j]`, `∂L/∂b2[o]=dlogit[o]`; hidden `δ_j =
  (Σ_o dlogit[o]·w2[o*H+j])·relu'(pre_j)`; input grad `dInput[i] = Σ_j δ_j·w1[j*I+i]`
  (computed from **pre-update** weights); then SGD-update all params. Return
  `(loss, dInput)`.

- [ ] **Step 4: Run — green**

Run: `cd core && cargo test -p featherkey-nn`
Expected: PASS (incl. the untouched `Mlp` suite).

- [ ] **Step 5: Commit**

```bash
git add core/crates/nn/src/multi_train.rs core/crates/nn/src/error.rs core/crates/nn/src/lib.rs
git commit -m "feat(nn): MlpMulti cross-entropy train_step returning input gradient"
```

**DoD:** finite-difference gradient test green; argmax converges; `target` guarded;
no panic; functions ≤ 60 lines (split helpers if needed).
**Rollback:** delete `multi_train.rs`, revert the `error.rs`/`lib.rs` edits.

---

## Task 4: `MlpMulti` versioned codec

**Files:**
- Create: `core/crates/nn/src/multi_codec.rs`
- Modify: `core/crates/nn/src/lib.rs` (`mod multi_codec;`)

**Interfaces:**
- Produces: `MlpMulti::to_bytes(&self) -> Vec<u8>`, `MlpMulti::from_bytes(&[u8]) -> Result<Self, NnError>`.
  Distinct magic `FKNM` (vs `Mlp`'s `FKNN`) so the two blob types can never be
  confused; header = magic(4) + version(2) + inputs(2) + hidden(2) + outputs(2),
  then `f32`s in `[w1, b1, w2, b2]` order, little-endian.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn codec_round_trips() {
    let m = MlpMulti::with_weights(vec![0.1,0.2,0.3,0.4], vec![0.5,0.6],
        vec![0.7,0.8,0.9,1.0,1.1,1.2], vec![0.1,0.2,0.3], 2, 2, 3);
    assert_eq!(MlpMulti::from_bytes(&m.to_bytes()).unwrap(), m);
}
#[test]
fn codec_rejects_bad_magic_wrong_version_and_shape() {
    let m = MlpMulti::with_weights(vec![1.0], vec![0.0], vec![0.0,0.0], vec![0.0,0.0], 1,1,2);
    let mut bad = m.to_bytes(); bad[0] = b'X';
    assert_eq!(MlpMulti::from_bytes(&bad).unwrap_err(), NnError::Blob);
    let mut ver = m.to_bytes(); ver[4] = 0xFF;
    assert_eq!(MlpMulti::from_bytes(&ver).unwrap_err(), NnError::Blob);
    assert_eq!(MlpMulti::from_bytes(b"FKNM\x01\x00").unwrap_err(), NnError::Blob); // truncated
}
```

- [ ] **Step 2: Run — see them fail.** `cd core && cargo test -p featherkey-nn multi_codec` → FAIL.

- [ ] **Step 3: Implement** mirroring `Mlp`'s codec (`shape(inputs,hidden,outputs)`
  via `checked_mul`/`checked_add`, returning `Err(Blob)` on overflow or a declared
  shape whose implied length ≠ the byte count; `derive(PartialEq)` on `MlpMulti`
  for the round-trip assert).

- [ ] **Step 4: Run — green.** `cd core && cargo test -p featherkey-nn` → PASS.

- [ ] **Step 5: Commit**

```bash
git add core/crates/nn/src/multi_codec.rs core/crates/nn/src/lib.rs
git commit -m "feat(nn): MlpMulti versioned codec (FKNM) with shape validation"
```

**DoD:** round-trip + corruption rejection green; distinct magic; panic-free.
**Rollback:** delete `multi_codec.rs`, revert `lib.rs`.

---

## Task 5: `featherkey-context` — promote the learnable-token predicate (DRY prep)

**Files:**
- Modify: `core/crates/context/src/lib.rs`

**Interfaces:**
- Produces: `pub fn featherkey_context::is_learnable(&str) -> bool`,
  `pub fn is_storable(&str) -> bool`, `pub const MIN_TOKEN_CHARS: usize`.
- Consumed by: `neural-lm::Vocab` (Task 6) — the single home for "a token ≥ 2
  chars, free of the codec separators `\n`/`\t`, is worth learning." No second copy.

- [ ] **Step 1: Write a failing test** in `context` asserting the now-public API

```rust
#[test]
fn learnable_predicate_is_public_and_matches_record_rules() {
    assert!(featherkey_context::is_learnable("cat"));
    assert!(!featherkey_context::is_learnable("a"));        // < MIN_TOKEN_CHARS
    assert!(!featherkey_context::is_learnable("bad\ttok")); // separator
    assert_eq!(featherkey_context::MIN_TOKEN_CHARS, 2);
}
```

- [ ] **Step 2: Run — see it fail.** `cd core && cargo test -p featherkey-context learnable_predicate` → FAIL (private).

- [ ] **Step 3: Implement:** change `const MIN_TOKEN_CHARS`, `fn is_storable`,
  `fn is_learnable` to `pub`; add `///` docs. No behaviour change — `record`/`import`
  keep calling them. (Their existing tests must stay green.)

- [ ] **Step 4: Run — green.** `cd core && cargo test -p featherkey-context` → PASS (all prior tests + the new one).

- [ ] **Step 5: Commit**

```bash
git add core/crates/context/src/lib.rs
git commit -m "refactor(context): expose is_learnable/is_storable for reuse by neural-lm"
```

**DoD:** predicate public + documented; all existing `context` tests green.
**Rollback:** revert the visibility change.

---

## Task 6: `featherkey-neural-lm` crate + `Vocab`

**Files:**
- Create: `core/crates/neural-lm/Cargo.toml`, `core/crates/neural-lm/README.md`,
  `core/crates/neural-lm/src/lib.rs` (stub), `core/crates/neural-lm/src/vocab.rs`
- Modify: `core/Cargo.toml` (add `"crates/neural-lm"` to `members`)

**Interfaces:**
- Consumes: `featherkey_context::{is_learnable}`.
- Produces: `Vocab` with `new()`, `intern(&str) -> usize`, `index_of(&str) -> usize`
  (`0`=`<unk>` if absent), `word_of(usize) -> Option<&str>`, `len() -> usize`,
  reserved `UNK=0`,`BOS=1`, ceiling `MAX_VOCAB`. Eviction reports the freed index so
  the model can reset that row (Task 7 wiring); in SP1 `Vocab` owns only the
  string↔index map + frequencies.

- [ ] **Step 1: Scaffold the crate** (Cargo.toml mirrors `neural-tap`'s: domain
  layer, deps `featherkey-nn`, `featherkey-contracts`, `featherkey-context`),
  `src/lib.rs` with `mod vocab; pub use vocab::Vocab;` and crate `#![deny(...)]`
  lints matching the workspace. Add the workspace member.

- [ ] **Step 2: Write failing `Vocab` tests** (`vocab.rs` `#[cfg(test)]`)

```rust
#[test]
fn intern_assigns_stable_indices_and_is_idempotent() {
    let mut v = Vocab::new();
    let a = v.intern("cat");
    assert_eq!(a, v.intern("cat"));       // idempotent
    assert!(a >= 2);                       // past reserved
    assert_ne!(a, v.intern("dog"));
}
#[test]
fn oov_maps_to_unk_and_bos_pads() {
    let v = Vocab::new();
    assert_eq!(v.index_of("never-seen"), 0); // UNK
}
#[test]
fn sub_two_char_and_separator_tokens_are_never_interned() {
    let mut v = Vocab::new();
    assert_eq!(v.intern("a"), 0);           // too short -> UNK, not registered
    assert_eq!(v.intern("bad\ttok"), 0);    // separator -> UNK
}
#[test]
fn eviction_removes_least_frequent_deterministically() {
    let mut v = Vocab::with_capacity_for_test(2); // ceiling = 2 learned
    let rare = v.intern("aaa");
    let common = v.intern("bbb"); v.intern("bbb"); // bump freq
    let evicted = v.intern("ccc");                 // must evict least-frequent "aaa"
    assert_eq!(v.index_of("aaa"), 0);              // gone -> UNK
    assert_eq!(evicted, rare);                     // reused the freed index
    assert!(v.index_of("bbb") >= 2 && v.index_of("ccc") >= 2);
    let _ = common;
}
```

- [ ] **Step 3: Run — see them fail.** `cd core && cargo test -p featherkey-neural-lm vocab` → FAIL.

- [ ] **Step 4: Implement `Vocab`** — `BTreeMap<String, (usize, u32)>` (word →
  (index, freq)) + a reverse map or index→word `BTreeMap<usize,String>`; reject via
  `featherkey_context::is_learnable` (→ return `UNK`, do not register); eviction:
  pick min-frequency (tie → smallest index) learned entry, remove it, reuse its
  index. `with_capacity_for_test` is a **`#[cfg(test)]`-only** constructor setting
  the ceiling for the eviction test (never public API); the real ceiling is
  `MAX_VOCAB` (=2000). All deterministic (`BTreeMap`).

- [ ] **Step 5: Run — green.** `cd core && cargo test -p featherkey-neural-lm` → PASS.

- [ ] **Step 6: Commit**

```bash
git add core/crates/neural-lm core/Cargo.toml
git commit -m "feat(neural-lm): crate scaffold + bounded per-user Vocab with eviction"
```

**DoD:** crate builds in the workspace; `Vocab` tests green; reuses `context`'s
predicate (no second copy); deterministic; no panic.
**Rollback:** remove the member line and delete `core/crates/neural-lm/`.

---

## Task 7: `NextWordLm` — cold-start + inference

**Files:**
- Modify: `core/crates/neural-lm/src/lib.rs` (add `NextWordLm`)
- Create (if lib.rs nears the cap): `core/crates/neural-lm/src/model.rs`, `src/tests.rs`

**Interfaces:**
- Consumes: `Vocab`, `featherkey_nn::MlpMulti`.
- Produces: `NextWordLm::new()` (cold-start), `score_next(&[&str], &str) -> f32`
  (log-prob), `rank_next(&[&str], usize) -> Vec<(String, f32)>` (best-first, skips
  reserved), `confidence() -> f32` in `[0,1]`. Dims: `K=2, D=16, H=32, N=MAX_VOCAB`;
  `MlpMulti` shape `I=K*D, H, O=2+N`.

- [ ] **Step 1: Write failing tests** (realises the `@BR-10` cold scenario)

```rust
#[test]
fn fresh_model_has_zero_confidence_and_uniform_ranking() {
    let lm = NextWordLm::new();
    assert_eq!(lm.confidence(), 0.0);
    // Uniform logits -> deterministic tie order; no reserved token emitted.
    let ranked = lm.rank_next(&["anything"], 5);
    assert!(ranked.iter().all(|(w, _)| w != "<unk>" && w != "<bos>"));
}
#[test]
fn fresh_score_is_finite_and_uniform_across_contexts() {
    // With a zero output layer every context yields the same uniform log-prob —
    // the model asserts nothing (the *escape* from uniform, which needs live
    // w1/b1, is the Task 8 dead-ReLU guard, not this test).
    let lm = NextWordLm::new();
    let a = lm.score_next(&["go"], "work");
    let b = lm.score_next(&["swim"], "work");
    assert!(a.is_finite() && (a - b).abs() < 1e-6);
}
```

- [ ] **Step 2: Run — see them fail.** `cd core && cargo test -p featherkey-neural-lm` → FAIL.

- [ ] **Step 3: Implement cold-start + inference.** `embed: Vec<f32>` length
  `(2+N)*D` initialised by a **deterministic** `f(index, dim)` (e.g. a fixed hash of
  `index*D+dim` mapped into a small range — no `rand`); `MlpMulti::with_weights`
  where `w1/b1` are deterministic non-zero and `w2/b2` are **all zeros** (design §7);
  `confidence` from a `warmup: u32` counter via `n/(n+WARMUP_HALF)` (starts 0). Context
  assembly: last `K` of the `&[&str]`, `index_of` each, left-pad with `BOS`, gather
  embedding rows → `I`-vector. `score_next` = `ln(softmax(forward)[idx])` (clamp);
  `rank_next` = softmax over live learned classes, skip `UNK`/`BOS` and empty
  indices, sort by score DESC then word ASC, take `limit`.

- [ ] **Step 4: Run — green.** PASS.

- [ ] **Step 5: Commit**

```bash
git add core/crates/neural-lm/src
git commit -m "feat(neural-lm): NextWordLm cold-start init + next-word inference"
```

**DoD:** fresh confidence 0; forward finite/uniform; reserved never emitted;
**only** the output layer zero-initialised; files < 500 lines.
**Rollback:** revert to the Task 6 crate stub.

---

## Task 8: `NextWordLm::observe` — online training + embedding update

**Files:**
- Create: `core/crates/neural-lm/src/learn.rs` (+ `src/learn/tests.rs` if needed)
- Modify: `src/lib.rs` (`mod learn;`)

**Interfaces:**
- Produces: `NextWordLm::observe(&mut self, context: &[&str], next_word: &str)` —
  interns `next_word` (→ target class), assembles input, `net.train_step`, applies
  the returned `dInput` to the `K` embedding rows that formed the input, and bumps
  `warmup`. Evicted indices get their embedding row re-initialised
  (deterministic) and their `MlpMulti` output row zeroed.

- [ ] **Step 1: Write failing tests** (the three learning scenarios + guards)

```rust
#[test] // @BR-11 scenario 1
fn learns_two_word_context_the_bigram_cannot() {
    let mut lm = NextWordLm::new();
    for _ in 0..300 {
        lm.observe(&["going", "to"], "work");
        lm.observe(&["walking", "to"], "school");
    }
    let after_going = top(&lm.rank_next(&["going", "to"], 5));
    let after_walking = top(&lm.rank_next(&["walking", "to"], 5));
    assert_eq!(after_going, "work");
    assert_eq!(after_walking, "school");
}

#[test] // @BR-11 escape-from-uniform (dead-ReLU guard)
fn training_escapes_uniform() {
    let mut lm = NextWordLm::new();
    for _ in 0..300 { lm.observe(&["hello"], "there"); }
    assert_eq!(top(&lm.rank_next(&["hello"], 3)), "there");
    assert!(lm.confidence() > 0.0);
}

#[test] // @BR-11 scenario 2, with contamination guard
fn generalises_across_similar_contexts_via_embeddings() {
    let train = |lm: &mut NextWordLm| {
        for _ in 0..300 {
            lm.observe(&["the"], "cat");
            lm.observe(&["a"], "cat");
            lm.observe(&["the"], "dog");
        }
    };
    // "a dog" was never typed; the shared behaviour of "the"/"a" (learned into
    // their embeddings) must pull "dog" up after "a".
    let mut lm = NextWordLm::new();
    train(&mut lm);
    let learned = lm.score_next(&["a"], "dog");

    // Contamination guard (app #3 lesson: assert a MARGIN, not a binary — w1/b1
    // still train in the twin, so "dog" could otherwise sneak in and the test
    // would never go RED). The frozen twin's `observe` skips the embedding
    // update, so the embedding is the *only* remaining path to generalisation.
    let mut frozen = NextWordLm::new_frozen_embeddings_for_test();
    train(&mut frozen);
    let without = frozen.score_next(&["a"], "dog");

    // Embedding learning must lift "dog" after "a" by a clear margin over the
    // frozen twin — this fails (goes RED) if the embedding update is dropped.
    assert!(learned > without + 0.5, "learned={learned} without={without}");
}
```
(`top`/helpers live in the test module. `new_frozen_embeddings_for_test` is a
`#[cfg(test)]`-only constructor building an LM whose `observe` skips the embedding
update — it must NOT be public API. The margin `0.5` is a starting value; the
implementer tunes it so the test is RED without the embedding update and GREEN with
it — the point is a real margin, not the exact number.)

- [ ] **Step 2: Run — see them fail.** FAIL (no `observe`).

- [ ] **Step 3: Implement `observe`** per design §6: intern target; assemble input;
  `(loss, dinput) = net.train_step(input, target, LM_LR)?` (on `Err(Shape)` — target
  out of range after a race with eviction — skip the step rather than panic);
  update each context word's embedding row: `row[w_{t-j}] -= LM_LR *
  dinput[j*D .. (j+1)*D]`; `warmup += 1`. Wire eviction's row-reset (embedding
  re-init + zero the output row for the freed index).

- [ ] **Step 4: Run — green.** `cd core && cargo test -p featherkey-neural-lm` → PASS.

- [ ] **Step 5: Commit**

```bash
git add core/crates/neural-lm/src
git commit -m "feat(neural-lm): online cross-entropy observe + embedding update"
```

**DoD:** all three learning scenarios green **including** the contamination guard
(the frozen twin must fail to generalise — proving the embedding is load-bearing);
escape-from-uniform green; no panic on eviction/target races.
**Rollback:** delete `learn.rs`, revert `lib.rs`.

---

## Task 9: `NextWordLm` — encrypted persist / load

**Files:**
- Create: `core/crates/neural-lm/src/persist.rs` (+ tests)
- Modify: `src/lib.rs` (`mod persist;`)

**Interfaces:**
- Consumes: `featherkey_contracts::{Namespace, SecureStore, StoreError}`.
- Produces: `NextWordLm::persist(&self, &impl SecureStore) -> Result<(), StoreError>`,
  `NextWordLm::load(&impl SecureStore) -> Result<Self, StoreError>`. One blob under
  `Namespace::PersonalLm`, key `b"lm_v1"`; layout = version header + `Vocab` codec
  (`\n`/`\t`-delimited, mirroring `context`) + `embed` `f32`s + `MlpMulti::to_bytes`.

- [ ] **Step 1: Write failing tests** (mirror `neural-tap`'s `MemStore` pattern)

```rust
#[test] // @BR-11 persistence
fn trained_model_survives_persist_then_load() {
    let store = MemStore::default();
    let mut lm = NextWordLm::new();
    for _ in 0..200 { lm.observe(&["going", "to"], "work"); }
    lm.persist(&store).unwrap();
    let loaded = NextWordLm::load(&store).unwrap();
    assert_eq!(top(&loaded.rank_next(&["going", "to"], 5)), "work");
    assert_eq!(loaded.confidence(), lm.confidence());
}
#[test]
fn absent_or_corrupt_blob_loads_cold_start() {
    let store = MemStore::default();
    assert_eq!(NextWordLm::load(&store).unwrap().confidence(), 0.0); // absent
    store.put(Namespace::PersonalLm, b"lm_v1", &[0xff]).unwrap();
    assert_eq!(NextWordLm::load(&store).unwrap().confidence(), 0.0); // corrupt -> cold
}
```

- [ ] **Step 2: Run — see them fail.** FAIL.

- [ ] **Step 3: Implement** persist/load mirroring `neural-tap/src/persist.rs`:
  `persist` = one `put`; `load` = `get` → `None`/decode-error ⇒ `NextWordLm::new()`
  (never `Err` on user-data state; only the store's own `StoreError` propagates).
  Vocab codec reuses `context`'s separator discipline so
  `featherkey_context::is_storable` guards it.

- [ ] **Step 4: Run — green.** `cd core && cargo test -p featherkey-neural-lm` → PASS.

- [ ] **Step 5: Commit**

```bash
git add core/crates/neural-lm/src
git commit -m "feat(neural-lm): encrypted persist/load under PersonalLm (lm_v1)"
```

**DoD:** round-trip preserves rankings + confidence; absent/corrupt/wrong-shape →
cold-start; key `b"lm_v1"`; no panic.
**Rollback:** delete `persist.rs`, revert `lib.rs`.

---

## Task 10: Docs, ADR-3 amendment, traceability, full gate sweep

**Files:**
- Modify: `core/crates/nn/README.md`, `core/crates/neural-lm/README.md` (create),
  `SOFTWARE_ENGINEERING.md` (ADR-3 amendment + BR-10/BR-11 rows)
- Regenerate: `CODEMAP.md`

**Interfaces:** none (docs + gate).

- [ ] **Step 1: Crate docs.** `nn/README.md`: document `MlpMulti` and the Deferred
  items (negative sampling if `V` grows). `neural-lm/README.md`: one job, ports,
  the Deferred list (per-query confidence, dynamic-shape lazy footprint, SP2
  wiring), and the "generalises across contexts" behaviour.
- [ ] **Step 2: ADR-3 amendment** (design §10 / O-4). In `SOFTWARE_ENGINEERING.md`,
  record that the v1.x neural LM is delivered via the dependency-free
  `featherkey-nn` micro-net path (apps #1–4), **superseding** the `neural-runtime`
  mechanism; note it answers open question Q3 (footprint ≈ 0.4 MB, §8). Update the
  BR-10 and BR-11 traceability rows to name `featherkey-neural-lm`.
- [ ] **Step 3: Regenerate CODEMAP.** `python3 core/tools/codemap.py`
- [ ] **Step 4: Full gate.** `bash core/tools/ci-local.sh` — expect ALL GATES
  PASSED (tests green, coverage ≥ 98%, fitness exit 0, bdd_check green, codemap
  `--check` clean). Paste the summary into the commit body / audit log.
- [ ] **Step 5: Commit**

```bash
git add core/crates/nn/README.md core/crates/neural-lm/README.md SOFTWARE_ENGINEERING.md CODEMAP.md
git commit -m "docs(neural-lm): READMEs, ADR-3 amendment, BR-10/BR-11 traceability"
```

**DoD:** `ci-local.sh` ALL GATES PASSED with output captured; CODEMAP regenerated
(not hand-edited); ADR-3 amendment + traceability rows present.
**Rollback:** revert the doc edits; regenerate CODEMAP.

---

## Self-review (author checklist, run once)

1. **Spec coverage:** design §4 → Tasks 2–4; §5 (Vocab) → Tasks 5–6; §6 (model +
   observe) → Tasks 7–8; §7 (cold-start/confidence) → Task 7 + escape test Task 8;
   §9 (persistence) → Task 9; §10 (ADR-3) → Task 10; §12 BDD → Task 1; §13 tests →
   distributed. All covered.
2. **Placeholder scan:** every code step carries real code or an exact algorithm;
   no "TBD"/"add error handling"/"similar to".
3. **Type consistency:** `MlpMulti` signatures identical across Tasks 2–4/7/8;
   `train_step -> Result<(f32, Vec<f32>), NnError>` used consistently; `Vocab`
   `UNK=0`/`BOS=1` and `index_of` semantics consistent Tasks 6–9; persist key
   `b"lm_v1"` matches design §9.

## Audit log

### Pass 1 — ✅ Complete and verified (plan phase)
Audited the plan against the design and, crucially, against whether the plan's
literal test code compiles and goes RED for the right reason (the app #3 lesson).
Gaps found + fixed:
- **P1 (compile bug):** Task 3's finite-difference test clones `MlpMulti`, but no
  task derived `Clone`. Fixed: Task 2 now derives `#[derive(Clone, PartialEq,
  Debug)]`.
- **P2 (test that could never go RED — the app #3 defect):** Task 8's
  generalisation contamination guard used a binary present/absent assertion, but
  `w1/b1` still train in the frozen twin so "dog" could sneak in regardless. Fixed
  to a **comparative margin** assertion (`learned > without + 0.5`) that is RED
  without the embedding update, with an explicit note that the margin is tuned to
  stay honest.
- **P3 (misnamed test):** Task 7's `cold_start_..._hidden_is_live` asserted only
  finiteness, not hidden liveness. Renamed/refocused to
  `fresh_score_is_finite_and_uniform_across_contexts`; the real dead-ReLU guard is
  Task 8's escape test.
- **P4 (coverage + API hygiene):** marked `with_capacity_for_test` /
  `new_frozen_embeddings_for_test` `#[cfg(test)]`-only; added a
  `softmax_degenerate_input_falls_back_to_uniform` test so the zero-sum fallback
  branch is covered (≥ 98%).

Verified against source/architecture:
- **Layer legality:** `neural-lm → context` is domain→domain, permitted by ADR-12
  (same pattern as `prediction → context`). `nn` is a leaf → no cycle.
- **Type consistency:** `MlpMulti::train_step -> Result<(f32, Vec<f32>), NnError>`,
  `forward -> Vec<f32>` (truncation-safe), `Vocab` `UNK=0`/`BOS=1`, persist key
  `b"lm_v1"` — all consistent across Tasks 2–9 and with design §§4/6/9.
- **Substrate reuse:** codec/`NnError`/`hidden_activations` patterns mirror the
  existing `Mlp` (checked against `nn/src/{codec,error,train}.rs`); persist mirrors
  `neural-tap/src/persist.rs`; predicate reuse via the Task 5 promotion (no second
  copy).
- **Coverage of the design's must-haves:** cold-start split (Task 7), input-grad
  contract + finite-diff (Task 3), escape-from-uniform (Task 8), fixed `O=2+N`
  shape (Tasks 2/4/7), encrypted absent/corrupt→cold-start (Task 9), ADR-3
  amendment (Task 10). All mapped.

Evidence limit (honest): no code exists yet, so `cargo test`/coverage/fitness are
**not** run here — they are Task 10's build gate. This pass verifies the plan's
completeness, faithfulness to the design, and that its own test code is sound.
