# Implementation Plan

**Project (working name):** FeatherKey — A Fast, Private, Modular Android Keyboard
**Document type:** Implementation Plan / Delivery Sequencing & Execution Protocol
**Version:** 0.1 (Draft)
**Date:** 2026-07-24
**Source-of-truth chain:** [`BUSINESS_REQUIREMENTS.md`](./BUSINESS_REQUIREMENTS.md) (BRD v0.7, **source of truth**) → [`SOFTWARE_ENGINEERING.md`](./SOFTWARE_ENGINEERING.md) (SEDD v0.7) → [`ARCHITECTURE.md`](./ARCHITECTURE.md) (ARCH v0.3) → **this plan**.

### Revision History

| Version | Date | Summary |
|---|---|---|
| 0.1 | 2026-07-24 | Initial plan: current state, execution protocol (subagents/workflows + `/r-u-sure` gates), pre-flight decisions, wave-based build order grounded in a module-dependency + BR-traceability analysis of BRD/SEDD/ARCH and the current code. |

> **Purpose & relationship to the other docs.** The BRD says *what/why*, the SEDD says *how* (technologies/modules), the ARCH says *how the modules are shaped* (SOLID, TDD/BDD, fitness functions). **This plan says *in what order* we build, *by what mechanism* (subagents + workflows), and *how each piece is proven correct* (`/r-u-sure` + regression against everything already built).** Where this plan and a higher doc conflict, the higher doc wins and this plan is corrected. This plan is a schedule of work and a governance protocol — not new requirements and not new architecture.

---

## Table of Contents

1. [How to read this plan](#1-how-to-read-this-plan)
2. [Current state (Wave 0 — done)](#2-current-state-wave-0--done)
3. [The execution protocol (the core of this plan)](#3-the-execution-protocol-the-core-of-this-plan)
4. [Pre-flight: decisions & enforcement debt to clear before Wave 1](#4-pre-flight-decisions--enforcement-debt-to-clear-before-wave-1)
5. [Build order — the waves](#5-build-order--the-waves)
6. [Per-increment work-item template](#6-per-increment-work-item-template)
7. [BR ownership: module-owned vs CI/process-owned](#7-br-ownership-module-owned-vs-ciprocess-owned)
8. [What cannot be verified in this sandbox](#8-what-cannot-be-verified-in-this-sandbox)
9. [Risks & mitigations](#9-risks--mitigations)
10. [How this plan stays consistent with the source docs](#10-how-this-plan-stays-consistent-with-the-source-docs)

---

## 1. How to read this plan

The unit of work is an **increment** — normally one Rust crate (or one Kotlin module), sometimes a decision or an enforcement gate. Increments are grouped into **waves**; every module in a wave has all of its dependencies satisfied by an earlier wave (the crate graph is a DAG — ARCH §5.3, SEDD §5.5 r4).

Three rules govern *every* increment, no exceptions:

- **R1 — Built by subagents/workflows.** No increment is hand-built ad hoc. Each is executed by the mechanism in §3: a Plan subagent scopes it, a Workflow drives Red→Green→Refactor (and fans out across independent crates), adversarial verifier subagents check it.
- **R2 — Gated by `/r-u-sure`, repeated until it passes.** An increment is not "done" on a vibe. It passes the audit in §3.3 with *evidence* (tests run, coverage measured, fitness run), re-run as many times as needed until every requirement is DONE-with-evidence.
- **R3 — Validated against everything already built.** Every increment ends with a full-workspace regression: all prior tests still green, fitness still green, coverage still ≥ 98%, and no existing public API broken unless this plan explicitly scheduled the break (see D-4).

If an increment can't meet R1–R3, it is not merged — it goes back to §3's loop.

---

## 2. Current state (Wave 0 — done)

Verified by execution, not assertion (last full run: 17 tests pass, 0 warnings, 100% line coverage, fitness green):

| Artifact | Status | Evidence |
|---|---|---|
| `crates/kernel` | Done | Value objects `TouchPoint`/`KeyId`/`Confidence`, `CoreError`; zero deps; 5 tests |
| `crates/layout-engine` | Done | `Key`/`Layout` + `qwerty_tracer_row()`; deps: kernel; 4 tests |
| `crates/input-decoder` | Done **(sans touch-model)** | `NearestKeyDecoder`; deps: kernel, layout-engine; 5 tests. **Trait is `decode(touch, layout)` — SEDD §5.4 requires a `model` arg; see D-4.** |
| Keystroke tracer bullet | Done | `input-decoder/tests/tracer_bullet.rs`, 3 tests, touch→char; closes the thin slice of BR-5/BR-6 |
| BDD spec | Done | `features/keystroke_decoding.feature`, tagged `@BR-5`/`@BR-6` |
| Fitness functions | Done | `tools/fitness/check.py`: file/fn size caps, core purity, acyclic DAG + kernel-purity; **proven to fail on violations** |
| CI | Done (Rust) | `.github/workflows/ci.yml`: fmt, clippy `-D warnings`, test, fitness, **98% line-coverage gate** (branch informational); `android-shell` job dormant until a Gradle build exists |
| Android shell | Scaffold only | `ime-service`/`keyboard-view`/`ffi-bridge` + UDL, **uncompiled** (no JDK/Gradle/SDK/NDK here) |

**Sandbox capability confirmed this session:** crates.io is reachable (`fst` fetched & compiled), so the core MVP crates below *are* buildable and testable here. This can change if the environment goes offline — see Risk R-5.

---

## 3. The execution protocol (the core of this plan)

Every increment flows through the same five stages. Stages 2–4 loop until stage 4 passes clean.

```
  ┌─ 1. SCOPE ──────────────────────────────────────────────────────────┐
  │  Plan subagent reads BRD/SEDD/ARCH for this module and emits its      │
  │  spec: port traits it implements, BR IDs it closes, the failing-test  │
  │  list (Red), and its Definition of Done. Grounded in the docs.        │
  └──────────────────────────────────────────────────────────────────────┘
                              │
  ┌─ 2. BUILD (Workflow) ─────▼──────────────────────────────────────────┐
  │  Workflow drives TDD: Red (write failing tests) → Green (implement)   │
  │  → Refactor. Independent crates in a wave fan out in parallel, each    │
  │  in a git worktree (isolation) to avoid file collisions.              │
  └──────────────────────────────────────────────────────────────────────┘
                              │
  ┌─ 3. VERIFY (adversarial) ─▼──────────────────────────────────────────┐
  │  Verifier subagents try to REFUTE the increment: missed edge cases,   │
  │  a hot-path that panics, an interface that drifts from SEDD §5.4, a    │
  │  BR only partially met. Diverse lenses (correctness / security /      │
  │  interface-fidelity). Findings feed back into stage 2.                │
  └──────────────────────────────────────────────────────────────────────┘
                              │
  ┌─ 4. /r-u-sure GATE ───────▼──────────────────────────────────────────┐
  │  Run the r-u-sure audit (§3.3). If any requirement is PARTIAL/NOT     │
  │  DONE, or any check was not actually executed, LOOP back to stage 2.  │
  │  Repeat as many times as needed. Only a clean, evidenced pass exits.  │
  └──────────────────────────────────────────────────────────────────────┘
                              │
  ┌─ 5. INTEGRATE & VALIDATE ─▼──────────────────────────────────────────┐
  │  Full-workspace regression (R3). Commit (NO AI-attribution trailer).  │
  │  Update the traceability row(s). Increment is now part of "prior      │
  │  work" that the next increment must not break.                        │
  └──────────────────────────────────────────────────────────────────────┘
```

### 3.1 Which mechanism for which shape of work

| Work shape | Mechanism |
|---|---|
| A single crate, TDD | `Workflow`: a pipeline of `red → green → refactor → verify` stages for that crate |
| A wave of independent crates (e.g. Wave 1) | `Workflow`: `pipeline(crates, build, verify)` — each crate flows independently, no barrier; parallel worktrees |
| Cross-crate integration / regression | `Workflow` barrier: build all, then one regression + dedup-findings stage |
| A design decision (a D-item) | `Agent` (Plan): produce options + a recommendation traced to the docs; user ratifies |
| A one-off audit | `/r-u-sure` inline, or a verifier `Agent` |

### 3.2 Definition of Done (every code increment must satisfy all)

1. **Tests pass** — `cargo test --workspace`, 0 failures, 0 warnings (`cargo build` clean; `clippy -D warnings` in CI).
2. **Coverage ≥ 98% line** on the new crate *and* the workspace (`cargo llvm-cov --fail-under-lines 98`). Branch coverage reported; production-code branches expected at 100%.
3. **Fitness green** — `tools/fitness/check.py` exit 0 (no god-files, core purity, acyclic DAG, and the inward Dependency Rule once E-1 lands).
4. **Interface fidelity** — the public API matches the SEDD §5.4 port/trait sketch, or a deviation is recorded and justified (as D-4 does for `input-decoder`).
5. **BDD** — at least one Gherkin scenario per closed BR, tagged with the BR ID, in `features/`.
6. **Traceability** — the SEDD §15 row(s) for the closed BR(s) reference the new module; this plan's wave table is ticked.
7. **No panics on the hot path** — `Result` at boundaries; `unwrap_used`/`expect_used`/`panic` lints clean (SEDD §5.5 r3).

### 3.3 The `/r-u-sure` gate (stage 4) — what it audits

Per the r-u-sure discipline: re-read the increment's spec, then produce the four sections with *evidence*, not reassurance:
1. **What was required** — the increment's BRs + DoD items + implicit edge cases.
2. **What was done, mapped to each** — DONE/PARTIAL/NOT DONE + the artifact proving it.
3. **Verification with evidence** — the actual `cargo test` / `llvm-cov` / fitness output. "I didn't run X" is stated, never implied.
4. **The extra mile** — the gaps a careful engineer still worries about; fixed or explicitly handed off.

Verdict must be ✅ *Complete and verified* to exit. ⚠ or 🚧 → loop back to stage 2.

### 3.4 Validation against prior work (R3, stage 5)

Before commit, the increment must prove it broke nothing:
- `cargo test --workspace` — every pre-existing test still green.
- `cargo llvm-cov --workspace --fail-under-lines 98` — coverage did not regress.
- `tools/fitness/check.py` — still exit 0.
- Public-API diff of touched-but-not-owned crates — empty, unless a break was scheduled (D-4).
- `git status` — no stray artifacts; `target/` ignored; commit has **no** AI-attribution trailer (standing user preference).

---

## 4. Pre-flight: decisions & enforcement debt to clear before Wave 1

The grounding analysis found ambiguities in the docs and gaps in enforcement that would make a naive build order *wrong*. These are cheap now and expensive later. **Clear these before opening Wave 1.**

### 4.1 Decisions to ratify (each is a Plan-subagent → user-ratify D-item)

| ID | Decision | Why it blocks | Proposed default (to ratify) |
|---|---|---|---|
| **D-1** | **Where do the port traits live** (`SecureStore`, `Predictor`, `AutoCorrect`, `SensitiveContextSource`, …)? | Domain crates must depend on a *port*, not on an adapter (ARCH §3.2). Until the port crate exists, `personalization`/`secure-store` can't be built without an illegal edge. | A new dependency-free **`contracts`** crate (peer of `kernel`) holding the port traits. Keeps `kernel` as pure data; keeps the DAG legal. |
| **D-2** | **`locale-manager` ↔ `dictionary` edge direction.** | Sets whether locale-manager is Wave 1 or Wave 2. | `locale-manager` *depends on* `dictionary` (it scores a token against each active language's lexicon) → locale-manager in Wave 2. |
| **D-3** | **Who writes the tap-distribution model** — `touch-model` or `personalization`? SEDD §5.5 r1 ("personalization is sole writer") vs §5.2/§15 (touch-model "incremental learning"). | Determines the `touch-model` ↔ `personalization` boundary and the "sole writer" invariant. | `touch-model` owns *only* the tap-geometry model (a distinct data domain); §5.5 r1 is scoped to *lexical/vocabulary* learning. Record the two-domain split in SEDD. |
| **D-4** | **`input-decoder` signature drift.** Shipped `decode(touch, layout)`; SEDD §5.4 requires `decode(touch, layout, model)`. | Wave 2 touch-model integration is a **breaking change**, not additive — must be planned, not stumbled into. | Introduce the `model` parameter when `touch-model` lands (Wave 2). Keep an unbiased default model so the tracer tests still pass. This is the *one* scheduled API break. |
| **D-5** | **Is `layout-engine` RTL/bidi in MVP?** SEDD §15 marks BR-53 "MVP\* only if an RTL language is in the launch set" (BRD §18, undecided). | Scopes `layout-engine` MVP work. | Defer bidi until the launch language set is fixed; keep the port shape RTL-ready. Revisit when BRD §18 resolves. |

### 4.2 Enforcement debt to close (each is a small increment, same protocol)

| ID | Gap | Fix |
|---|---|---|
| **E-1** | Fitness does **not** enforce the inward Dependency Rule (a domain crate could import an adapter and nothing would catch it). | Extend `tools/fitness/check.py` with a layer map (domain / port / adapter / composition) and assert no domain→adapter edge. Add a negative test proving it bites. |
| **E-2** | `sensitive-context` ordering (BR-26: password fields *structurally* cannot be learned) is prose-only; no test guarantees the gate runs before `personalization`/`prediction`. | An application-layer property test at the `featherkey-core` composition root. Scheduled with Wave 4 but the *contract* is written when `sensitive-context` lands (Wave 1). |
| **E-3** | CI stages promised by SEDD §13.1 are missing: supply-chain (`cargo-deny`/`cargo-audit`, BR-65), permission/network guard (BR-20/BR-27), reproducibility (BR-24). Several **Must** privacy/security BRs currently have *no active guardrail*. | Add these as CI jobs. `cargo-deny` + `cargo-audit` first (cheap, high value). These are process-owned BRs (see §7) and must not wait for a module. |
| **E-4** | The **`featherkey-core`** composition façade was absent from the SEDD §5.2 registry though named in §3.6/ARCH; the `contracts` port crate (ADR-12) was also unregistered. | **Done (SEDD v0.7):** added `contracts` + `featherkey-core` to §5.2. `featherkey-core` keeps the UniFFI surface (no separate `featherkey-ffi` crate — consistent with §3.6/ARCH); it is Wave 4, `contracts` is Wave 0.5. |

> These pre-flight items are themselves increments: each goes through the §3 protocol (a decision D-item via a Plan subagent + user ratification; an enforcement E-item via the full build/verify/`/r-u-sure` loop).

---

## 5. Build order — the waves

Grounded in the dependency analysis. Every module's deps are built in an earlier wave. **Sandbox-verifiable** = testable here with no Android toolchain. BRs listed are what the wave *closes* or *advances*.

### Wave 0 — Done
`kernel`, `layout-engine`, `input-decoder` (tracer). BR-5, BR-6 (thin slice). ✔

### Wave 0.5 — Pre-flight ✅ **Done**
D-1…D-5 **ratified** (ADR-12–16, SEDD v0.7). Landed: the `contracts` port crate (D-1); **E-1** inward-dependency-rule fitness (proven to bite); **E-3** supply-chain gate (`deny.toml` + CI `cargo-deny`/`cargo-audit`, validated locally). SEDD/ARCH reconciled (E-4, §7 doc-fidelity). **E-2** (sensitive-context ordering property) is scheduled for Wave 4; its contract is written when `sensitive-context` lands in Wave 1. No product BRs. Sandbox-verifiable: **Yes**.

### Wave 1 — kernel-only leaves (fan out in parallel)
Each depends only on `kernel` (+ `contracts` from D-1), so all can be built concurrently, one worktree each.

| Increment | Closes (MVP BRs only) | External crate | Notes (incl. deferred depth) |
|---|---|---|---|
| `layout-engine` **finalize** | BR-47 (**Must**), BR-53 (**Must\***, per D-5) | — | Wave 0 shipped only the tracer row. Closes the MVP layouts: alpha + number/symbol/punctuation (BR-47) and RTL/bidi (BR-53, conditional per D-5). **Deferred depth:** ergonomic/one-handed/split (BR-51, v1.x), complex-script readiness (BR-54, v2+). Depends only on `kernel`. |
| `dictionary` | BR-10, BR-12 | `fst` | Prefix/fuzzy lookup. Every predictive module reads it → must precede Wave 3. |
| `secure-store` | BR-8, BR-23, BR-62 (at-rest) | `redb`, `aes-gcm`, `hkdf`, `zeroize` | Implements the `SecureStore` port. Precedes personalization/clipboard. |
| `touch-model` | BR-7 (geometry), BR-46 | — | Tap-distribution model (per D-3). Feeds input-decoder finalize. |
| `sensitive-context` | BR-26 | — | The learn/predict gate. Writes the E-2 ordering contract. |
| `smart-typing` | BR-48 | — | Auto-cap, smart punctuation. Independent. |
| `editing` | BR-49 | `unicode-segmentation` | Cursor/selection ops. Independent. |
| `diagnostics` | BR-60 | — | Opt-in content-free ring buffer. **Deferred depth:** user-exportable history (BR-61, v1.x). Independent. |
| `crash-guard` | BR-29, BR-30, BR-31 (core half) | — | Safe-mode/error-conversion host-testable now; FFI-seam/watchdog validated in Wave 5. |

Sandbox-verifiable: **Yes** (network permitting — R-5).

### Wave 2 — one hop from Wave 1
| Increment | Closes (MVP BRs only) | Depends on / deferred depth |
|---|---|---|
| `personalization` | BR-7 (vocab), BR-13 | `secure-store` (port). **Deferred depth (v1.x):** view/reset UI (BR-9), edit dictionary (BR-14), foreign-dict import (BR-57). |
| `locale-manager` | BR-16, BR-17, BR-18, BR-19b | `dictionary` (per D-2). **Deferred depth:** language breadth (BR-19a v1.x, BR-19 v2+). |
| `input-decoder` **finalize** | BR-6, BR-46 (+ BR-7 targeting) | `touch-model` — **scheduled break D-4** |

Sandbox-verifiable: **Yes**.

### Wave 3 — the predictive layer
| Increment | Closes (MVP BRs only) | Depends on / deferred depth |
|---|---|---|
| `prediction` | BR-10 (competitive) | `dictionary`, `locale-manager` (statistical n-gram; neural behind the `Predictor` port, v1.x). **Deferred depth:** inline-prediction polish (BR-42, v1.x), neural quality (BR-11, v1.x). |
| `autocorrect` | BR-12, BR-18 | `dictionary`, `personalization` (whitelist), `locale-manager`. Must follow personalization so the no-clobber property test passes. **Deferred depth:** alternative-word UI (BR-45, v1.x). |

Sandbox-verifiable: **Yes**.

### Wave 4 — Rust composition (last sandbox-buildable wave)
`featherkey-core` — compose all MVP core crates behind the ports in `contracts`, present the UniFFI surface, and enforce the E-2 sensitive-context ordering property here. Closes no new product BR directly but is the prerequisite for the shell. Sandbox-verifiable: **Yes** (Rust side; the generated bindings compile against the shell in Wave 5).

### Wave 5 — Android shell integration (NOT sandbox-buildable)
Needs JDK/Gradle/Android SDK/NDK. Order within the wave: `ffi-bridge` → `ime-service`, `keyboard-view`, `platform-services` → `onboarding`, `settings-ui`, `accessibility-adapter`.
Closes (MVP BRs only): BR-1, BR-2, BR-29, BR-30, BR-31 (end-to-end), BR-32, BR-33, BR-34, BR-35, BR-55, BR-58, BR-62 (keys)/BR-63, and **BR-22** (Must — plain-language consent visibility + withdrawal, via `settings-ui` + `onboarding`). **Deferred depth on these shell modules:** theme customization (BR-36, v1.x), key-preview/long-press polish (BR-37, v1.x), emoji entry (BR-44, v2+), haptic/sound feedback (BR-52, v1.x), switch-access (BR-56, v1.x). Validated only once the `android-shell` CI job is live (§8).

### Deferred (post-MVP — keep OUT of the MVP waves; every BR here is v1.x/v2+ per SEDD §15)
- **v1.x — new modules:** `gesture` (BR-41), `clipboard-core` (BR-50), `neural-runtime` (BR-11).
- **v1.x — depth on MVP-built modules:** layout-engine ergonomic modes (BR-51); personalization BR-9/BR-14/BR-57; autocorrect alternatives (BR-15, BR-45); prediction inline polish (BR-42); diagnostics exportable history (BR-61); locale-manager BR-19a; shell BR-36/BR-37/BR-52/BR-56.
- **v1.x — process/CI (owned in §7, scheduled v1.x):** reproducible build (BR-24), security review (BR-28), modularity-evolution (BR-39), disclosure policy (BR-64), supply-chain (BR-65), device-matrix perf (BR-3). *Note: E-3 lands `cargo-deny`/`audit` early as a cheap guardrail, but BR-65's formal completion is v1.x.*
- **v2+:** `dictation` (BR-43); emoji (BR-44); complex-script depth (BR-54); language breadth (BR-19).

---

## 6. Per-increment work-item template

Copy this per increment; it is the concrete instance of §3.

```
Increment: <crate/module name>
Wave: <n>          Sandbox-verifiable: <yes/no>
Closes BRs: <ids>  Implements ports: <trait names from contracts crate>
Depends on: <already-built modules>

1. SCOPE (Plan subagent)
   - Read: SEDD §5.x for this module, ARCH §<port section>, BRD <BR ids>.
   - Output: port signature, failing-test list, DoD (§3.2), BDD scenarios.

2. BUILD (Workflow: red → green → refactor)
   - Red: write the failing tests + Gherkin.
   - Green: minimal implementation to pass.
   - Refactor: within fitness caps (≤500 loc/file, ≤60 loc/fn).

3. VERIFY (adversarial subagents; diverse lenses)
   - correctness / hot-path-panic / interface-fidelity / BR-completeness.

4. /r-u-sure GATE  → loop to 2 until ✅ with evidence.

5. INTEGRATE & VALIDATE (R3)
   - Full-workspace regression; API-diff of untouched crates empty.
   - Commit (no AI trailer); tick SEDD §15 + this plan's wave table.
```

---

## 7. BR ownership: module-owned vs CI/process-owned

Not every BR maps to a buildable module. The analysis found a set of **quality-attribute / cross-cutting BRs** that are satisfied structurally, by CI, or by release process — scheduling them as "build module X" would be a category error. They are owned as follows and must appear on the plan even though no wave "builds" them:

| Ownership | BRs | Vehicle |
|---|---|---|
| **Structural (emergent from architecture)** | BR-4, BR-20, BR-21, BR-23, BR-25, BR-38, BR-39, BR-40, BR-59 | Enforced by the no-network core, modular decomposition, and fitness functions — verified by CI gates, not a module. |
| **CI-enforced** | BR-27 (permission allowlist), BR-65 (supply-chain) | E-3 CI jobs (`cargo-deny`/`audit`, permission/network guard). |
| **Release-process** | BR-24 (reproducible build), BR-64 (disclosure policy), BR-66 (secure update), BR-67 (open-source), BR-28 (security review) | Release checklist + repo docs (`SECURITY.md`), not code. |
| **Perf-property** | BR-2 (cold-start), BR-3 (device-matrix) | Benchmarks + the Wave 5 shell; no owning module. |

**Doc-fidelity fixes to make (small SEDD edits, not blockers):** BR-37 missing from `accessibility-adapter`'s "Serves" list; BR-38 owner is a principle not a crate; BR-62 secure-store/platform-services split is implied not stated. Fold into the next SEDD revision.

---

## 8. What cannot be verified in this sandbox

Honesty about the boundary (no JDK/Gradle/Android SDK/NDK here):

- **All of Wave 5** (Android shell) and the **end-to-end** halves of BR-1, BR-2, BR-29–31, BR-32–34, BR-55, BR-58, BR-62/63.
- **`crash-guard`'s** FFI-seam and watchdog behavior (its core logic *is* testable now).
- The **UniFFI-generated bindings** actually compiling against Kotlin.

These are validated by standing up the dormant `android-shell` CI job on a runner that has the Android toolchain (the workflow is already written and gated on `android/gradlew` existing). Until then, Wave 5 work is authored and reviewed but explicitly marked *unverified*, exactly as the current scaffold is.

---

## 9. Risks & mitigations

| ID | Risk | Mitigation |
|---|---|---|
| R-1 | `input-decoder` API break (D-4) ripples into the tracer + any early shell wiring. | Schedule it as the *only* planned break; keep an unbiased default model so existing tests pass; do it in Wave 2 before the shell consumes it. |
| R-2 | Domain→adapter edge slips in (port rule eroded). | E-1 fitness rule + the `contracts` crate (D-1) make the legal edge the easy one and the illegal one a red build. |
| R-3 | `sensitive-context` gate ordering regresses silently (a privacy Must, BR-26). | E-2 property test at the composition root; no learning path ships without it. |
| R-4 | Cross-module BRs (BR-7/11/16/18/50/62) look "done" at single-crate level but aren't closed until the last contributing wave. | Track these as *multi-wave* BRs; their acceptance BDD runs only after the final contributing increment. |
| R-5 | Sandbox goes offline → `fst`/`redb`/`aes-gcm`/etc. can't be fetched, blocking Wave 1 verification. | Vendor dependencies (`cargo vendor`) once, early; pin versions; keep `Cargo.lock` for any crate that needs reproducibility. |
| R-6 | `neural-runtime` (v1.x) pulls large `tract`/`candle` trees, hurting footprint (BR-4/40). | Already feature-gated off by default (SEDD §5.5 r5); MVP excludes it entirely; measure footprint when v1.x lands. |
| R-7 | Workflow fan-out with shared files causes conflicts. | Parallel crate builds use git-worktree isolation; only the barrier stage touches shared traceability. |

---

## 10. How this plan stays consistent with the source docs

- **Direction of authority:** BRD → SEDD → ARCH → this plan. A conflict is resolved by fixing the *lower* doc; if a requirement itself is wrong, the BRD is revised first.
- **Every increment updates traceability:** closing a BR ticks its SEDD §15 row and this plan's wave table in the same commit.
- **The pre-flight D-items may edit SEDD** (D-3 two-domain writer split; E-4 adds `featherkey-core`/`featherkey-ffi`; §7 doc-fidelity fixes). Those edits are the mechanism by which this plan feeds corrections *back* up the chain — always to the source doc, never only here.
- **This plan is re-audited** (`/r-u-sure`) whenever a wave completes or a D-item changes the order, so "what's next" is always current and evidence-backed.

---

*End of Implementation Plan v0.1.*
