# Personalized Autocorrect Gate — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a tiny per-user neural gate that learns when to trust an autocorrect — suppressing corrections the user reverts and applying ones they reach — while the BR-12 no-clobber guarantee stays absolute.

**Architecture:** A new `featherkey-autocorrect-gate` domain crate on the `featherkey-nn` substrate holds a bounded MLP residual over structural features. `featherkey-autocorrect` surfaces the winning correction's existing confidence score (`assess()`). Core applies `applied = (winner_confidence + residual) ≥ T` after the no-clobber veto, caches the deciding features + any withheld correction, and trains the gate from three gated signals (revert / applied-and-kept / suppressed-then-reached). Encrypted under a new `AutocorrectGate` namespace, purged by the whole-store wipe.

**Tech Stack:** Rust (workspace `core/`), `featherkey-nn` MLP, redb `SecureStore`, UniFFI, Kotlin shell.

**Spec:** `docs/superpowers/specs/2026-07-31-autocorrect-gate-design.md`

## Global Constraints

- Errors are values: no `unwrap`/`expect`/`panic` in library code (clippy `-D warnings`).
- **Zero new dependencies** (cargo-deny gate); reuse `featherkey-nn`.
- ≤ 500 lines/file, ≤ 60 lines/fn (`core/tools/fitness/check.py`).
- Coverage ≥ 98% line (ci-local).
- TDD/BDD first: a failing test before implementation; a `@BR-12`-tagged scenario per closed requirement.
- No-clobber (BR-12) absolute: the gate is consulted **only** for corrections `assess()` reports as available; a vetoed/no-candidate token never reaches the gate.
- Learning gated by `learningEnabled && !field.isSensitive()` (BR-22/BR-26).
- `Cargo.lock` committed; never commit `.so`.
- Regenerate `CODEMAP.md` via `python3 core/tools/codemap.py`; never hand-edit.
- Verify each task with `cargo test -p <crate>`; final gate `bash core/tools/ci-local.sh`.

## Feature vector (slot order — the single contract)

`GateFeatures.to_array()` → `[f32; 5]`, in this order:

| slot | field | meaning |
|------|-------|---------|
| 0 | `edit_distance` | edits from typed token to the winner (f32) |
| 1 | `winner_confidence` | the winner's `score_with_sticky` score (f64→f32) |
| 2 | `dict_rank_norm` | winner's bundled rank, normalized `1/(1+rank)` (0 if unknown) |
| 3 | `typed_len_norm` | typed token length / 16, capped at 1.0 |
| 4 | `momentum_weight` | `ln(momentum.weight_of(winner_lang))` |

## File Structure

- **Create** `core/crates/autocorrect-gate/` — `Cargo.toml`, `README.md`, `src/lib.rs` (`GateFeatures`, `INPUTS`, `RESIDUAL_BOUND`, `AutocorrectGate`), `src/persist.rs`, `tests/` as needed.
- **Modify** `core/crates/contracts/src/lib.rs` — add `Namespace::AutocorrectGate`.
- **Modify** `core/crates/autocorrect/src/lib.rs`, `src/rank.rs` — add `assess()` + `CorrectionAssessment`.
- **Modify** `core/crates/featherkey-core/src/{lib.rs,correct.rs,learn.rs,ffi.rs}` — hold the gate, apply it, train it, persist it, expose it.
- **Modify** `core/crates/featherkey-core/Cargo.toml` — depend on `featherkey-autocorrect-gate`.
- **Create** `core/features/autocorrect-gate.feature` — BDD `@BR-12`.
- **Modify** `apps/android/ime-service/src/main/kotlin/com/featherkey/ime/{FeatherKeyImeService.kt,CorrectionDetector.kt}` — wire the three signals.

---

## Task 1: `contracts` — `Namespace::AutocorrectGate`

**Files:**
- Modify: `core/crates/contracts/src/lib.rs` (enum near line 27; `as_str` near line 48; the all-variants test near line 230)

**Interfaces:**
- Produces: `Namespace::AutocorrectGate` with `as_str() == "autocorrect_gate"`.

- [ ] **Step 1: Write the failing test.** In the contracts test module, add:

```rust
#[test]
fn autocorrect_gate_namespace_has_a_stable_key() {
    assert_eq!(Namespace::AutocorrectGate.as_str(), "autocorrect_gate");
}
```

- [ ] **Step 2: Run it, verify it fails.** `cargo test -p featherkey-contracts autocorrect_gate_namespace` → FAIL (no variant `AutocorrectGate`).

- [ ] **Step 3: Add the variant + arm.** Add `AutocorrectGate,` to the enum, `Namespace::AutocorrectGate => "autocorrect_gate",` to `as_str`, and add `Namespace::AutocorrectGate,` to the `all` array in the existing all-namespaces uniqueness test.

- [ ] **Step 4: Run tests.** `cargo test -p featherkey-contracts` → PASS.

- [ ] **Step 5: Commit.**

```bash
git add core/crates/contracts/src/lib.rs
git commit -m "feat(contracts): add Namespace::AutocorrectGate"
```

---

## Task 2: `featherkey-autocorrect-gate` — crate + `GateFeatures`

**Files:**
- Create: `core/crates/autocorrect-gate/Cargo.toml`, `core/crates/autocorrect-gate/README.md`, `core/crates/autocorrect-gate/src/lib.rs`
- Modify: `core/Cargo.toml` (workspace `members`)

**Interfaces:**
- Produces: `pub const INPUTS: usize = 5;`, `pub struct GateFeatures { pub edit_distance: f32, pub winner_confidence: f32, pub dict_rank_norm: f32, pub typed_len_norm: f32, pub momentum_weight: f32 }`, `impl GateFeatures { pub fn to_array(&self) -> [f32; INPUTS] }`.

- [ ] **Step 1: Cargo.toml.** Create `core/crates/autocorrect-gate/Cargo.toml`:

```toml
[package]
name = "featherkey-autocorrect-gate"
version = "0.0.0"
edition = "2021"
description = "Tiny per-user neural gate deciding whether to trust an autocorrect (bounded MLP residual over structural features)."

[package.metadata.featherkey]
layer = "domain"

[dependencies]
featherkey-nn = { path = "../nn" }
featherkey-contracts = { path = "../contracts" }
```

Add `"crates/autocorrect-gate",` to the `members` list in `core/Cargo.toml`.

- [ ] **Step 2: Write the failing test.** In `src/lib.rs` test module:

```rust
#[test]
fn features_serialize_in_slot_order() {
    let f = GateFeatures {
        edit_distance: 1.0,
        winner_confidence: 0.5,
        dict_rank_norm: 0.25,
        typed_len_norm: 0.375,
        momentum_weight: 0.0,
    };
    assert_eq!(f.to_array(), [1.0, 0.5, 0.25, 0.375, 0.0]);
}
```

- [ ] **Step 3: Run it, verify it fails.** `cargo test -p featherkey-autocorrect-gate` → FAIL (unresolved `GateFeatures`).

- [ ] **Step 4: Implement.** In `src/lib.rs`:

```rust
//! Tiny per-user neural gate: decide whether to trust an autocorrect.
pub const INPUTS: usize = 5;

/// Structural features of one correction decision (slot order = the contract).
#[derive(Debug, Clone, Copy)]
pub struct GateFeatures {
    pub edit_distance: f32,
    pub winner_confidence: f32,
    pub dict_rank_norm: f32,
    pub typed_len_norm: f32,
    pub momentum_weight: f32,
}

impl GateFeatures {
    #[must_use]
    pub fn to_array(&self) -> [f32; INPUTS] {
        [
            self.edit_distance,
            self.winner_confidence,
            self.dict_rank_norm,
            self.typed_len_norm,
            self.momentum_weight,
        ]
    }
}
```

- [ ] **Step 5: Run, regenerate CODEMAP, commit.** `cargo test -p featherkey-autocorrect-gate` → PASS; `python3 core/tools/codemap.py`.

```bash
git add core/crates/autocorrect-gate core/Cargo.toml core/Cargo.lock CODEMAP.md
git commit -m "feat(autocorrect-gate): crate + GateFeatures slot contract"
```

---

## Task 3: `AutocorrectGate::from_prior` + `residual` (cold-start ≈ 0, bounded)

**Files:**
- Modify: `core/crates/autocorrect-gate/src/lib.rs`

**Interfaces:**
- Consumes: `featherkey_nn::Mlp` (`from_linear(a, bias, scale, offset_c)`, `forward(x)`).
- Produces: `pub const RESIDUAL_BOUND: f64 = 1.5;`, `pub struct AutocorrectGate`, `AutocorrectGate::from_prior() -> Self`, `fn residual(&self, f: &GateFeatures) -> f64` (clamped to ±`RESIDUAL_BOUND`).

- [ ] **Step 1: Write the failing tests.**

```rust
#[test]
fn cold_start_residual_is_negligible() {
    let g = AutocorrectGate::from_prior();
    let f = GateFeatures { edit_distance: 1.0, winner_confidence: 0.5,
        dict_rank_norm: 0.2, typed_len_norm: 0.3, momentum_weight: 0.0 };
    assert!(g.residual(&f).abs() < 1e-3, "cold-start residual must be ~0");
}

#[test]
fn residual_is_bounded() {
    // Even a hand-built extreme model cannot exceed the clamp.
    let g = AutocorrectGate::from_prior();
    let f = GateFeatures { edit_distance: 1e6, winner_confidence: 1e6,
        dict_rank_norm: 1e6, typed_len_norm: 1e6, momentum_weight: 1e6 };
    assert!(g.residual(&f).abs() <= RESIDUAL_BOUND + 1e-9);
}
```

- [ ] **Step 2: Run, verify fail.** `cargo test -p featherkey-autocorrect-gate residual` → FAIL.

- [ ] **Step 3: Implement.** Read `core/crates/nn/src/prior.rs` to confirm `from_linear` zero-coefficient handling (`DEAD_UNIT_WEIGHT` keeps units trainable). Then:

```rust
use featherkey_nn::Mlp;

/// The residual is clamped to this magnitude so the gate can only nudge the
/// apply threshold, never overturn a no-clobber veto (which is applied first).
pub const RESIDUAL_BOUND: f64 = 1.5;

/// Offset for the from_linear prior (kept small: a large offset causes f32
/// catastrophic cancellation near ties — see the re-ranker design).
const PRIOR_OFFSET_C: f32 = 8.0;

pub struct AutocorrectGate {
    nn: Mlp,
}

impl AutocorrectGate {
    /// Cold start: a ~0 residual (autocorrect behaves as base+floor), with the
    /// dead-unit weights `from_linear` supplies so training still flows step 1.
    #[must_use]
    pub fn from_prior() -> Self {
        let zero = [0.0_f32; INPUTS];
        Self { nn: Mlp::from_linear(&zero, 0.0, 1.0, PRIOR_OFFSET_C) }
    }

    /// The learned nudge on the apply threshold, clamped to ±[`RESIDUAL_BOUND`].
    #[must_use]
    pub fn residual(&self, f: &GateFeatures) -> f64 {
        f64::from(self.nn.forward(&f.to_array())).clamp(-RESIDUAL_BOUND, RESIDUAL_BOUND)
    }
}
```

- [ ] **Step 4: Run.** `cargo test -p featherkey-autocorrect-gate` → PASS. If `cold_start_residual_is_negligible` fails, adjust `PRIOR_OFFSET_C` per `nn/src/prior.rs` semantics until forward(zero-coeff) ≈ 0; document the chosen value.

- [ ] **Step 5: Commit.**

```bash
git add core/crates/autocorrect-gate/src/lib.rs
git commit -m "feat(autocorrect-gate): from_prior + bounded residual"
```

---

## Task 4: `AutocorrectGate::reinforce` (pointwise training toward a target)

**Files:**
- Modify: `core/crates/autocorrect-gate/src/lib.rs`

**Interfaces:**
- Consumes: `Mlp::train_step(x, d_output, lr)`, `Mlp::forward`.
- Produces: `pub fn reinforce(&mut self, f: &GateFeatures, target: f32, lr: f32)`; `pub const GATE_LR: f32 = 0.05;`.

- [ ] **Step 1: Write the failing tests.** Training toward a higher target must raise the residual for those features; toward a lower target must lower it.

```rust
#[test]
fn reinforce_moves_the_residual_toward_the_target() {
    let f = GateFeatures { edit_distance: 2.0, winner_confidence: 0.1,
        dict_rank_norm: 0.05, typed_len_norm: 0.25, momentum_weight: 0.0 };
    let mut up = AutocorrectGate::from_prior();
    let before = up.residual(&f);
    for _ in 0..200 { up.reinforce(&f, 1.0, GATE_LR); }
    assert!(up.residual(&f) > before + 0.1, "target +1 must raise residual");

    let mut down = AutocorrectGate::from_prior();
    for _ in 0..200 { down.reinforce(&f, -1.0, GATE_LR); }
    assert!(down.residual(&f) < -0.1, "target -1 must lower residual");
}
```

- [ ] **Step 2: Run, verify fail.** → FAIL (no `reinforce`).

- [ ] **Step 3: Implement.**

```rust
/// Default learning rate for one correction outcome.
pub const GATE_LR: f32 = 0.05;

impl AutocorrectGate {
    /// One SGD step of squared-error regression toward `target` (the desired
    /// residual for these features): reverts train toward a negative target,
    /// kept/reached toward positive. `d_output = 2 * (forward - target)`.
    pub fn reinforce(&mut self, f: &GateFeatures, target: f32, lr: f32) {
        let x = f.to_array();
        let d = 2.0 * (self.nn.forward(&x) - target);
        self.nn.train_step(&x, d, lr);
    }
}
```

- [ ] **Step 4: Run.** `cargo test -p featherkey-autocorrect-gate` → PASS.

- [ ] **Step 5: Commit.**

```bash
git add core/crates/autocorrect-gate/src/lib.rs
git commit -m "feat(autocorrect-gate): pointwise reinforce toward a target"
```

---

## Task 5: `AutocorrectGate` persist / load (encrypted, prior on absent/corrupt)

**Files:**
- Create: `core/crates/autocorrect-gate/src/persist.rs`
- Modify: `core/crates/autocorrect-gate/src/lib.rs` (`mod persist;`, expose `Mlp` serde use)

**Interfaces:**
- Consumes: `featherkey_contracts::{Namespace, SecureStore, StoreError}`; `featherkey_nn` serialize/deserialize (see `neural-ranker/src/persist.rs`).
- Produces: `pub fn persist(&self, store: &impl SecureStore) -> Result<(), StoreError>`, `pub fn load(store: &impl SecureStore) -> Result<Self, StoreError>`.

- [ ] **Step 1: Read the template.** Read `core/crates/neural-ranker/src/persist.rs` in full — mirror its structure (key = `Namespace::AutocorrectGate.as_str()`, serialize the `Mlp`, and on a missing **or** deserialization-failing blob return `from_prior()` rather than `Err`).

- [ ] **Step 2: Write the failing tests.**

```rust
#[test]
fn round_trips_through_the_store() {
    let store = InMemoryStore::default();
    let mut g = AutocorrectGate::from_prior();
    let f = probe();
    for _ in 0..50 { g.reinforce(&f, 1.0, GATE_LR); }
    g.persist(&store).expect("persist");
    let back = AutocorrectGate::load(&store).expect("load");
    assert!((back.residual(&f) - g.residual(&f)).abs() < 1e-6);
}

#[test]
fn absent_or_corrupt_blob_falls_back_to_prior() {
    let store = InMemoryStore::default();
    let g = AutocorrectGate::load(&store).expect("absent -> prior");
    assert!(g.residual(&probe()).abs() < 1e-3);
    store.put(Namespace::AutocorrectGate.as_str(), b"garbage").unwrap();
    let g2 = AutocorrectGate::load(&store).expect("corrupt -> prior, never Err");
    assert!(g2.residual(&probe()).abs() < 1e-3);
}
```

(Reuse the `InMemoryStore` test double pattern from `neural-ranker`'s tests; add a `probe()` helper returning a fixed `GateFeatures`.)

- [ ] **Step 3: Run, verify fail.** → FAIL.

- [ ] **Step 4: Implement** `src/persist.rs` mirroring `neural-ranker/src/persist.rs`, keyed by `Namespace::AutocorrectGate`, `load` returning `from_prior()` on absent/corrupt.

- [ ] **Step 5: Run, CODEMAP, commit.** `cargo test -p featherkey-autocorrect-gate` → PASS; `python3 core/tools/codemap.py`.

```bash
git add core/crates/autocorrect-gate CODEMAP.md
git commit -m "feat(autocorrect-gate): encrypted persist/load with prior fallback"
```

---

## Task 6: `autocorrect` — `assess()` surfacing the winner's confidence

**Files:**
- Modify: `core/crates/autocorrect/src/lib.rs` (near `correct` at line 132), `core/crates/autocorrect/src/rank.rs`

**Interfaces:**
- Produces: `pub struct CorrectionAssessment { pub correction: Correction, pub available: Option<AvailableCorrection> }`, `pub struct AvailableCorrection { pub winner: String, pub winner_confidence: f64, pub edit_distance: u32, pub winner_lang: String, pub winner_dict_rank: Option<u32> }`, `NoClobberCorrector::assess(&self, token, ctx, device) -> CorrectionAssessment`.
- `available == None` for a vetoed/no-candidate token (nothing to gate); `Some` when today's policy would apply (`winner != word`).

- [ ] **Step 1: Write the failing tests.** In `autocorrect/src/lib.rs` tests:

```rust
#[test]
fn assess_reports_no_available_correction_for_a_known_word() {
    let c = corrector_over(&["hello"]);
    let a = c.assess(&token("hello"), &TypingContext::default(), &DeviceHints::default());
    assert!(a.available.is_none());
    assert!(!a.correction.applied);
}

#[test]
fn assess_reports_the_winner_and_a_finite_confidence_for_a_typo() {
    let c = corrector_over(&["cat", "hat", "bat"]);
    let a = c.assess(&token("xat"), &TypingContext::default(), &DeviceHints::default());
    let av = a.available.expect("a correction is available");
    assert_eq!(av.winner, "cat");
    assert!(av.winner_confidence.is_finite());
    assert_eq!(av.edit_distance, 1);
}
```

- [ ] **Step 2: Run, verify fail.** → FAIL (no `assess`).

- [ ] **Step 3: Implement `assess`.** Factor today's `correct` body into `assess`: run the veto + candidate gather + `score_with_sticky`; when `winner != word`, populate `available` with `winner_confidence = scored[0].1`, `edit_distance` (add a small `rank::edit_distance(typed, winner)` helper — plain Levenshtein, capped), `winner_lang`, `winner_dict_rank` (look up the winner in its pack's `rank`). Keep `correct()` delegating: `self.assess(...).correction`. Expose the winner's score out of `score_with_sticky` (already the `.1`).

- [ ] **Step 4: Run.** `cargo test -p featherkey-autocorrect` → PASS (existing `correct` tests unchanged via delegation).

- [ ] **Step 5: CODEMAP, commit.**

```bash
git add core/crates/autocorrect CODEMAP.md
git commit -m "feat(autocorrect): assess() surfaces the winner's confidence"
```

---

## Task 7: core — hold the gate, init from prior, persist/restore

**Files:**
- Modify: `core/crates/featherkey-core/Cargo.toml`, `core/crates/featherkey-core/src/lib.rs` (field near 143, init near 190), `core/crates/featherkey-core/src/learn.rs` (persist 228, restore 243)

**Interfaces:**
- Consumes: `featherkey_autocorrect_gate::AutocorrectGate`.
- Produces: `FeatherKeyCore.autocorrect_gate: AutocorrectGate` field; persisted alongside the neural ranker.

- [ ] **Step 1: Add the dependency.** In `featherkey-core/Cargo.toml` add `featherkey-autocorrect-gate = { path = "../autocorrect-gate" }`.

- [ ] **Step 2: Write the failing test.** In `learn.rs` tests (mirror the ranker persist test):

```rust
#[test]
fn the_autocorrect_gate_survives_persist_and_restore() {
    let store = InMemoryStore::default();
    let mut core = core_with_en();
    // Drive one suppression so the gate diverges from prior (Task 9 wires the
    // observe call; here reach through the field directly in-test).
    let f = gate_probe();
    for _ in 0..50 { core.autocorrect_gate.reinforce(&f, -1.0, 0.05); }
    core.persist(&store).expect("persist");
    let restored = FeatherKeyCore::restore(&store, langs_en()).expect("restore");
    assert!((restored.autocorrect_gate.residual(&f) - core.autocorrect_gate.residual(&f)).abs() < 1e-6);
}
```

- [ ] **Step 3: Run, verify fail.** → FAIL.

- [ ] **Step 4: Implement.** Add `autocorrect_gate: AutocorrectGate` to the struct; init `AutocorrectGate::from_prior()` in `new`; in `persist()` add `self.autocorrect_gate.persist(store)?;` and in `restore()` add `self.autocorrect_gate = AutocorrectGate::load(store)?;` (mirror the `neural_ranker` lines 228/243). Make the field `pub(crate)`.

- [ ] **Step 5: Run.** `cargo test -p featherkey-core` → PASS.

- [ ] **Step 6: Commit.**

```bash
git add core/crates/featherkey-core/Cargo.toml core/crates/featherkey-core/src/lib.rs core/crates/featherkey-core/src/learn.rs core/Cargo.lock
git commit -m "feat(core): hold the autocorrect gate, persist/restore it"
```

---

## Task 8: core — assemble features, apply the gate, cache the decision

**Files:**
- Modify: `core/crates/featherkey-core/src/correct.rs`

**Interfaces:**
- Consumes: `NoClobberCorrector::assess`, `AutocorrectGate::residual`, `GateFeatures`.
- Produces: gated `applied` in `choose_correction`; a `withheld: Option<String>` on the returned `Correction` (the winner the gate withheld when `applied == false`, else `None`) — carried to the shell for the counterfactual signal; `FeatherKeyCore.last_correction: Option<LastCorrection>` cache (`{ features: GateFeatures, winner: String, applied: bool }`); `pub(crate) const AUTOCORRECT_FLOOR: f64`.
- Note: adding `withheld` to `Correction` (a `contracts` type) is additive; existing callers ignore it. Update the `contracts` `Correction` struct + its constructors in the same task.

- [ ] **Step 1: Write the failing tests.**

```rust
#[test]
fn cold_start_below_floor_is_withheld_and_cached() {
    // A very weak correction (low winner_confidence) drops under the mild floor
    // at cold start, where today it would apply. This is the approved base shift.
    let core = core_with_weak_only(); // fixture whose best fix scores < AUTOCORRECT_FLOOR
    let got = core.choose_correction("xöq", &[], vec![]).expect("ok");
    assert!(!got.applied);
    assert_eq!(core.last_correction.as_ref().map(|l| l.applied), Some(false));
}

#[test]
fn a_strong_correction_still_applies_at_cold_start() {
    let core = en_core(); // "xat" -> "cat", high confidence
    let got = core.choose_correction("xat", &[], vec![]).expect("ok");
    assert!(got.applied);
    assert_eq!(got.primary, "cat");
}

#[test]
fn a_trained_up_gate_applies_a_previously_withheld_correction() {
    let mut core = core_with_weak_only();
    let f = /* the GateFeatures the weak fix produces */;
    for _ in 0..200 { core.autocorrect_gate.reinforce(&f, 1.0, 0.05); }
    let got = core.choose_correction("xöq", &[], vec![]).expect("ok");
    assert!(got.applied, "residual lifted it over the floor");
}
```

- [ ] **Step 2: Run, verify fail.** → FAIL.

- [ ] **Step 3: Implement.** In `choose_correction`: call `assess()`; if `available` is `None`, return its `correction` unchanged (veto/no-candidate path — gate not consulted). If `Some(av)`, build `GateFeatures` (edit distance, `winner_confidence`, `dict_rank_norm = av.winner_dict_rank.map_or(0.0, |r| 1.0/(1.0+r as f32))`, `typed_len_norm`, `momentum_weight = self.momentum.weight_of(&av.winner_lang).ln() as f32`); compute `applied = (av.winner_confidence + self.autocorrect_gate.residual(&features)) >= AUTOCORRECT_FLOOR`; set `self.last_correction = Some(LastCorrection { features, winner: av.winner.clone(), applied })`; return `Correction { primary: if applied { av.winner } else { text }, applied, alternatives: if applied { av... } else { vec![] } }`. Add `pub(crate) const AUTOCORRECT_FLOOR: f64 = 0.0;` and tune in Step 4. Note: `choose_correction` becomes `&mut self` (it now caches) — update its signature and the FFI caller.

- [ ] **Step 4: Tune `AUTOCORRECT_FLOOR`.** Run the existing `correct.rs` fixtures; pick the smallest floor that leaves the strong-correction tests applying while dropping only clearly-weak ones. Enumerate in a comment which existing fixtures (if any) flip applied→not — that is the deliberate, approved base shift. Re-baseline those fixture assertions.

- [ ] **Step 5: Run.** `cargo test -p featherkey-core` → PASS.

- [ ] **Step 6: Commit.**

```bash
git add core/crates/featherkey-core/src/correct.rs
git commit -m "feat(core): gate the autocorrect apply decision behind the neural residual"
```

---

## Task 9: core — `observe_autocorrect_outcome` (three signals, gated)

**Files:**
- Modify: `core/crates/featherkey-core/src/learn.rs`

**Interfaces:**
- Consumes: `self.last_correction`, `AutocorrectGate::reinforce`, the sensitivity/consent gate.
- Produces: `pub fn observe_autocorrect_outcome(&mut self, outcome: AutocorrectOutcome, field: &dyn SensitiveField)`; `pub enum AutocorrectOutcome { Reverted, Kept, Reached }`; `const GATE_KEPT_TARGET/REVERT_TARGET/REACHED_TARGET`.

- [ ] **Step 1: Write the failing tests.**

```rust
#[test]
fn revert_suppresses_a_repeatedly_reverted_correction() {
    let mut core = en_core();
    let _ = core.choose_correction("xat", &[], vec![]); // applies -> caches features
    for _ in 0..6 { // simulate repeated reverts of the same decision
        let _ = core.choose_correction("xat", &[], vec![]);
        core.observe_autocorrect_outcome(AutocorrectOutcome::Reverted, &ordinary_field());
    }
    let got = core.choose_correction("xat", &[], vec![]).expect("ok");
    assert!(!got.applied, "the user's reverts pushed it under the floor");
}

#[test]
fn a_sensitive_field_records_nothing() {
    let mut core = en_core();
    let _ = core.choose_correction("xat", &[], vec![]);
    let before = core.autocorrect_gate.residual(core.last_correction.as_ref().unwrap().features_ref());
    core.observe_autocorrect_outcome(AutocorrectOutcome::Reverted, &sensitive_field());
    let after = core.autocorrect_gate.residual(/* same features */);
    assert_eq!(before, after, "sensitive field must short-circuit");
}
```

- [ ] **Step 2: Run, verify fail.** → FAIL.

- [ ] **Step 3: Implement.** Gate on `self.learning_enabled() && !field.is_sensitive()` (reuse the existing gate helper used by `observe_strip_pick`). On the gate, map outcome→target: `Reverted → REVERT_TARGET (-1.0)`, `Kept → KEPT_TARGET (0.25)`, `Reached → REACHED_TARGET (1.0)` and call `self.autocorrect_gate.reinforce(&last.features, target, GATE_LR)`. `Reached`/`Kept` train even when `last.applied` is false/true respectively; consume nothing (the cache persists until the next `choose_correction`). Add the `AutocorrectOutcome` enum.

- [ ] **Step 4: Run.** `cargo test -p featherkey-core` → PASS.

- [ ] **Step 5: Commit.**

```bash
git add core/crates/featherkey-core/src/learn.rs
git commit -m "feat(core): observe_autocorrect_outcome trains the gate, gated"
```

---

## Task 10: core FFI — `observe_autocorrect_outcome`

**Files:**
- Modify: `core/crates/featherkey-core/src/ffi.rs`

**Interfaces:**
- Produces: FFI `observe_autocorrect_outcome(outcome: FfiAutocorrectOutcome, field)` + `enum FfiAutocorrectOutcome { Reverted, Kept, Reached }`; `FfiCorrection` gains `withheld: Option<String>` mirroring the core `Correction` field (Task 8); `choose_correction` FFI signature updated for `&mut self` if needed.

- [ ] **Step 1: Write the failing test.** In `ffi.rs` tests (uniffi feature), assert the wrapper forwards to the core method and that the enum maps 1:1.

```rust
#[test]
fn ffi_forwards_the_autocorrect_outcome() {
    let core = ffi_core_en();
    let _ = core.choose_correction("xat".into(), vec![], vec![]);
    core.observe_autocorrect_outcome(FfiAutocorrectOutcome::Reverted, ordinary_ffi_field());
    // no panic; behaviour covered by the core test — this pins the FFI surface.
}
```

- [ ] **Step 2: Run under the uniffi feature.** `cargo test -p featherkey-core --features uniffi ffi_forwards` → FAIL.

- [ ] **Step 3: Implement.** Add the `FfiAutocorrectOutcome` enum, a `From` into `AutocorrectOutcome`, and the wrapper method mirroring existing gated FFI wrappers (e.g. `observe_strip_pick`). Wrap interior mutability exactly as the other `&mut` FFI methods do.

- [ ] **Step 4: Run.** `cargo test -p featherkey-core --features uniffi` → PASS. Confirm `python3 core/tools/bindings_check.py --check` reflects the new method (bindings regenerate at build; the checked-in `.kt` updates in Task 11's build step).

- [ ] **Step 5: CODEMAP, commit.**

```bash
git add core/crates/featherkey-core/src/ffi.rs CODEMAP.md
git commit -m "feat(ffi): observe_autocorrect_outcome"
```

---

## Task 11: Kotlin — wire the three signals (+ withheld-reached detection)

**Files:**
- Modify: `apps/android/ime-service/src/main/kotlin/com/featherkey/ime/CorrectionDetector.kt`, `.../FeatherKeyImeService.kt`
- Regenerate UniFFI bindings + rebuild `.so` (build step).

**Interfaces:**
- Consumes: `bridge.observeAutocorrectOutcome(outcome, field)`; the core's `chooseCorrection` result (already used at `correctedWord`, line 810-823).

- [ ] **Step 1: Regenerate bindings + rebuild the `.so`.** Per [[gradle-sandbox-build-workaround]] / `apps/android/ffi-bridge/build-jni.sh` (`ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/28.2.13676358`). Commit the regenerated `featherkey_core.kt` bindings (never the `.so`).

- [ ] **Step 2: Write the failing JVM tests** for the pure detection logic in `CorrectionDetector` — e.g. `onBackspaceUndo` after an autocorrect yields `Reverted`; a boundary passing without a revert yields `Kept`; a delete-retype / strip-pick landing on the last **withheld** word yields `Reached`, and landing on a *different* word does **not**. Add a `withheld: String?` slot the service sets from the core's decision.

```kotlin
@Test fun reaching_the_withheld_word_is_a_reached_signal() {
    val d = CorrectionDetector()
    d.noteWithheld("cat")               // core withheld "cat" for a weak "xat"
    assertEquals(Outcome.REACHED, d.onManualWord("cat"))
    d.noteWithheld("cat")
    assertNull(d.onManualWord("dog"))   // different word -> no signal
}
```

- [ ] **Step 3: Run, verify fail.** `./gradlew :ime-service:testDebugUnitTest --tests '*CorrectionDetector*'` → FAIL.

- [ ] **Step 4: Implement.** In `CorrectionDetector`: add `noteWithheld`, `onManualWord`, and a `Kept` emission when a boundary passes without a revert. In `FeatherKeyImeService.correctedWord` (line 810-823): the `chooseCorrection` result now carries `c.withheld` (Task 8/10) — when non-null, call `corrections.noteWithheld(c.withheld)`. Wire `onBackspaceUndo` → `observeAutocorrectOutcome(REVERTED)`, boundary-without-revert → `KEPT`, and delete-retype/strip-pick → `onManualWord` → `REACHED`, each behind `observeGate()`.

- [ ] **Step 5: Run + on-device smoke.** `./gradlew :ime-service:testDebugUnitTest` → PASS; `:app:installDebug` on SM-A166B; confirm typing + autocorrect + revert with **no crash** (logcat).

- [ ] **Step 6: Commit** (bindings + Kotlin; not the `.so`).

```bash
git add apps/android/ime-service/src/main/kotlin apps/android/ffi-bridge/src/main/kotlin/.../generated/featherkey_core.kt
git commit -m "feat(ime): wire autocorrect-gate outcome signals"
```

---

## Task 12: BDD `@BR-12`, traceability, CODEMAP, full gate

**Files:**
- Create: `core/features/autocorrect-gate.feature`
- Modify: traceability table (per `core/tools/bdd_check.py`), `CODEMAP.md` (regenerated)

- [ ] **Step 1: Write the scenario.** `@BR-12` scenario: cold-start applies a strong correction; after repeated reverts of a specific correction it is suppressed; a known/intended word is never clobbered regardless of gate state; sensitive-field records nothing.

- [ ] **Step 2: Map it to a Rust integration test** in `featherkey-core/tests/` (e.g. `autocorrect_gate.rs`) that exercises the scenario end-to-end via the public API.

- [ ] **Step 3: Run the traceability + BDD check.** `python3 core/tools/bdd_check.py` → `@BR-12` maps.

- [ ] **Step 4: Regenerate CODEMAP + run the full gate.** `python3 core/tools/codemap.py`; `bash core/tools/ci-local.sh` → **ALL GATES PASSED** (fmt, clippy -D, tests, fitness, bdd, codemap --check, coverage ≥98%, cargo-deny zero new deps).

- [ ] **Step 5: Commit.**

```bash
git add core/features/autocorrect-gate.feature core/crates/featherkey-core/tests CODEMAP.md
git commit -m "test(core): @BR-12 autocorrect-gate scenario; full gate green"
```

---

## Definition of Done (whole feature — IMPLEMENTATION_PLAN §3.2)

Tests green · coverage ≥ 98% line · fitness exit 0 · public API matches the design · `@BR-12` scenario present · traceability updated · no panics on the hot path · `ci-local` ALL GATES PASSED · CODEMAP regenerated · zero new dependencies · `.so` rebuilt for on-device smoke (no crash) — device acceptance (does the gate actually suppress/apply on real use) is a post-merge handoff, like prior slices.

## Rollback

Each task is one commit. The gate is additive and defaults to the prior (≈0 residual) with a mild floor; reverting Task 8 restores today's unconditional-apply behaviour. If the base floor proves too aggressive in device testing, `AUTOCORRECT_FLOOR` is a single tunable const (Task 8) — lower it toward the value that reproduces today, or set it to `NEG_INFINITY` to make cold-start identical again.

## Audit log

### Plan gate — ✅ Complete and verified
Audited the plan against the design spec.
- **Spec coverage:** every design section maps to a task — mechanism (winner_confidence + floor + bounded residual) → T3/T6/T8; crate + features → T2–T5; three signals incl. counterfactual → T9/T11; persistence → T5/T7; no-clobber-absolute → T8 (gate consulted only when `assess()` reports a correction available) + T12 scenario; namespace → T1; FFI/shell wiring → T6/T8/T10/T11.
- **Design open-risks each have a task step:** prior ≈0 trainable → T3 step 4 (tune `PRIOR_OFFSET_C` against `nn/src/prior.rs`); `T` value + which fixtures re-baseline → T8 step 4; counterfactual false-positives → T11 step 2 (`onManualWord("dog")` → no signal).
- **Interfaces pinned across tasks:** `GateFeatures`, `AutocorrectGate::{from_prior,residual,reinforce,persist,load}`, `AutocorrectOutcome`/`FfiAutocorrectOutcome`, `withheld: Option<String>` threaded core→FFI→Kotlin (T8/T10/T11), `AUTOCORRECT_FLOOR`/`GATE_LR` consts.
- **Residual (accepted):** two illustrative `/* … */` fixtures in T8/T9 test scaffolding depend on the tuned floor and resolve during implementation — instructive, not silent gaps.
- **Feasibility:** each task is a small TDD cycle mirroring the shipped re-ranker (13-task precedent); DoD + rollback defined; `ci-local` is the exit gate.
Verdict: ✅ faithful, feasible decomposition of the design. Ready to execute.

_(Build-phase `/r-u-sure` runs recorded here as the plan is executed.)_
