# Neural Tap Decoder (global coordinate warp) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a tiny per-user neural *coordinate warp* that learns a smooth,
position-dependent `(Δx, Δy)` shift generalizing systematic tap bias across keys,
applied to the touch before the unchanged `NearestKeyDecoder`, so the keyboard
becomes accurate on keys the user has barely tapped (BR-7).

**Architecture:** New domain crate `featherkey-neural-tap` holding `TapWarp` (two
`featherkey-nn` MLPs, one per axis; cold-start prior ≈ (0,0), trainable). The core
façade applies the warp in `decode` and trains it in the already-gated
`observe_tap` toward target `t = mean_k − (dx,dy)` (few-shot generalization, no
double-correction with the per-key Gaussian). Self-persists under a new
`Namespace::TapWarpModel`; purged by the existing whole-store wipe.

**Tech stack:** Rust (workspace `core/`), `featherkey-nn` substrate, `secure-store`
via the `SecureStore` port. Zero new external dependencies.

**Design:** `docs/superpowers/specs/2026-08-01-neural-tap-decoder-design.md`
(both `/r-u-sure` passes ✅).

## Global Constraints

Every task's requirements implicitly include these — copied verbatim from the spec
and CLAUDE.md:

- **Zero new external dependencies.** `cargo-deny` must stay green; the crate
  depends only on `featherkey-nn` + `featherkey-contracts` (+ dev-deps already in
  the workspace).
- **O(1), allocation-free hot path (BR-46).** Warp read = two fixed-size forward
  passes per tap; warp train = two fixed-size SGD steps per tap. No clone, no
  `sqrt`, no heap growth on the keystroke path.
- **BR-26 sensitivity gating is absolute.** The warp is trained only through the
  existing `should_suppress`-gated `observe_tap`. Never learn in a
  sensitive/password/OTP field.
- **No double-correction.** The warp trains toward `t = mean_k − (dx,dy)` where
  `mean_k = touch_model.offset(k)` read *before* folding the current observation;
  this makes the target zero-mean at a converged key.
- **Cold-start is behaviourally identical, regression-guarded — NOT byte-identical.**
  Assert equal *winner + candidate order*, not equal `f32` confidences.
- **Errors are values.** No `unwrap`/`expect`/`panic` in library code (tests may,
  under the existing `#[allow]`). `load` never returns `Err` for absent/corrupt/
  wrong-shape blobs — it falls back to `from_prior`.
- **Fitness:** ≤500 lines/file, ≤60 lines/function. **Coverage ≥98% line.**
- **CODEMAP is generated** — never hand-edit; regenerate with
  `python3 core/tools/codemap.py` and commit the result.
- **No AI attribution** in commits/code/PRs.
- **Verify each task** with `bash core/tools/ci-local.sh` (or the scoped subset)
  before marking done.

---

## Task 1: BDD scenarios (behaviour first)

**Files:**
- Create: `core/features/neural-tap-decoder.feature`
- Verify: `python3 core/tools/bdd_check.py` (traceability green)

**Interfaces:**
- Consumes: nothing.
- Produces: the `@BR-7` scenarios the Task-6/7 integration tests realize.

- [ ] **Step 1: Write the feature file**

```gherkin
@BR-7
Feature: The tap decoder learns the user's systematic aim and generalizes it

  # BR-7: the keyboard learns the user's typing style and becomes measurably
  # more accurate for that user — here, across keys, not just per key.

  Scenario: A cold-start warp does not change decoding
    Given a fresh tap-warp model
    When the user taps at a spread of positions across the keyboard
    Then every decoded key and candidate order is identical to decoding with no warp

  Scenario: A systematic hand-bias generalizes to an un-tapped key
    Given a user who consistently taps several keys off-centre in the same direction
    When the tap-warp model has learned from those taps
    And the user taps a different key they have never tapped, with the same bias
    Then the decoder resolves the intended un-tapped key
    And an unbiased decoder would have mis-resolved it to a neighbour

  Scenario: The warp does not double-correct a well-learned key
    Given a key whose per-key mean offset has already converged
    When the user taps it on its learned centre
    Then the warp contributes approximately zero additional shift

  Scenario: Learning is suppressed in a sensitive field
    Given a password field
    When the user taps keys off-centre
    Then the tap-warp model is not updated
```

- [ ] **Step 2: Run traceability check**

Run: `python3 core/tools/bdd_check.py`
Expected: PASS — `@BR-7` recognised, no orphaned scenario. If `bdd_check` requires
a traceability row, add the `@BR-7 → neural-tap-decoder.feature` row it names in
its failure message.

- [ ] **Step 3: Regenerate CODEMAP and commit**

```bash
python3 core/tools/codemap.py
git add core/features/neural-tap-decoder.feature CODEMAP.md
git commit -m "test(core): @BR-7 neural tap decoder scenarios"
```

**Definition of Done:** feature file present, `bdd_check` green, CODEMAP regenerated.
**Rollback:** `git revert` the commit; the file is inert (no code depends on it yet).

---

## Task 2: `contracts` — `Namespace::TapWarpModel`

**Files:**
- Modify: `core/crates/contracts/src/lib.rs:27` (enum), `:53` (match arm)
- Test: same file's `#[cfg(test)]` module (mirror the existing namespace tests)

**Interfaces:**
- Consumes: nothing.
- Produces: `Namespace::TapWarpModel` (`as_str() == "tap_warp_model"`), consumed by
  Task 5's persist and Task 7's wiring.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn tap_warp_model_namespace_has_a_stable_key() {
    assert_eq!(Namespace::TapWarpModel.as_str(), "tap_warp_model");
}
```

- [ ] **Step 2: Run it — expect FAIL** (`no variant TapWarpModel`)

Run: `cargo test -p featherkey-contracts tap_warp_model`

- [ ] **Step 3: Add the variant + arm**

In the enum (after `AutocorrectGate`):
```rust
    /// Per-user neural tap-warp weights (sole writer: `featherkey-neural-tap`
    /// via the composition root).
    TapWarpModel,
```
In `as_str`'s match:
```rust
            Namespace::TapWarpModel => "tap_warp_model",
```

- [ ] **Step 4: Run tests — expect PASS**

Run: `cargo test -p featherkey-contracts`

- [ ] **Step 5: Commit**

```bash
python3 core/tools/codemap.py
git add core/crates/contracts CODEMAP.md
git commit -m "feat(contracts): add Namespace::TapWarpModel"
```

**Definition of Done:** variant + arm added, contracts tests green, CODEMAP fresh.
**Rollback:** `git revert`; no consumer yet, so removal is clean.

---

## Task 3: `layout-engine` — `normalize` + `center_of`

**Files:**
- Modify: `core/crates/layout-engine/src/lib.rs` (two methods on `Layout`)
- Test: same file's `#[cfg(test)]` module

**Interfaces:**
- Consumes: `Layout::keys()`, `Key::center()` (exist), `Key` rect fields.
- Produces:
  - `Layout::normalize(&self, x: f32, y: f32) -> (f32, f32)` — maps a surface-local
    pixel to `[-1, 1]` per axis using the layout's logical bounds (max key extent).
    A degenerate/empty layout returns `(0.0, 0.0)` (no panic).
  - `Layout::center_of(&self, ch: char) -> Option<TouchPoint>` — the centre of the
    key that commits `ch`, or `None` if no key on this page commits it.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn normalize_maps_bounds_to_unit_range() {
    let l = Layout::qwerty();
    // Far bottom-right corner (max key right/bottom edge) maps to ~(1,1).
    let (bx, by) = l.keys().iter().fold((0.0_f32, 0.0_f32), |(mx, my), k| {
        (mx.max(k.x + k.width), my.max(k.y + k.height))
    });
    let (ex, ey) = l.normalize(bx, by);
    assert!((ex - 1.0).abs() < 1e-3 && (ey - 1.0).abs() < 1e-3, "corner near 1: {ex},{ey}");
    let (cx, cy) = l.normalize(bx / 2.0, by / 2.0);
    assert!(cx.abs() < 0.05 && cy.abs() < 0.05, "centre near origin: {cx},{cy}");
}

#[test]
fn center_of_returns_a_known_key_and_none_for_absent() {
    let l = Layout::qwerty();
    assert!(l.center_of('q').is_some());
    assert_eq!(l.center_of('€'), None); // not on the qwerty alpha page
}

#[test]
fn normalize_never_panics_on_empty_layout() {
    let l = Layout::default();               // empty
    assert_eq!(l.normalize(10.0, 10.0), (0.0, 0.0));
}
```

- [ ] **Step 2: Run — expect FAIL** (`no method normalize`)

Run: `cargo test -p featherkey-layout-engine normalize`

- [ ] **Step 3: Implement**

```rust
impl Layout {
    /// Logical bounds (far right/bottom edge over all keys), or `(0,0)` if empty.
    /// `Key` fields are public (`x, y, width, height`), so the true rect edge is
    /// `x + width` / `y + height` — NOT `2·center`, which overshoots off-origin keys.
    fn bounds(&self) -> (f32, f32) {
        self.keys.iter().fold((0.0_f32, 0.0_f32), |(mx, my), k| {
            (mx.max(k.x + k.width), my.max(k.y + k.height))
        })
    }

    /// Map a surface-local pixel to `[-1, 1]` per axis. `(0,0)` for an empty layout.
    #[must_use]
    pub fn normalize(&self, x: f32, y: f32) -> (f32, f32) {
        let (bx, by) = self.bounds();
        if bx <= 0.0 || by <= 0.0 {
            return (0.0, 0.0);
        }
        ((x / bx) * 2.0 - 1.0, (y / by) * 2.0 - 1.0)
    }

    /// Centre of the key that commits `ch` (matched via `KeyId::ch`), or `None` if
    /// no key on this page commits it.
    #[must_use]
    pub fn center_of(&self, ch: char) -> Option<TouchPoint> {
        self.keys.iter().find(|k| k.id.ch() == ch).map(Key::center)
    }
}
```
(`Key` fields are `pub` and in the same module, so `bounds`/`center_of` read them
directly. `KeyId::ch()` is the existing accessor in `featherkey-kernel`.)

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p featherkey-layout-engine`

- [ ] **Step 5: Commit**

```bash
python3 core/tools/codemap.py
git add core/crates/layout-engine CODEMAP.md
git commit -m "feat(layout-engine): add Layout::normalize and center_of"
```

**Definition of Done:** both methods, tests green (incl. empty-layout no-panic),
CODEMAP fresh.
**Rollback:** `git revert`; methods are additive, no caller yet.

---

## Task 4: new crate `featherkey-neural-tap` — the `TapWarp` model

**Files:**
- Create: `core/crates/neural-tap/Cargo.toml`, `core/crates/neural-tap/README.md`,
  `core/crates/neural-tap/src/lib.rs`
- Modify: `core/Cargo.toml:43` (add `"crates/neural-tap"` to `members`)

**Interfaces:**
- Consumes: `featherkey_nn::Mlp` (`with_weights`, `forward`, `train_step`).
- Produces: `TapWarp` with
  `from_prior() -> Self`,
  `warp(&self, nx: f32, ny: f32) -> (f32, f32)` (clamped ±`WARP_BOUND`),
  `reinforce(&mut self, nx: f32, ny: f32, tx: f32, ty: f32, lr: f32)`,
  and pub consts `INPUTS = 2`, `WARP_BOUND`, `WARP_LR`.

- [ ] **Step 1: Cargo.toml + workspace member**

`core/crates/neural-tap/Cargo.toml`:
```toml
[package]
name = "featherkey-neural-tap"
version = "0.0.0"
publish = false
edition.workspace = true
license.workspace = true
rust-version.workspace = true
description = "Tiny per-user neural tap-warp: a bounded (dx,dy) coordinate shift over normalized tap position that generalizes systematic aim across keys."

[package.metadata.featherkey]
layer = "domain"

[lints]
workspace = true

[dependencies]
featherkey-nn = { path = "../nn" }
featherkey-contracts = { path = "../contracts" }
```
Add `"crates/neural-tap",` to `core/Cargo.toml` `members`.

- [ ] **Step 2: Write the failing unit tests** (`src/lib.rs` `#[cfg(test)]`)

```rust
#[test]
fn cold_start_warp_is_near_zero_across_the_grid() {
    let w = TapWarp::from_prior();
    for nx in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        for ny in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            let (dx, dy) = w.warp(nx, ny);
            assert!(dx.abs() < 0.05 && dy.abs() < 0.05, "cold warp {dx},{dy} @ {nx},{ny}");
        }
    }
}

#[test]
fn warp_output_is_bounded() {
    let mut w = TapWarp::from_prior();
    // A large but FINITE over-bound target (real targets are ≤ keyboard px). A 1e6
    // target would overflow the weights to inf/NaN, and `f32::NAN.clamp(..)` is NaN
    // — so keep it realistic; the point under test is the ±WARP_BOUND clamp.
    for _ in 0..5_000 { w.reinforce(0.5, 0.5, 200.0, -200.0, WARP_LR); }
    let (dx, dy) = w.warp(0.5, 0.5);
    assert!(dx.abs() <= WARP_BOUND + 1e-3 && dy.abs() <= WARP_BOUND + 1e-3);
    assert!(dx.is_finite() && dy.is_finite(), "clamped output must stay finite");
}

#[test]
fn reinforce_moves_toward_a_systematic_offset_target() {
    // A stream whose target is a constant negative-x shift (cancel a +x bias).
    let mut w = TapWarp::from_prior();
    let before = w.warp(0.3, 0.3).0;
    for _ in 0..500 { w.reinforce(0.3, 0.3, -20.0, 0.0, WARP_LR); }
    let after = w.warp(0.3, 0.3).0;
    assert!(after < before - 1.0, "x-warp should move negative: {before} -> {after}");
}

#[test]
fn a_zero_mean_target_stream_keeps_the_warp_near_zero() {
    // Targets that average to 0 (a converged key) must not accumulate drift.
    let mut w = TapWarp::from_prior();
    for i in 0..1000 {
        let t = if i % 2 == 0 { 15.0 } else { -15.0 };
        w.reinforce(0.2, -0.4, t, 0.0, WARP_LR);
    }
    assert!(w.warp(0.2, -0.4).0.abs() < 5.0, "zero-mean target must not drift far");
}
```

- [ ] **Step 3: Run — expect FAIL** (`cannot find type TapWarp`)

Run: `cargo test -p featherkey-neural-tap`

- [ ] **Step 4: Implement `TapWarp`** (`src/lib.rs`)

```rust
//! Tiny per-user neural tap-warp: a bounded (dx,dy) shift over normalized tap
//! position, generalizing systematic aim across keys. Pure math; no I/O, no RNG.

mod persist; // Task 5

use featherkey_nn::Mlp;

/// Position inputs: normalized (x, y).
pub const INPUTS: usize = 2;
/// Max per-axis shift in logical px — a warp can never fling a tap across keys.
pub const WARP_BOUND: f32 = 40.0;
/// Learning rate per tap. Small: track the slow systematic field, not per-tap noise.
pub const WARP_LR: f32 = 0.01;

// Signed-pair prior constants (mirror autocorrect-gate::from_prior). Position is
// centred at 0 in [-1,1], so each feature centre is 0.0.
const PRIOR_SCALE: f32 = 4.0;
const PRIOR_MARGIN: f32 = 0.05;
const PRIOR_WEIGHT: f32 = 0.005;

/// A per-user coordinate warp: two independent scalar MLPs (Δx and Δy) over the
/// normalized tap position. Cold start ≈ (0,0) everywhere yet trainable.
#[derive(Debug, Clone)]
pub struct TapWarp {
    dx: Mlp,
    dy: Mlp,
}

impl TapWarp {
    /// One axis's zero-output-but-trainable prior: two hidden units per input form
    /// a centred signed reader that cancels to ~0 while every input weight keeps a
    /// gradient path (identical construction to `AutocorrectGate::from_prior`).
    fn axis_prior() -> Mlp {
        let hidden = 2 * INPUTS;
        let mut w1 = vec![0.0_f32; hidden * INPUTS];
        let mut b1 = vec![0.0_f32; hidden];
        let mut w2 = vec![0.0_f32; hidden];
        for j in 0..INPUTS {
            let (pos, neg) = (2 * j, 2 * j + 1);
            w1[pos * INPUTS + j] = PRIOR_SCALE;
            b1[pos] = PRIOR_MARGIN;          // centre μ_j = 0
            w2[pos] = PRIOR_WEIGHT;
            w1[neg * INPUTS + j] = -PRIOR_SCALE;
            b1[neg] = PRIOR_MARGIN;
            w2[neg] = -PRIOR_WEIGHT;
        }
        Mlp::with_weights(w1, b1, w2, 0.0, INPUTS, hidden)
    }

    #[must_use]
    pub fn from_prior() -> Self {
        Self { dx: Self::axis_prior(), dy: Self::axis_prior() }
    }

    /// The learned (Δx, Δy) shift for a normalized tap, each clamped ±`WARP_BOUND`.
    #[must_use]
    pub fn warp(&self, nx: f32, ny: f32) -> (f32, f32) {
        let x = [nx, ny];
        (
            self.dx.forward(&x).clamp(-WARP_BOUND, WARP_BOUND),
            self.dy.forward(&x).clamp(-WARP_BOUND, WARP_BOUND),
        )
    }

    /// One squared-error SGD step per axis toward `(tx, ty)` (design §6 target).
    pub fn reinforce(&mut self, nx: f32, ny: f32, tx: f32, ty: f32, lr: f32) {
        let x = [nx, ny];
        let ddx = 2.0 * (self.dx.forward(&x) - tx);
        self.dx.train_step(&x, ddx, lr);
        let ddy = 2.0 * (self.dy.forward(&x) - ty);
        self.dy.train_step(&x, ddy, lr);
    }
}
```
(`persist` mod is added empty-then-filled in Task 5; to compile Task 4 alone,
create `src/persist.rs` with just the `#![allow(unused)]` stub or fold Task 5 in —
the implementer keeps the file compiling.)

- [ ] **Step 5: Run — expect PASS**, then write `README.md` (crate anatomy per
  ARCHITECTURE.md §5.2: one job, ports, deferred items).

Run: `cargo test -p featherkey-neural-tap`

- [ ] **Step 6: Commit**

```bash
python3 core/tools/codemap.py
git add core/crates/neural-tap core/Cargo.toml CODEMAP.md
git commit -m "feat(neural-tap): TapWarp coordinate-warp model with cold-start prior"
```

**Definition of Done:** crate builds, 4 unit tests green, README present, workspace
member added, CODEMAP fresh, coverage ≥98% for the crate.
**Rollback:** remove the member line + `git revert`; no core consumer yet.

---

## Task 5: `featherkey-neural-tap` — persist / load

**Files:**
- Create: `core/crates/neural-tap/src/persist.rs`
- Depends on: Task 2 (`Namespace::TapWarpModel`), Task 4 (`TapWarp`).

**Interfaces:**
- Produces: `TapWarp::persist(&self, store: &impl SecureStore) -> Result<(), StoreError>`
  and `TapWarp::load(store: &impl SecureStore) -> Result<Self, StoreError>`
  (absent / corrupt / wrong-shape ⇒ `from_prior`, never `Err`).

- [ ] **Step 1: Write the failing tests** (`persist.rs` `#[cfg(test)]`, mirror the
  gate's `InMemoryStore`)

```rust
#[test]
fn round_trips_through_the_store() {
    let store = InMemoryStore::default();
    let mut w = TapWarp::from_prior();
    for _ in 0..200 { w.reinforce(0.3, -0.2, -18.0, 6.0, WARP_LR); }
    w.persist(&store).expect("persist");
    let back = TapWarp::load(&store).expect("load");
    let (a, b) = w.warp(0.3, -0.2);
    let (c, d) = back.warp(0.3, -0.2);
    assert!((a - c).abs() < 1e-6 && (b - d).abs() < 1e-6);
}

#[test]
fn absent_or_corrupt_blob_falls_back_to_prior() {
    let store = InMemoryStore::default();
    let w = TapWarp::load(&store).expect("absent -> prior");
    assert!(w.warp(0.5, 0.5).0.abs() < 0.05);
    store.put(Namespace::TapWarpModel, b"v1", b"garbage").unwrap();
    let w2 = TapWarp::load(&store).expect("corrupt -> prior, never Err");
    assert!(w2.warp(0.5, 0.5).0.abs() < 0.05);
}

#[test]
fn a_valid_but_wrong_shape_blob_falls_back_to_prior() {
    // A well-formed blob whose inner MLPs have the WRONG input width (3, not 2)
    // must degrade to the prior — this is the only test that exercises the
    // `inputs() == INPUTS` guard (the "garbage" case fails earlier in from_bytes),
    // so it closes the coverage on that branch.
    use featherkey_nn::Mlp;
    let store = InMemoryStore::default();
    let three_in = Mlp::with_weights(vec![0.0; 6], vec![0.0, 0.0], vec![0.0, 0.0], 0.0, 3, 2);
    let inner = three_in.to_bytes();
    let mut blob = (inner.len() as u32).to_le_bytes().to_vec();
    blob.extend_from_slice(&inner);
    blob.extend_from_slice(&inner);
    store.put(Namespace::TapWarpModel, b"v1", &blob).unwrap();
    let w = TapWarp::load(&store).expect("wrong-shape -> prior, never Err");
    assert!(w.warp(0.5, 0.5).0.abs() < 0.05);
}
```

- [ ] **Step 2: Run — expect FAIL** (`no method persist`)

Run: `cargo test -p featherkey-neural-tap persist`

- [ ] **Step 3: Implement** (two MLPs in one length-prefixed, versioned blob)

```rust
use featherkey_contracts::{Namespace, SecureStore, StoreError};
use featherkey_nn::Mlp;
use crate::{TapWarp, INPUTS};

const BLOB_KEY: &[u8] = b"v1";

impl TapWarp {
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), StoreError> {
        let dx = self.dx.to_bytes();
        let dy = self.dy.to_bytes();
        let mut blob = Vec::with_capacity(4 + dx.len() + dy.len());
        blob.extend_from_slice(&(dx.len() as u32).to_le_bytes());
        blob.extend_from_slice(&dx);
        blob.extend_from_slice(&dy);
        store.put(Namespace::TapWarpModel, BLOB_KEY, &blob)
    }

    pub fn load(store: &impl SecureStore) -> Result<Self, StoreError> {
        let Some(bytes) = store.get(Namespace::TapWarpModel, BLOB_KEY)? else {
            return Ok(Self::from_prior());
        };
        Ok(decode(&bytes).unwrap_or_else(Self::from_prior))
    }
}

/// Parse the `[u32 dx_len][dx][dy]` blob; any shape/format problem ⇒ `None` so the
/// caller falls back to the prior (never an error the caller must handle).
fn decode(bytes: &[u8]) -> Option<TapWarp> {
    let (len_bytes, rest) = bytes.split_first_chunk::<4>()?;
    let dx_len = u32::from_le_bytes(*len_bytes) as usize;
    let (dx_b, dy_b) = rest.split_at_checked(dx_len)?;
    let dx = Mlp::from_bytes(dx_b).ok().filter(|m| m.inputs() == INPUTS)?;
    let dy = Mlp::from_bytes(dy_b).ok().filter(|m| m.inputs() == INPUTS)?;
    Some(TapWarp { dx, dy })
}
```
(`split_first_chunk`/`split_at_checked` are available on the pinned toolchain
(rust-version 1.85; `secure-store/src/lib.rs:134` already uses `split_first_chunk`)
and are panic-free by construction — they return `Option`, never index-panic.)

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p featherkey-neural-tap`

- [ ] **Step 5: Commit**

```bash
python3 core/tools/codemap.py
git add core/crates/neural-tap CODEMAP.md
git commit -m "feat(neural-tap): encrypted self-persistence (absent/corrupt -> prior)"
```

**Definition of Done:** persist/load round-trips; absent/corrupt/wrong-shape ⇒
prior with no `Err`; coverage ≥98%; CODEMAP fresh.
**Rollback:** `git revert`; the model still works in-memory without persistence.

---

## Task 6: `featherkey-core` — apply the warp in `decode` (read path)

**Files:**
- Modify: `core/crates/featherkey-core/Cargo.toml` (add `featherkey-neural-tap` dep)
- Modify: `core/crates/featherkey-core/src/lib.rs` — `use` (~:73), struct field
  (~:135, near `touch_model`), `new()` init (~:193), `decode` (`:309`), and two
  `#[cfg(test)] pub(crate)` accessors next to `neural_ranker()` (`:358`)
- Test: `core/crates/featherkey-core/src/lib.rs` `#[cfg(test)] mod tests` (unit)

**Interfaces:**
- Consumes: `TapWarp` (Task 4), `Layout::normalize` (Task 3).
- Produces: `FeatherKeyCore` field `tap_warp: TapWarp`; a `decode` that warps the
  touch before decoding; test accessors `tap_warp()` / `layout()`. Cold-start decode
  is winner+order identical to warp-off.

**Why unit tests, not `tests/*.rs`:** the ranker/gate expose internal state through
`#[cfg(test)] pub(crate)` accessors (e.g. `neural_ranker()`, `lib.rs:358`), which
are visible **only** to in-crate unit tests (an integration test compiles the lib
without `cfg(test)`). The cold-start-identity and (Task 7) state assertions poke
the warp/layout, so they live in `#[cfg(test)] mod tests`, exactly like the gate's
`the_autocorrect_gate_survives_persist_and_restore` (`learn.rs:450`).

- [ ] **Step 1: Write the failing unit test** (in `lib.rs` `#[cfg(test)] mod tests`)

```rust
#[test]
fn cold_start_warp_shift_is_negligible_so_decode_is_unchanged() {
    // A fresh core's warp is the prior. Its (Δx,Δy) at any position is far below
    // inter-key spacing, so the pre-warp touch and the warped touch decode to the
    // SAME argmin — i.e. decode is behaviourally identical to no warp at cold start.
    let c = core();                                   // existing test helper
    for &(x, y) in &[(120.0, 80.0), (300.0, 80.0), (500.0, 200.0)] {
        let (nx, ny) = c.layout().normalize(x, y);
        let (dx, dy) = c.tap_warp().warp(nx, ny);
        assert!(dx.abs() < 1e-2 && dy.abs() < 1e-2, "cold warp {dx},{dy} @ {x},{y}");
    }
}

#[test]
fn decode_is_deterministic() {
    let mut c = core();
    let a = c.decode(300.0, 80.0).unwrap();
    let b = c.decode(300.0, 80.0).unwrap();
    assert_eq!(a.best, b.best);
    assert!(a.best.is_some());
}
```

- [ ] **Step 2: Run — expect FAIL** (no `tap_warp` field / no accessor)

Run: `cargo test -p featherkey-core cold_start_warp`

- [ ] **Step 3: Implement the read path + accessors**

`Cargo.toml`: add `featherkey-neural-tap = { path = "../neural-tap" }`.
`lib.rs`:
```rust
use featherkey_neural_tap::TapWarp;                 // near other domain uses
// struct field (near touch_model):
    tap_warp: TapWarp,
// new():
    tap_warp: TapWarp::from_prior(),
// decode() — warp the touch before decoding:
pub fn decode(&mut self, x: f32, y: f32) -> Result<DecodeResult, FeatherKeyError> {
    let (nx, ny) = self.layout.normalize(x, y);
    let (wdx, wdy) = self.tap_warp.warp(nx, ny);
    let touch = TouchPoint::new(x + wdx, y + wdy);
    let candidates = self.decoder.decode(touch, &self.layout, &self.touch_model)?;
    // …unchanged: push TapDistribution from `candidates`, build DecodeResult…
}
// test accessors (next to `neural_ranker()`):
    #[cfg(test)]
    pub(crate) fn tap_warp(&self) -> &TapWarp { &self.tap_warp }
    #[cfg(test)]
    pub(crate) fn layout(&self) -> &featherkey_layout_engine::Layout { &self.layout }
```

- [ ] **Step 4: Run — expect PASS**, and run the full core suite (no regression in
  the existing decode/tracer tests).

Run: `cargo test -p featherkey-core`

- [ ] **Step 5: Commit**

```bash
python3 core/tools/codemap.py
git add core/crates/featherkey-core CODEMAP.md
git commit -m "feat(core): apply neural tap-warp to the touch before decode"
```

**Definition of Done:** warp applied in `decode`; cold-start decode identity held
(winner+order); full core suite green; coverage ≥98%; CODEMAP fresh.
**Rollback:** `git revert`; decode falls back to the direct touch (today's behaviour).

---

## Task 7: `featherkey-core` — train the warp + persist/restore (write path)

**Files:**
- Modify: `core/crates/featherkey-core/src/learn.rs` — `observe_tap` (`:187`),
  `persist` (`:246`), `restore` (`:262`), and the `#[cfg(test)] mod tests` block
- Test: `core/crates/featherkey-core/src/learn.rs` `#[cfg(test)] mod tests` (unit,
  beside `the_autocorrect_gate_survives_persist_and_restore`)

**Interfaces:**
- Consumes: `TapWarp::reinforce` / `persist` / `load`; `touch_model.offset`;
  `Layout::center_of` + `normalize`; the Task-6 `tap_warp()` / `layout()` accessors.
- Produces: warp trained on every gated tap toward `t = mean_k − (dx,dy)`; warp
  persisted/restored alongside the other models.

- [ ] **Step 1: Write the failing unit tests** (reuse the module's existing
  `SensitiveContextSource` test doubles and in-memory store; a `core_qwerty()`
  helper builds a full qwerty core)

```rust
#[test]
fn a_systematic_bias_generalizes_to_an_untapped_key() {
    // Bias must EXCEED the half-key distance (~50px on a ~100px qwerty key) or an
    // unbiased tap already lands on the intended key and proves nothing. At +60px a
    // tap right of 'f' is nearer 'g' (unbiased mis-resolves). The bounded warp
    // (±WARP_BOUND) pulls it back onto 'f' — even a partial correction flips the
    // argmin, since f+60 − WARP_BOUND ≈ f+20 is nearest 'f'.
    let mut c = core_qwerty();
    let f = c.layout().center_of('f').unwrap();
    // Sanity: unbiased decode of the biased tap resolves the WRONG key.
    assert_ne!(c.decode(f.x + 60.0, f.y).unwrap().best.as_deref(), Some("f"));
    // Teach the same +60px rightward bias on neighbouring keys (never 'f').
    for _ in 0..100 {
        for ch in ['a', 's', 'd', 'g', 'h'] {
            c.observe_tap(ch, 60.0, 0.0, &non_sensitive()).unwrap();
        }
    }
    let got = c.decode(f.x + 60.0, f.y).unwrap();
    assert_eq!(got.best.as_deref(), Some("f"),
        "the learned warp must generalize the bias to the never-tapped 'f'");
}

#[test]
fn a_converged_key_gets_no_extra_shift() {
    let mut c = core_qwerty();
    for _ in 0..100 { c.observe_tap('j', 0.0, 0.0, &non_sensitive()).unwrap(); } // on-centre
    let j = c.layout().center_of('j').unwrap();
    let (nx, ny) = c.layout().normalize(j.x, j.y);
    let (dx, dy) = c.tap_warp().warp(nx, ny);
    assert!(dx.abs() < 3.0 && dy.abs() < 3.0, "no double-correction: {dx},{dy}");
}

#[test]
fn a_sensitive_field_does_not_train_the_warp() {
    let mut c = core_qwerty();
    let j = c.layout().center_of('j').unwrap();
    let (nx, ny) = c.layout().normalize(j.x, j.y);
    let before = c.tap_warp().warp(nx, ny);
    for _ in 0..50 { c.observe_tap('j', 25.0, 10.0, &sensitive()).unwrap(); }
    assert_eq!(c.tap_warp().warp(nx, ny), before, "sensitive taps must not train");
}

#[test]
fn the_tap_warp_survives_persist_and_restore() {
    let store = mem_store();
    let mut c = core_qwerty();
    for _ in 0..60 { c.observe_tap('k', 18.0, -6.0, &non_sensitive()).unwrap(); }
    c.persist(&store).unwrap();
    let mut restored = core_qwerty();
    restored.restore(&store).unwrap();
    let k = c.layout().center_of('k').unwrap();
    let (nx, ny) = c.layout().normalize(k.x, k.y);
    assert_eq!(restored.tap_warp().warp(nx, ny), c.tap_warp().warp(nx, ny));
}
```
(`non_sensitive()`/`sensitive()`/`mem_store()` reuse the module's existing test
helpers — confirm their exact names when implementing; the gate's persist/restore
test already uses them.)

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p featherkey-core a_systematic_bias`

- [ ] **Step 3: Implement the write path**

`learn.rs::observe_tap` (after the existing sensitivity gate, reading `mean_k`
BEFORE the fold):
```rust
pub fn observe_tap(&mut self, key: char, dx: f32, dy: f32, field: &dyn SensitiveContextSource)
    -> Result<(), FeatherKeyError>
{
    if self.sensitivity.should_suppress(field) { return Ok(()); }
    let (mx, my) = self.touch_model.offset(KeyId(key));       // mean_k BEFORE fold
    self.touch_model.observe(KeyId(key), dx, dy)?;
    if let Some(center) = self.layout.center_of(key) {
        let (nx, ny) = self.layout.normalize(center.x + dx, center.y + dy);
        self.tap_warp.reinforce(nx, ny, mx - dx, my - dy, WARP_LR);   // t = mean_k − (dx,dy)
    }
    Ok(())
}
```
`learn.rs::persist` — add `self.tap_warp.persist(store)?;` beside the others (`:251`).
`learn.rs::restore` — add `self.tap_warp = TapWarp::load(store)?;` beside the others
(`:267`). Import `WARP_LR`/`TapWarp` at the top of `learn.rs`.

- [ ] **Step 4: Run — expect PASS**, then the full core suite + fitness.

Run: `cargo test -p featherkey-core && python3 core/tools/fitness/check.py`

- [ ] **Step 5: Commit**

```bash
python3 core/tools/codemap.py
git add core/crates/featherkey-core CODEMAP.md
git commit -m "feat(core): train the tap-warp from gated taps; persist/restore it"
```

**Definition of Done:** all four integration tests green; generalization, no-double-
correction, sensitivity-gate, and persist/restore all proven; `@BR-7` scenarios
realized; fitness green; coverage ≥98%; CODEMAP fresh.
**Rollback:** `git revert`; the warp stays at its prior (inert) — decode unaffected.

---

## Task 8: Full gate + traceability sweep

**Files:**
- Modify (if needed): traceability rows the BDD/DoD checks name; `CODEMAP.md`.

- [ ] **Step 1: Run the whole CI gate locally**

Run: `bash core/tools/ci-local.sh`
Expected: ALL GATES PASSED — fmt, clippy `-D`, `cargo test --workspace`, coverage
≥98%, `cargo-deny` (advisories/bans/licenses/sources, **zero new deps**), fitness
(≤500/≤60), `bdd_check` (`@BR-7`), `codemap --check` clean.

- [ ] **Step 2: Confirm no FFI/bindings drift**

The warp is core-internal (no new FFI). Confirm `decode`/`observe_tap` signatures
are unchanged at the FFI boundary and the committed UniFFI bindings still match
(no `ffi.rs` change). If `ci-local` has a bindings check, it must report UNCHANGED.

- [ ] **Step 3: Commit any final CODEMAP/traceability delta**

```bash
git add -A && git commit -m "chore(neural-tap): finalize traceability + CODEMAP"
```

**Definition of Done:** `ci-local.sh` ALL GATES PASSED, pasted into the build audit
log; no FFI/bindings change; zero new deps.
**Rollback:** the feature is on its own branch; abandon by not merging.

---

## Self-review (author checklist, run once)

- **Spec coverage:** BR-7 (generalization scenario, Task 7) ✓; the warp model
  (Task 4) ✓; persistence + purge-for-free (Task 5, Task 7 restore) ✓; cold-start
  identity (Task 6) ✓; BR-26 gate (Task 7) ✓; O(1) hot path (two fixed passes,
  Global Constraints) ✓; zero new deps (Task 8) ✓. Deferred Increment 2 is *not*
  planned, by design.
- **Type consistency:** `TapWarp::{from_prior,warp,reinforce,persist,load}`,
  `INPUTS/WARP_BOUND/WARP_LR`, `Namespace::TapWarpModel`, `Layout::{normalize,
  center_of}` used identically across Tasks 2–7. Target `t = mean_k − (dx,dy)`
  stated once (Global Constraints) and used once (Task 7).
- **Placeholders:** none — every code step carries real code; the two spots that
  depend on the exact `Key`/toolchain API (`center_of`/`commits`, `split_at_checked`)
  name the fallback explicitly rather than hand-waving.
- **Open items from the design (§12)** land in concrete tasks: normalize/center_of
  (Task 3), constants (Task 4, tunable), two-MLP blob (Task 5), warp placement in
  the façade `decode` (Task 6).

## Audit log

### Pass 1 — ✅ Complete and verified (plan phase)
**Audited against:** the design doc (`…-neural-tap-decoder-design.md`) — the plan is
audited against the design, per CLAUDE.md §1.1 — plus the real API signatures every
task references.

**Every API reference verified first-hand (not assumed):**
- `Key` fields are `pub id: KeyId, x, y, width, height` (`layout-engine/src/lib.rs:24-30`);
  `Key::center` = `x+width/2, y+height/2` (`:47`); `KeyId::ch()` exists (kernel).
- Test-accessor pattern: `neural_ranker()` is `#[cfg(test)] pub(crate)` (`lib.rs:358`),
  visible only to **unit** tests; the gate's persist/restore test is a unit test in
  `learn.rs` `#[cfg(test)] mod tests` (`:273/:450`).
- Core wiring lines: `observe_tap` (`learn.rs:187`), `persist` (`:246/:251`),
  `restore` (`:262/:267`), `new()` field init (`lib.rs:193-198`); deps present
  (`neural-ranker`/`autocorrect-gate` at `featherkey-core/Cargo.toml:45-46`).
- `nn::Mlp` is single-scalar-output (`nn/src/lib.rs:49`) → two-MLP warp confirmed;
  `with_weights(w1,b1,w2,b2,inputs,hidden)` (`:24`); `to_bytes`/`from_bytes`/
  `from_linear`/`train_step` present (`codec.rs`/`prior.rs`/`train.rs`).
- `Namespace` enum + `as_str` (`contracts/src/lib.rs:27/53`); workspace `members`
  ends `crates/autocorrect-gate` (`core/Cargo.toml:42-43`) — insertion point real.
- Persist template read in full (`autocorrect-gate/src/persist.rs`): the
  absent/corrupt/wrong-shape ⇒ prior contract the warp mirrors.

**Defects found this pass → fixed in the plan:**
1. **`bounds()` bug** — used `2·center` (overshoots any off-origin key); corrected to
   `x+width` / `y+height` (Task 3).
2. **Test visibility** — Task 6/7 poked `#[cfg(test)]` state from `tests/*.rs`, which
   cannot see it; moved to in-crate unit tests + added `tap_warp()`/`layout()`
   accessors mirroring `neural_ranker()`.
3. **Phantom `Key::commits(ch)`** — replaced with `k.id.ch() == ch` (Task 3).
4. **Generalization test premise false** — a 20px bias on ~100px keys is still
   nearest the intended key unbiased; changed to 60px with a pre-training
   `assert_ne!` sanity check that the unbiased decode mis-resolves (Task 7).

**Not verified (correctly):** no code exists, so nothing was compiled or run — TDD
Red/Green and `ci-local` are the build phase. Two spots still name a fallback rather
than a proven call (`split_at_checked` availability on the pinned toolchain, Task 5;
exact `non_sensitive()/sensitive()/mem_store()` helper names, Task 7) — legitimate
build-time reconciliation, flagged inline, not hidden.

**Verdict:** ✅ plan complete, bite-sized, TDD-first, each task independently
verifiable with a rollback; every referenced symbol checked against source; four
real defects caught and fixed. Ready for build (subagent-driven-development).

### Pass 2 — ✅ Complete and verified (re-audit; found + fixed 2 real defects, resolved 1 risk)
Re-ran the gate against "would this compile and hit the gates," not just "does it
read right." Findings, each changed:
- **Coverage hole (Task 5).** The `inputs() == INPUTS` wrong-shape guard in `decode`
  was unreached by any test — the "garbage" blob fails inside `from_bytes` first, so
  the guard branch would show uncovered and threaten the ≥98% gate. Added
  `a_valid_but_wrong_shape_blob_falls_back_to_prior` (a well-formed blob wrapping
  3-input MLPs) to exercise exactly that branch.
- **NaN bug in a test (Task 4).** `warp_output_is_bounded` trained toward `1e6`,
  which overflows the weights to inf/NaN; `f32::NAN.clamp(..)` returns NaN, so the
  bound assertion would fail. Changed to a finite over-bound target (±200) and added
  an `is_finite()` assertion. (Confirmed the training path itself can't be fed NaN:
  `observe_tap`'s `touch_model.observe(..)?` rejects non-finite `dx/dy` and returns
  before the warp step.)
- **Toolchain risk resolved, hedge removed.** `split_first_chunk`/`split_at_checked`
  are available (rust-version **1.85**; `secure-store/src/lib.rs:134` already uses
  `split_first_chunk`) and are `Option`-returning (panic-free). Task 5's hedge
  replaced with that evidence.

**Also re-verified (held up):** `DecodeResult.best` is `Option<String>`
(`.as_deref() == Some("f")` valid); `observe_tap`'s `?`-early-return correctly skips
the warp for rejected taps; `TapWarp` derives `Debug`/`Clone` so the `FeatherKeyCore`
field is fine; the default `core()` helper already builds a qwerty layout
(`alpha_for("en")`), so `center_of('f')` resolves.

**Still not run (correctly):** no compile/test — that's Red/Green in the build. One
residual test-strength note handed to the implementer: `warp_output_is_bounded`
proves the invariant `≤ WARP_BOUND` but, if constants are under-tuned, could pass
without the clamp actually engaging; tune iters (or assert the pre-clamp forward
exceeds the bound) so the clamp is genuinely exercised.

**Verdict:** ✅ still complete; two real defects (coverage, NaN) fixed, one risk
retired with evidence. No plan claim left resting on inference. Ready for build.
