# Neural Tap Decoder (global coordinate warp) — Design

**Status:** design (pre-plan). Gated by `/r-u-sure` before advancing to the plan.
**Date:** 2026-08-01
**Roadmap:** neural app #3 of 4 — re-ranker (BR-11, done) → autocorrect gate
(BR-12, done) → **tap decoder (this)** → next-word LM (app #4).
**Primary requirement:** **BR-7** — *"the keyboard must learn the individual
user's typing style over time and become measurably more accurate for that
user."* Supporting: BR-5 (register the intended key), BR-6 (consistent/decisive
accuracy), BR-46 (must-beat iOS "missed keys when typing fast" — the O(1) hot
path).

---

## 1. Problem

Today's decoder already personalizes tap geometry: `input-decoder`
(`NearestKeyDecoder`) scores each key by Mahalanobis distance from the touch to
that key's **model-biased** centre, where the bias is a per-key learned 2D
Gaussian (mean + covariance) owned by `touch-model`
(`core/crates/touch-model/src/lib.rs:114` `observe`, running mean + Welford
covariance).

Two structural gaps remain, both verified against the code:

1. **No cross-key generalization.** Each key is an *independent* Gaussian
   (`touch-model` keys a `HashMap<KeyId, Mean>`), so a key only improves once
   *that specific key* has been tapped enough times (`observations(k) < 2` ⇒ the
   decoder falls back to the identity covariance, `input-decoder/src/lib.rs:155`).
   A user whose whole right hand lands low-and-right gets **no** benefit on a
   rarely-typed key until they have individually tapped it many times.

2. **No correction signal.** Training is committed-key, positive-only
   (`observe_tap` at `featherkey-core/src/learn.rs:187` folds the offset of
   whatever key was *committed*). A mis-decode reinforces the **wrong** key's
   mean. (This gap is analysed in §8 and **explicitly deferred** — the retype
   already supplies a positive sample, so a dedicated negative pipeline is not
   yet justified.)

This design closes gap **(1)**: a tiny neural net learns a **global coordinate
warp** — a smooth, position-dependent `(Δx, Δy)` shift applied to the incoming
touch *before* the existing decoder runs — that generalizes systematic tap bias
across keys, including keys the user has barely pressed. The per-key Gaussian
stays exactly as-is as the fine-grained layer underneath.

---

## 2. What already exists (CODEMAP consulted — do not rebuild)

Queried `CODEMAP.md` and read the sources. Relevant existing surface:

| Capability | Where | Decision |
|---|---|---|
| The tiny dependency-free MLP substrate (`forward`, `train_step`, `from_linear`, `with_weights`, versioned `to_bytes`/`from_bytes`) | `featherkey-nn` (`core/crates/nn`) | **Reuse.** The warp is built on `Mlp`, exactly as `neural-ranker` and `autocorrect-gate` are. No new NN code. |
| Zero-output-but-trainable "centred signed feature-pair" prior | `autocorrect-gate::from_prior` (`core/crates/autocorrect-gate/src/lib.rs:105`) | **Mirror.** The warp's cold-start prior is the same construction, per output axis. |
| Self-persisting model (`persist`/`load`, absent/corrupt ⇒ prior) under a `Namespace` via `SecureStore` | `neural-ranker` / `autocorrect-gate` `persist.rs` | **Mirror.** New `Namespace::TapWarpModel`. |
| The live decode seam | `InputDecoder` trait (`core/crates/input-decoder/src/lib.rs:42`); façade `FeatherKeyCore::decode` (`featherkey-core/src/lib.rs:309`) applies `self.touch_model` per keystroke | **Integrate here.** Verified on the live hot path (Kotlin sends raw `(x,y)`; Rust picks the key). |
| Per-key learned mean/covariance + `observe` | `touch-model` (`core/crates/touch-model/src/lib.rs`) | **Keep unchanged.** The warp composes with it; it is the fine layer. Provides `offset(k)` = `mean_k`, used as the warp's training target (§6). |
| The positive training call site (gated) | `FeatherKeyCore::observe_tap` (`learn.rs:187`), sensitivity-gated at `:194` | **Extend.** One added warp step after the existing `touch_model.observe`. |
| Sensitive-field gate + whole-store wipe | `SensitivityPolicy::should_suppress`; `SettingsActivity.clearLearnedData()` deletes the whole redb | **Reuse.** No new gate, no new purge code. |

**New code is exactly one crate** (`neural-tap`) plus thin wiring in
`featherkey-core`. Nothing here duplicates an existing responsibility.

---

## 3. The warp model

New crate **`core/crates/neural-tap/`**, package `featherkey-neural-tap`, **domain
layer**, depends on `featherkey-nn` + `featherkey-contracts` (mirrors
`neural-ranker`). One public type:

```rust
pub struct TapWarp {
    dx: Mlp,   // normalized (x,y) -> Δx  (pixels)
    dy: Mlp,   // normalized (x,y) -> Δy  (pixels)
}
```

- **Inputs (2):** the tap position normalized to `[-1, 1]` by the active layout's
  logical bounds — `INPUTS = 2`. Position, *not* key identity, is the entire
  input: that is what makes the warp generalize across keys by construction.
- **Two scalar MLPs, one per axis.** `Mlp::forward` returns a single `f32`; a warp
  needs two outputs, so `TapWarp` holds two independent `Mlp`s. This needs **no
  change to the `nn` substrate** (KISS — do not generalize `Mlp` to multi-output
  before a second caller needs it).
- **Cold-start prior:** each axis uses the `autocorrect-gate::from_prior`
  construction — two hidden units per input forming a centred signed reader
  (`FEATURE_CENTERS = [0.0, 0.0]`, since inputs are centred at the keyboard
  middle), output weights `±κ` that cancel to ~0 while every input weight keeps a
  gradient path. Cold-start output is `≈ (0, 0)` for every position.
- **Bounded output:** `warp()` clamps each axis to `±WARP_BOUND` (proposed **40
  logical px**, ~⅖ of a key width) so no single bad update can fling a tap across
  keys. Tunable in the build.

```rust
impl TapWarp {
    pub fn from_prior() -> Self;                         // ≈(0,0) everywhere, trainable
    pub fn warp(&self, nx: f32, ny: f32) -> (f32, f32);  // clamped ±WARP_BOUND
    pub fn reinforce(&mut self, nx: f32, ny: f32, tx: f32, ty: f32, lr: f32); // §6
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), StoreError>;
    pub fn load(store: &impl SecureStore) -> Result<Self, StoreError>;        // absent/corrupt ⇒ from_prior
}
```

`reinforce` is one squared-error SGD step per axis toward `(tx, ty)`, identical in
form to `AutocorrectGate::reinforce` (`d = 2·(forward − target)`, then
`train_step`).

---

## 4. Integration seam (decode)

`FeatherKeyCore` gains a `tap_warp: TapWarp` field (constructed `from_prior` in
`new()`, mirroring `neural_ranker`/`autocorrect_gate`). `decode` applies the warp
to the touch **before** the existing decoder call:

```rust
pub fn decode(&mut self, x: f32, y: f32) -> Result<DecodeResult, FeatherKeyError> {
    let (nx, ny) = self.layout.normalize(x, y);          // to [-1,1] by layout bounds
    let (wdx, wdy) = self.tap_warp.warp(nx, ny);          // clamped ±WARP_BOUND
    let touch = TouchPoint::new(x + wdx, y + wdy);
    let candidates = self.decoder.decode(touch, &self.layout, &self.touch_model)?;
    // …unchanged: push TapDistribution, build DecodeResult…
}
```

The `InputDecoder` trait and `NearestKeyDecoder` are **unchanged** — the warp is
a pre-transform of the touch, not a new decoder. (Alternative: a `WarpedDecoder`
wrapper implementing `InputDecoder`; rejected in §9 as more machinery for no gain,
since the warp model lives in the core façade next to its trainer.)

`Layout::normalize` (small helper on `layout-engine`) maps a surface-local pixel
to `[-1,1]` using the layout's logical bounds (computed from `keys()` — there is
no exposed bounds accessor today, confirmed against CODEMAP); it plus
`Layout::center_of(char)` are the two new methods on an existing crate.

**The warp flows into the suggestion path — consistently.** `decode` stores the
warped candidates in `self.taps` (`lib.rs:317`), which feed `spatial_hypotheses`
→ the neural ranker's `spatial` feature (`rank.rs`). This is intended: the warped
decode is the truer decode. At cold start the warp is identity (§5), so the
spatial feature — and therefore suggestion ranking — is **unchanged** until the
warp has learned; the regression guard covers it.

---

## 5. Cold-start behaviour — corrected claim

**The warp is *behaviourally* identical to today's decode at cold start, and
regression-guarded — but NOT byte-for-byte identical.** (This corrects an earlier
overstatement.)

- The centred signed-pair prior cancels to `≈0`, not exact `0` (f32 rounding
  leaves `|Δ| ~1e-6 px`). A sub-micron coordinate shift **cannot** change
  nearest-key ranking except on an exact-tie knife-edge (measure zero on a
  continuous surface), so the committed key and the ranked candidate order are
  unchanged for every realistic tap.
- **Regression guard (a required test):** for a spread of touch positions, the
  decode result (best key + ranked order) with a cold-start `TapWarp` **equals**
  the decode result with no warp. Confidences may differ below an epsilon far
  smaller than any inter-key gap; the *order and winner* are identical.

This is a weaker-but-honest guarantee than the re-ranker's byte-identical prior,
and it is the correct one for a decoder whose output is a discrete argmin.

---

## 6. Training — Increment 1 (positive signal, zero new FFI)

Hook the existing gated `observe_tap` (`learn.rs:187`). After the current
`touch_model.observe`, take one warp step:

```rust
pub fn observe_tap(&mut self, key: char, dx: f32, dy: f32, field: &dyn SensitiveContextSource)
    -> Result<(), FeatherKeyError>
{
    if self.sensitivity.should_suppress(field) { return Ok(()); }  // BR-26, unchanged
    let (mx, my) = self.touch_model.offset(KeyId(key));            // mean_k BEFORE the fold
    self.touch_model.observe(KeyId(key), dx, dy)?;                 // unchanged
    if let Some(center) = self.layout.center_of(key) {            // p = center + (dx,dy)
        let (nx, ny) = self.layout.normalize(center.x + dx, center.y + dy);
        // Target derivation (§ below): warp should move the tap so decode lands
        // it on center(k)+mean_k. t = mean_k − (dx,dy).
        self.tap_warp.reinforce(nx, ny, mx - dx, my - dy, WARP_LR);
    }
    Ok(())
}
```

**Why `t = mean_k − (dx, dy)` (the no-double-correction target).** Decode places
key `k`'s effective centre at `center(k) + mean_k` (verified: `touch-model`
`observe(e,+60,0)×8` shifts `e`'s effective centre to x=310). With the warp the
decoder sees `p' = p + w(p)`. For the committed key to win we want
`p' ≈ center(k) + mean_k`, i.e. `w(p) ≈ center(k) + mean_k − p = mean_k − (dx,dy)`
since `p = center(k) + (dx,dy)`.

- **Unobserved key** (`mean_k = 0`): `t = −(dx,dy)`; the warp learns to cancel the
  systematic offset and, being a smooth function of position, **generalizes it to
  neighbouring keys** the user has not individually tapped — the few-shot win.
- **Converged key** (`mean_k = E[(dx,dy)]`): `E[t] = 0`; the warp's target has
  zero mean, so it learns `≈0` there and **never double-corrects** with the
  per-key Gaussian.

Cost: two tiny fixed-size forward/backward passes per tap — O(1), allocation-free,
no `sqrt`, no clone (BR-46). No new FFI: `observe_tap` already carries `key`,
`dx`, `dy`, `field`; `center_of`/`normalize` read the in-core layout.

`WARP_LR` (proposed ~0.01, tuned in the build) is small so the warp tracks the
slow systematic field, not per-tap noise. Smallness + the `±WARP_BOUND` clamp
also serve **BR-6** (consistent/decisive accuracy): the warp moves in gentle,
bounded increments and cannot oscillate or jerk the decode from one tap to the
next — a stability property the test plan asserts directly.

**Convergence, stated honestly.** Constant-`WARP_LR` SGD toward a zero-mean noisy
target does **not** settle on an exact point; it converges to a small, bounded
neighbourhood of 0 (a stationary jitter whose size ∝ `WARP_LR`). The low-capacity
*smooth* model keeps that jitter far smaller than a per-position estimator would,
because it fits only the regional average of the targets — the per-tap noise
cancels across the region. So "the warp learns ≈0 at a converged key" means a
small bounded value, not literally zero.

**Safety argument (why the warp cannot meaningfully regress accuracy).** Where a
smooth systematic field exists, the warp captures it (the win). Where it does
*not* — genuinely per-key-idiosyncratic bias — the targets of nearby positions
conflict and average to ≈0, so the warp stays **inert** and the per-key Gaussian
does the work unchanged. In expectation the warp therefore cannot pull accuracy
below the per-key baseline; the only exposure is bounded transient over/under-shoot
during early learning, capped hard by `±WARP_BOUND` and kept gentle by a small
`WARP_LR`. Cold-start identity (§5) covers the very start. This is the property the
BR-6 stability test and the generalization test jointly defend.

**Known characteristic (not a defect):** because the field is smooth, a
well-observed key sitting among under-observed neighbours can carry a small shift
"borrowed" from the regional field even though its own per-key mean has converged
— bounded, intended (the field is real and shared), and distinct from
double-correction, which §6's zero-mean target rules out at the joint fixed point.

**A tap that can't be placed skips warp training.** `center_of(key)` returns
`None` when the committed char is not on the *active* layout page (e.g. a page
switched between decode and observe, or a non-letter key whose centre isn't
resolved); the `if let Some(center)` guard then skips the warp step for that tap
(the per-key `touch_model.observe` still runs). Safe degradation — some taps
simply don't train the warp — not a correctness risk.

---

## 7. Persistence, security, purge

- New `Namespace::TapWarpModel` (`= "tap_warp_model"`) in `contracts` — the warp
  is its sole writer (ADR-14 style).
- `TapWarp` self-persists via `SecureStore` (encrypted redb, AES-256-GCM) with a
  versioned blob, exactly like `NeuralRanker`/`AutocorrectGate`. Absent or corrupt
  blob ⇒ `from_prior`, never `Err`.
- Wired into `FeatherKeyCore` `persist()`/`restore()` (`learn.rs`) and initialised
  `from_prior` in `new()`.
- **Purge is free:** `SettingsActivity.clearLearnedData()` already deletes the
  whole `featherkey.redb`; the warp namespace goes with it and re-inits to the
  prior. No new purge code, no `SecureStore::delete`.
- **BR-26 absolute:** training is behind the existing `should_suppress` gate — the
  warp never learns from a password/OTP/sensitive field. The warp *model* is read
  on every decode (all fields) but that is a pure read of already-learned,
  consent-gated weights, exactly as `touch_model` is read today.

---

## 8. Increment 2 (correction signal) — explicitly DEFERRED

The roadmap's original framing included a negative/correction signal ("learn from
mis-taps the user backspaces"). It is **deferred**, for evidence-based reasons the
audit surfaced:

- **The retype already trains the warp positively.** When a user deletes a
  mis-tapped letter and types the correct key, that retype is an ordinary tap that
  fires `observe_tap` on the *correct* key at nearly the same coordinate — so the
  warp already gets a positive sample where it matters. The marginal value of an
  explicit negative signal is unproven. (Mirrors the re-ranker's finding that
  "revert-after-autocorrect is partly redundant.")
- **The data path does not exist and would not be free.** Verified:
  `self.taps` stores `(char, confidence)` distributions, **not** raw coordinates
  (`lib.rs:317`), and `observe_delete_retype(word, field)` (`learn.rs:110`)
  carries a *word*, not the mis-tapped char, the intended key, or a coordinate. A
  correction-warp would therefore need **new retained state (raw coordinates) or a
  new/extended FFI signal** — and raw-coordinate retention is more privacy-sensitive
  than the current char/confidence tap history (which is deliberately transient and
  unpersisted for BR-26).

**Deferred, with a trigger:** revisit only if on-device acceptance of Increment 1
shows residual mis-decodes that the positive path demonstrably fails to fix. If
built, it must (a) prove it beats the redundant-positive baseline and (b) keep any
coordinate retention transient + sensitivity-gated. Recorded here rather than
built early (KISS/YAGNI, per this repo's "Deferred" convention).

---

## 9. Alternatives rejected

- **Per-key score residual** (an MLP adjustment per key per keystroke): O(keys)
  forward passes on the hot path, cold-start identity must hold per key, and it
  overlaps the per-key Gaussian's job. Rejected for cost + duplication.
- **Full neural replacement of the decoder** (`P(key | x,y)` subsuming
  `touch-model`): discards a shipped, well-tested model, largest build, hardest
  cold-start proof. Rejected (risk/scope).
- **A `WarpedDecoder` implementing `InputDecoder`**: the warp *model* is stateful
  and trained in the core façade next to `observe_tap`; threading it through the
  stateless `decode(touch, layout, model)` trait would either widen the trait or
  smuggle state. Applying the warp to the touch in the façade is simpler and keeps
  the trait pure. Rejected as machinery for no gain.
- **Extending `Mlp` to multi-output**: not needed — two `Mlp`s cost the same and
  touch no shared code. Deferred until a second multi-output caller exists.
- **A heavier/nonlinear model or an added dependency**: fails `deny.toml` (zero new
  deps), coverage/fitness caps, and the battery budget. The systematic tap field
  is near-affine at keyboard scale; the tiny MLP already exceeds what's needed.

---

## 10. BDD scenarios (Gherkin, `@BR-7`)

`core/features/neural-tap-decoder.feature`:

```gherkin
@BR-7
Feature: The tap decoder learns the user's systematic aim and generalizes it

  Scenario: A cold-start warp does not change decoding
    Given a fresh tap-warp model
    When the user taps at a spread of positions across the keyboard
    Then every decoded key and candidate order is identical to decoding with no warp

  Scenario: A systematic hand-bias is learned and generalizes to an un-tapped key
    Given a user who consistently taps several keys low-and-right of their centres
    When the tap-warp model has learned from those taps
    And the user taps a DIFFERENT key they have never tapped, also low-and-right
    Then the decoder resolves the intended un-tapped key
    And an unbiased decoder would have mis-resolved it to a neighbour

  Scenario: The warp does not double-correct a well-learned key
    Given a key whose per-key mean offset has already converged
    When the user taps it on its learned centre
    Then the warp contributes ≈ zero additional shift

  Scenario: Learning is suppressed in a sensitive field
    Given a password field
    When the user taps keys off-centre
    Then the tap-warp model is not updated
```

---

## 11. Test plan (TDD, written first)

Unit (`neural-tap` crate):
- `from_prior` output is `≈(0,0)` (|Δ| < 0.05 px) across a grid of positions.
- `warp` is clamped to `±WARP_BOUND` even for extreme inputs.
- `reinforce` moves each axis toward its target; a systematic-offset stream drives
  the warp toward `−offset`; a zero-mean target stream leaves it `≈0`.
- persist → load round-trips; absent blob ⇒ `from_prior`; corrupt blob ⇒
  `from_prior` (never `Err`).

Integration (`featherkey-core`):
- **Cold-start decode identity** (§5 regression guard): warp-on vs warp-off decode
  agree on winner + order across positions.
- **Few-shot generalization:** feed off-centre taps on keys A/B, then a tap on
  never-tapped key C at the same bias resolves C (unbiased decoder resolves a
  neighbour). Realizes the `@BR-7` scenario.
- **No double-correction:** a converged per-key mean + on-centre tap ⇒ warp shift
  `≈0`.
- **Stability (BR-6):** across a stream of noisy taps at one position the
  per-tap change in the warp output stays within a small bound — no oscillation.
- **Spatial feature unchanged at cold start:** the neural-ranker `spatial` input
  for a given tap set is identical warp-on vs warp-off before any training.
- **Sensitivity gate:** taps in a sensitive field leave the warp unchanged.

Fitness/DoD: coverage ≥98% line, files ≤500 / fns ≤60, `cargo-deny` zero new deps,
codemap regenerated, `bdd_check` maps `@BR-7`, `ci-local.sh` green.

---

## 12. Open items to close in the plan

1. `Layout::normalize` / `Layout::center_of` exact signatures and bounds source
   (does `layout-engine` already expose logical bounds, or compute from keys?).
2. Final constants: `WARP_BOUND`, `WARP_LR`, prior `PRIOR_SCALE`/`κ`/margin —
   fixture-tuned in the build like the gate's were.
3. Confirm `neural-ranker` vs `autocorrect-gate` `persist.rs` is the closer
   template for the versioned two-`Mlp` blob (two models in one blob).
4. Whether the warp read belongs in the façade `decode` or a small
   `warp` helper module in `featherkey-core` (file-size fitness).
5. Per-page normalization: `normalize` uses the *active* page's bounds, so a warp
   learned on the alpha page applies in normalized space to the numeric/symbol
   pages (different aspect ratios). Low-stakes (numeric taps are rarer, keys
   larger) — decide in the plan whether to normalize by a single canonical
   (alpha) bound or accept per-page normalization.

---

## Audit log

### Pass 1 — ✅ Complete and verified (design phase)
**Audited against:** BR-7 (primary), BR-5/6/46 (supporting); the CODEMAP "does it
already exist?" contract (§2); internal consistency; the repo non-negotiables.

**Evidence:**
- **Requirements mapped.** BR-7 text read verbatim (`BUSINESS_REQUIREMENTS.md:195`
  — "learn the user's typing style … become measurably more accurate"); the
  few-shot generalization scenario (§10) is its realization. BR-5/6/46 supporting,
  each tied to a concrete design property (bounded/stable warp; O(1) two-pass hot
  path).
- **CODEMAP-exists claims verified first-hand**, not from summary: `featherkey-nn`
  `Mlp::{with_weights,from_linear,forward,train_step}` (read `nn`/gate source);
  `autocorrect-gate::from_prior` signed-pair prior (`autocorrect-gate/src/lib.rs:105`);
  `InputDecoder` seam + `FeatherKeyCore::decode` applying `touch_model`
  (`featherkey-core/src/lib.rs:309`); `observe_tap` gated at `learn.rs:194`;
  `touch_model.offset`/`observe` (`touch-model/src/lib.rs:114,133`); `Key::center`
  exists but **no** `Layout` bounds accessor (so `normalize`/`center_of` correctly
  flagged NEW, not falsely claimed existing); `Namespace` enum location
  (`contracts/src/lib.rs:27`).
- **Load-bearing math re-derived.** `t = mean_k − (dx,dy)` checked for sign and for
  a transient double-correction: the target references the *live* `mean_k`, so
  warp + per-key correction always sum to land the tap on `center(k)+mean_k`, and
  the warp decays to ≈0 at the joint fixed point where the mean has converged. No
  double-correction. Verified against the `touch-model` test that fixes
  `eff_center(e)=310` after `observe(e,+60,0)×8`.
- **Overclaims from the design-shape retracted and corrected in the doc:**
  cold-start is *behaviourally identical, regression-guarded* — **not**
  byte-identical (§5); Increment 2 is **not** zero-FFI/reconstructable and is
  **deferred** with a documented data-path gap and redundancy rationale (§8).

**Gaps found this pass → changed:** added the warp→`self.taps`/spatial-feature
consistency note (§4); added the smooth-field "borrowed shift" characteristic +
BR-6 stability property (§6); added stability + spatial-unchanged tests (§11);
added the per-page normalization open item (§12.5).

**Not verified (correctly, no code exists yet):** no tests/build run — TDD/BDD and
`ci-local` are the plan/build phases. The four §12 items (helper signatures, final
constants, blob template, file placement) are legitimate plan-phase details, not
design gaps.

**Verdict:** ✅ design complete, requirements-mapped, CODEMAP-checked, internally
consistent, non-negotiables honoured (one crate + thin wiring; zero new deps
intended; O(1) hot path; BR-26 gate reused; purge free). Ready for user review →
plan.

### Pass 2 — ✅ Complete and verified (re-audit; found + fixed real imprecisions)
Re-ran the gate rather than rubber-stamping Pass 1. Findings, each changed:
- **Substrate now read first-hand** (`core/crates/nn/src/lib.rs`) — Pass 1 had only
  read the two *callers* (gate/ranker), so its "read nn source" wording was ahead
  of the evidence. Confirmed: `Mlp::forward` returns a **single scalar** (`:49`,
  doc `:1`), so the two-MLP-per-axis structure is *necessary*, not a style choice;
  `with_weights(w1,b1,w2,b2,inputs,hidden)` signature matches §3 exactly; `nn`'s
  own test builds a **2-input/2-hidden** MLP (`:82-91`) — precisely the warp axis's
  envelope. The design's core substrate assumption is now evidenced, not inferred.
- **Convergence overstated.** "Warp learns ≈0" implied a fixed point; constant-lr
  SGD toward a zero-mean noisy target gives a small *stationary jitter*. §6 now
  states this honestly and adds the **safety argument** (the warp is inert where no
  smooth field exists ⇒ cannot regress accuracy below the per-key baseline in
  expectation; transients bounded by `±WARP_BOUND`).
- **`center_of` can miss** (char off the active page / non-letter) — §6 now
  documents the `Some(center)` guard as safe degradation (that tap skips warp
  training; per-key observe still runs).

**Not verified (still correctly):** no code exists, so no tests/build were run —
that is the plan/build phase. `train_step`/`from_linear`/`to_bytes` live in `nn`
submodules (`train`/`prior`/`codec`); confirmed present via CODEMAP + their use in
gate/ranker, implementations not line-read (not needed to ground the design).

**Verdict:** ✅ still complete; two honest imprecisions corrected and the one
unread dependency now read. No load-bearing claim left on inference. Ready for user
review → plan.
