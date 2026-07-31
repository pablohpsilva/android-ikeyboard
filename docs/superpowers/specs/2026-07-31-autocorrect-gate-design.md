# Personalized Autocorrect Gate — Design

**Date:** 2026-07-31
**Status:** Design (approved shape; awaiting spec review → plan)
**Slice:** App #2 of the 4-app neural roadmap (re-ranker ✅ → **autocorrect gate** → tap decoder → next-word LM).
**Follows:** the neural re-ranker (`docs/superpowers/specs/2026-07-31-tiny-neural-reranker-design.md`) — same `featherkey-nn` substrate, same gating/persistence pattern.

---

## 1. Problem

On-device autocorrect applies on a **fixed, universal policy**: `NoClobberCorrector`
picks the best fuzzy correction and applies it whenever the typed token is not
"intended" and a candidate exists (`autocorrect/src/lib.rs:132-160`). There is **no
confidence threshold and no personalization** — every permissible correction is applied,
identically for every user. Users who are annoyed by a specific over-correction can only
revert it one word at a time (the exact-word whitelist, §3.3).

Goal: a tiny per-user neural gate that **learns when to trust a correction**, reducing
corrections/backspaces over time (BRD success metric "Accuracy: reduction in user
corrections/backspaces"), while the **BR-12 no-clobber guarantee stays absolute**.

## 2. Requirements closed / advanced

| BR | How this design serves it |
|----|---------------------------|
| **BR-12** | No-clobber stays absolute — the veto runs *before* the gate; the gate can only re-weight decisions the policy already permits. |
| **BR-15** | Advances "autocorrect aggressiveness adjustable" — not a user slider (that is a separate future slice), but a *learned* per-user aggressiveness. |
| **BR-45** | Alternatives path is untouched; still offered. |
| **BR-22 / BR-26** | Learning gated by consent + field sensitivity (never learns in a sensitive field). |
| **BR-46** | Runs at a word boundary, off the sub-ms decode hot path; one tiny MLP forward added. |
| **BR-13** | Fully on-device; no new dependency (reuses `featherkey-nn`). |

**Non-goal:** a user-facing aggressiveness slider (BR-15's explicit control) — deferred to
its own slice. This slice is the *learned* gate only.

## 3. Existing code (CODEMAP consult, CLAUDE.md §2)

**Reused, not rebuilt:**
- `featherkey-autocorrect` (`NoClobberCorrector::correct`) — decides the correction + the
  no-clobber veto (BR-12). **Extended** here (additive API, §5.1), not duplicated.
- `featherkey-nn` — the tiny MLP substrate (forward, `from_linear` prior, `train_step`,
  versioned serde). Reused as-is; `train_step(x, d_output, lr)` already supports the
  pointwise scalar-target training this gate needs (verified: `nn/src/train.rs:19`).
- `featherkey-candidate-ranker::score` — the per-candidate score already used inside
  `score_with_sticky` (`autocorrect/src/rank.rs:101`); the gate's `winner_confidence` is the
  winning candidate's existing score, surfaced — not recomputed.
- Kotlin `CorrectionDetector` — already emits `onAutocorrect` (arm), `onBackspaceUndo`
  (revert), `onDeleteRetype`, `onSuggestionPicked`. The gate's signals wire to these.
- The exact-word revert whitelist: `onBackspaceUndo → bridge.addToDictionary(word)`
  (`FeatherKeyImeService.kt:844-846`) → `Personalization::is_known` veto. **Stays.** The
  gate does **not** duplicate it (§4: the gate carries no per-word memory).

**New:**
- Crate **`featherkey-autocorrect-gate`** (domain layer) — the residual model + its persistence.
- `Namespace::AutocorrectGate` in `contracts`.
- One FFI method + its Kotlin wiring (§5.3).

## 4. Architecture

A new domain crate `featherkey-autocorrect-gate` (SRP: `autocorrect` *assesses* a
correction; the gate *learns whether to trust it* — two reasons to change, mirroring how
`neural-ranker` is separate from `candidate-ranker`). It reuses `featherkey-nn` and owns:

- `GateFeatures` — the fixed feature vector (§4.1), the single slot-order contract.
- `AutocorrectGate` — holds the MLP; `residual(&GateFeatures) -> f64` (bounded),
  `reinforce(&GateFeatures, target)`, `from_prior()`, `persist(&store)` / `load(&store)`.

### 4.1 The decision mechanism

Today's decision has **no confidence dimension** (`applied = winner != typed_word`,
`autocorrect/src/lib.rs:149`). This slice introduces one, using the score that
`score_with_sticky` **already computes** — no invented baseline:

1. **Winner confidence** = `scored[0].1`, the winning candidate's momentum-weighted score
   (incl. the sticky bonus), already produced by `rank::score_with_sticky`
   (`autocorrect/src/rank.rs:97-109`). Higher = a more plausible correction.
2. **Base floor `T`** (a mild constant): the base policy becomes "apply iff
   `winner_confidence ≥ T`", slightly more conservative than today's unconditional apply
   (the approved cold-start behaviour change). `T` is mild — only the weakest-scoring
   corrections drop.
3. **Gate residual:** `applied = (winner_confidence + residual(features)) ≥ T`, where
   `residual` is the MLP output **clamped to ±B**, so the gate only nudges the threshold —
   never enough to overturn a no-clobber veto (applied *before* this, so the gate is not
   even consulted for vetoed words).

**Cold-start:** the prior makes `residual ≈ 0`, so `applied ⇔ winner_confidence ≥ T` =
base+floor. Not identical to today (floor added), but a defined, mild, tested shift.
`winner_confidence` is passed out of `autocorrect` as a plain `f64` — no new scoring code,
just surfacing a value the corrector already has.

### 4.2 Feature vector (slot order = the contract)

Structural signals only — **no exact-word counts** (the whitelist owns exact words; the
gate owns *generalization*): `[edit_distance, winner_confidence, correction_dict_rank_norm,
typed_len_norm, momentum_weight]`. Final slot order pinned in the plan; a prior-coeffs
constant reproduces `residual ≈ 0` at cold start, pinned by a
`prior_coeffs_are_near_zero`-style test (re-ranker precedent).

## 5. Learning

### 5.1 Signals (pointwise; gated `learningEnabled && !field.isSensitive()`)

| Event | Meaning | Training target |
|-------|---------|-----------------|
| **reverted** — `onBackspaceUndo` after an applied autocorrect | "wrong to apply" | residual **down** (below `T` for these features) |
| **applied & kept** — survives to the next boundary without revert | "fine to apply" | **mild up** (prevents over-suppression) |
| **suppressed → reached** — a correction the floor *withheld*, then the user manually lands on that exact word (delete-retype / strip-pick) | "should have applied" | **strong up** (the counterfactual apply-sooner signal) |

`train_step` runs one SGD step per observed outcome. Core caches, per applied/suppressed
decision, the `GateFeatures` that produced it (bounded to the last decision) so the outcome
trains against exactly what decided it.

### 5.2 The counterfactual (suppressed → reached)

A suppressed correction is never shown, so the only way to learn "should have applied it"
is to notice the user *manually* arriving at the exact word the gate withheld. Core caches
the last **withheld** `(typed_word → withheld_correction, features)`; the shell, on a
delete-retype or strip-pick, reports the word the user ended on; if it equals the withheld
correction, that is the strong-up signal. **Noisy** (the user may reach a *different* word)
→ low weight + explicit false-positive tests (must not fire when reached ≠ withheld).

### 5.3 Wiring

- `autocorrect`: `correct()` (or a new `assess()`) returns the `Correction` **plus** the
  `margin` and, when it withholds, the `withheld` correction — additive, no behaviour change
  to callers that ignore them.
- `core/correct.rs`: applies the gate after the veto, caches features + withheld word, adds
  gated `observe_autocorrect_outcome(kind)` and persist/restore of the gate; `lib.rs` inits
  `from_prior`.
- FFI: one new `observe_autocorrect_outcome{reverted|kept|reached}` method (no new types
  beyond an enum) — no change to the strip/rank FFI surface.
- Kotlin `FeatherKeyImeService` / `CorrectionDetector`: emit the three outcomes; add the
  "reached the withheld word" detection against the core's cached withheld correction.

## 6. Persistence

`Namespace::AutocorrectGate` ("autocorrect_gate"); the gate self-persists its MLP weights
in the redb `SecureStore` (AES-256-GCM), mirroring `NeuralRanker`: `load` returns
`from_prior` when the blob is absent **or** corrupt/wrong-shape (never `Err` on the hot
path). Purged for free by the existing whole-store wipe (`clearLearnedData`); no new purge
code, no `SecureStore::delete`.

## 7. Invariants

- **BR-12 absolute:** veto → `applied = false`, gate not consulted; no trained state can
  clobber an intended/known/device-known word. Pinned by the existing no-clobber tests +
  a new "no trained gate clobbers a known word" test.
- **Bounded residual:** clamped to ±B → the gate only nudges the threshold.
- **Cold-start = base+floor:** pinned test (the re-baselined `correct.rs` expectations).
- **Sensitivity/consent:** all three signals short-circuit in a sensitive field / with
  consent off (BR-26/22), pinned by a sensitive-field control test.
- **Speed:** one MLP forward at a word boundary; off the decode hot path (BR-46).

## 8. Testing (TDD + BDD, CLAUDE.md §3)

Cold-start = base+floor (re-baseline `correct.rs`); each signal moves the residual the
right way; no-clobber inviolate under any trained gate; sensitive/consent gating;
persist/restore + corrupt→prior; counterfactual matcher fires only on withheld == reached.
A `@BR-12`/gate BDD scenario in `core/features/`. Coverage ≥ 98%, fitness, cargo-deny
(zero new deps), all via `ci-local`.

## 9. Open risks (resolve in plan/build)

1. **Prior must output ≈0 residual yet stay trainable** — `from_linear` with nonzero
   weights (the re-ranker's `DEAD_UNIT_WEIGHT` handling); verify backprop reaches the
   features after step 1.
2. **Choosing `T`** — mild enough that only clearly-weak corrections drop at cold start;
   `winner_confidence`'s scale is set by `candidate_ranker::score` + `CORE_FUZZY_PRIOR`, so
   `T` is pinned against that scale and against the existing `correct.rs` fixtures (some
   flip applied→not; that re-baseline is the deliberate, approved shift — enumerate which).
3. **Counterfactual noise** — low weight; tests that it never fires on reached ≠ withheld,
   and never in a sensitive field.

## 10. Alternatives rejected

- **Non-neural per-(typed→corrected) counts** (extend `corrections`): per-exact-pair only —
  barely beats the whitelist, and not the neural slice the roadmap calls for.
- **Fold the gate into `autocorrect`:** SRP violation — "decide the correction" and "learn
  whether to trust it" are separate reasons to change (re-ranker precedent kept them apart).
- **User aggressiveness slider (BR-15):** its own future slice, not this one.
- **Suppress-only gate:** simplest and cold-start-identical, but forecloses the apply-sooner
  direction the product owner chose; superseded by the base-floor + both-ways decision.

## Audit log

### Pass 1 — 🚧 Incomplete (design-gate, caught before spec was written)
Gaps: the first design draft assumed `applied = (policy_margin + residual) > 0` — a margin
that **does not exist**. Verified against `autocorrect/src/lib.rs:149`: the real decision is
`applied = (winner != typed_word)` with no confidence threshold (autocorrect applies
maximally). Consequence: "both directions" + "cold-start identical" were infeasible together.
Also unverified: the positive/apply-sooner signal is counterfactual; the "kept" signal is
not first-class in the shell.
Changed: mechanism reworked to an explicit floor `T` + bounded residual (product owner
approved the cold-start base shift); three learning signals defined incl. the counterfactual
(product owner chose full counterfactual now); `nn` pointwise training confirmed
(`train.rs:19`); open risks enumerated (§9). This document is the reworked design.

### Pass 2 — ✅ Complete and verified (design-gate)
Re-audited the reworked design + a self-review simplification. Evidence:
- **Mechanism grounded, no invented quantities.** `winner_confidence = scored[0].1` is a
  value `rank::score_with_sticky` already produces (`autocorrect/src/rank.rs:97-109`);
  surfacing it is additive. The self-review removed the earlier "score the typed token as a
  pseudo-candidate" baseline (unnecessary + unverified) — Pass-1 risk #4 is now designed out,
  not deferred.
- **Every mechanism cites read code:** veto-before-gate (`lib.rs:132-160`), `applied` today
  (`lib.rs:149`), pointwise training (`train.rs:19`), the whitelist it must not duplicate
  (`FeatherKeyImeService.kt:844-846`), reused ports (`neural-ranker`, `nn`, `candidate-ranker`).
- **Internally consistent:** cold-start = base+floor stated the same throughout; no-clobber
  absolute (gate not consulted on a veto); feature set carries no exact-word memory (DRY with
  the whitelist).
- **Remaining items are correctly plan/build-scoped, not design unknowns:** prior-≈0
  trainability (§9.1), `T` value (§9.2), counterfactual weight/false-positive tests (§9.3) —
  each is a constant/tuning decision the plan pins, not an architectural gap. The re-ranker
  design deferred its coeffs the same way.
Verdict: ✅ the design is complete, feasible, and grounded. Ready for spec review → plan.
