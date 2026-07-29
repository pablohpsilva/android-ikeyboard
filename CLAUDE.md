# CLAUDE.md — how work is done in this repository

Operating contract for any agent or engineer working here. It is short on purpose:
the detail lives in the documents in §7, and this file says **when** to read them.

Goal: **safe and fast**. Those are not in tension here — the speed comes from not
rebuilding what exists, not re-deriving what is written down, and not discovering
a defect three phases after it was introduced.

---

## 0. The three non-negotiables

1. **Tests before code.** No task is done — ever — unless its TDD tests and BDD
   scenarios were written **first**, and were seen to fail before the
   implementation existed. See §3.
2. **Consult before creating.** Nothing is designed, planned, or written before
   `CODEMAP.md` has been queried for what already exists. See §2.
3. **Every phase is gated.** Design, plan, and implementation each exit only on a
   clean `/r-u-sure` verdict, and every gate run updates its markdown artifact.
   See §1.

Breaking one of these is not a shortcut — it is the thing that makes the next
three tasks slow.

---

## 1. The workflow: design → plan → build, each gated

Three phases. Each phase has **one markdown artifact** and **one gate**.

| Phase | Artifact | Gate |
|---|---|---|
| **Design** | `docs/superpowers/specs/YYYY-MM-DD-<slug>-design.md` | `/r-u-sure` until clean |
| **Plan** | `docs/superpowers/plans/YYYY-MM-DD-<slug>.md` | `/r-u-sure` until clean |
| **Build** | the code, tests, and features themselves | `/r-u-sure` until clean |

### 1.1 The gate loop

After finishing a phase — **before** announcing it, and **before** starting the
next one — run `/r-u-sure` against that phase's requirements.

```
  write/revise the artifact
        │
        ▼
   run /r-u-sure  ──►  ✅ Complete and verified  ──►  advance to next phase
        ▲                       │
        │                       ├──►  ⚠️  Done but unverified
        │                       └──►  🚧  Incomplete
        │                              │
        └──── fix the gaps AND update the artifact in the same pass ◄──┘
```

Rules that make the loop real rather than ceremonial:

- **Loop until clean.** Repeat as many times as it takes. There is no pass count
  at which a ⚠️ or 🚧 becomes acceptable.
- **Every run must change something.** A gate run that finds gaps must be
  followed by edits to the artifact *and* to whatever it describes. A gate run
  that changes nothing means the audit was performed, not done — redo it against
  the red-flag table in the `r-u-sure` skill.
- **Record it.** Each artifact carries an `## Audit log` section, appended to on
  every run:

  ```markdown
  ## Audit log
  ### Pass 1 — 🚧 Incomplete
  Gaps: no error path for a corrupt lexicon; BR-31 unmapped; duplicate of
  `featherkey-fold::fold_match` proposed in §4.
  Changed: §4 now delegates to `fold`; §6 adds the corrupt-lexicon case; BR-31
  mapped in §2.
  ### Pass 2 — ✅ Complete and verified
  Evidence: cargo test --workspace 214 passed / 0 failed; fitness exit 0.
  ```

- **Verdicts need evidence, not adjectives.** "Tests pass" without the output is
  a 🚧. If verification was not run, say so — never imply it.
- **The gate audits the phase's own product.** Design is audited against the
  requirements (BRD), the plan against the design, the build against the plan.

### 1.2 What each phase must contain

**Design** — the problem, the requirements (BR IDs) it closes, the module(s)
involved *and whether they already exist* (§2), the port traits, the invariants,
the alternatives rejected and why. A design that names no existing code has not
done §2.

**Plan** — ordered increments, each small enough to be independently verifiable.
Per increment: the failing tests to write first, the Gherkin scenario(s), the
files touched, the Definition of Done (§3), and the rollback if it goes wrong.

**Build** — Red → Green → Refactor, per increment, per
`IMPLEMENTATION_PLAN.md` §3.

---

## 2. Before building anything: query `CODEMAP.md`

`CODEMAP.md` is the generated index of every crate, module, public symbol, and
BDD feature in this repository. It exists so that finding existing code costs a
`grep`, not a repository read.

**Query it — do not read it end to end:**

```bash
grep -n 'Normalize'                CODEMAP.md   # does this already exist?
grep -n -A 30 '^### featherkey-dictionary$' CODEMAP.md   # one crate's full surface
sed -n '/^## 1\./,/^## 2\./p'      CODEMAP.md   # the crate map (read this first)
grep -n 'BR-42'                    CODEMAP.md   # what already serves a requirement
```

**The decision it drives:**

| CODEMAP says | Do this |
|---|---|
| The exact capability exists | Use it. Do not wrap it, do not re-export it, do not copy it. |
| Something close exists, same responsibility | Extend that crate. Add the case to its tests. |
| Something close exists, **different** responsibility | New code — but depend on the existing crate rather than duplicating its logic. |
| Nothing exists, and it is one coherent responsibility | New module. Follow `ARCHITECTURE.md` §12 (the recipe) and declare its layer. |
| Nothing exists, and it spans two responsibilities | It is two modules. Split it before writing it. |

A duplicated implementation is a defect, not a style issue: two copies of the
same rule drift, and the one that drifts is the one nobody remembers exists.
`featherkey-fold` vs. the Kotlin `Diacritics` object is the one deliberate
exception in this repo (an FFI-boundary twin) — and it is documented as such.

**CODEMAP.md is generated. Never hand-edit it.**

```bash
python3 core/tools/codemap.py           # regenerate
python3 core/tools/codemap.py --check   # the gate: exit 1 + a diff if stale
```

Two mechanisms keep it true, and they are not redundant:

- **CI** (`ci.yml` and `core/tools/ci-local.sh`) fails on a stale index. This is
  the enforcement — it works on every clone and cannot be skipped.
- **A `PostToolUse` hook** regenerates it after any edit to a `.rs`, `.kt`,
  `.feature`, `Cargo.toml`, or `settings.gradle.kts` file, so the index is
  accurate *while* the work happens rather than only at merge. This is a local
  convenience: `.claude/` is gitignored, so a fresh clone must opt in by adding
  it to `.claude/settings.local.json`:

  ```json
  { "hooks": { "PostToolUse": [ { "matcher": "Edit|Write|MultiEdit",
      "hooks": [ { "type": "command", "timeout": 30,
        "command": "\"$CLAUDE_PROJECT_DIR/core/tools/codemap-hook.sh\"" } ] } ] } }
  ```

  Without the hook, regenerate manually before finishing a task. The hook never
  blocks an edit; a failure there surfaces at the CI gate instead.

If it is wrong, fix the *source* it is derived from — the crate `README.md`, the
Cargo `description`, the `[package.metadata.featherkey] layer` — then regenerate.

---

## 3. TDD and BDD are entry conditions, not exit criteria

A task is not started until its tests are written, and not done until they pass.

**Order, strictly:**

1. **BDD first** — a Gherkin scenario in `core/features/<module>.feature`, tagged
   with the `@BR-<n>` it closes. It describes observable behaviour in the
   language of the requirement, not the implementation.
2. **TDD next** — the failing unit tests. Run them. *See them fail.* A test that
   has never failed has not been shown to test anything.
3. **Then** the implementation, minimally, until green.
4. **Then** refactor, with the tests as the safety net.

**Definition of Done** — all of `IMPLEMENTATION_PLAN.md` §3.2, in short:
tests green · coverage ≥ 98% line · fitness functions exit 0 · public API matches
the design · a `@BR`-tagged scenario per closed requirement · traceability rows
updated · no panics on the hot path.

Verify with one command:

```bash
bash core/tools/ci-local.sh   # the exact CI gate sequence, locally
```

---

## 4. SOLID, DRY, KISS — by design, not by review

These are properties of the design, so they are decided in phase 1. The gate in
§1.1 audits for them; `ARCHITECTURE.md` §4 has the concrete rules.

**SOLID** — one crate, one reason to change (a crate whose README needs "and" is
two crates). Extend by adding a type that implements a port, not by adding a
branch to a match. Depend on the `contracts` port traits, never on an adapter;
dependencies point inward only.

**DRY** — one rule, one place, verified against `CODEMAP.md` before it is
written. DRY is about *knowledge*, not characters: two functions that look alike
but answer to different requirements are not a duplication, and merging them
couples the requirements together.

**KISS** — the simplest thing that satisfies the requirement and its tests. No
speculative generality, no configuration point without a second caller, no
abstraction introduced before the second concrete case exists. Deferrals are
recorded in the crate README under "Deferred", not built early.

**No god-files.** ≤ 500 lines per file, ≤ 60 lines per function — enforced by
`core/tools/fitness/check.py`, not by taste.

---

## 5. Repository facts worth knowing before you touch anything

- **Monorepo.** `core/` is the Rust engine (host-testable, no Android types).
  `apps/android/` is the Kotlin shell (platform concerns only). Typing logic
  belongs in `core/`; if it is being written in Kotlin, that is a design smell.
- **The Rust core never imports Android/JNI types** — a fitness function fails
  the build if it does.
- **Errors are values.** `unwrap`/`expect`/`panic` are lints in library code.
- **`Cargo.lock` is committed** — the supply-chain gate depends on it.
- **Never commit** native binaries (`.so`), keystores, or `local.properties`.

---

## 6. Commands

```bash
cd core
cargo test --workspace                    # tests
python3 tools/fitness/check.py            # architectural rules
python3 tools/bdd_check.py                # BDD ↔ requirement traceability
python3 -m unittest discover -s tools/tests   # the tooling's own tests
python3 tools/codemap.py --check          # code index freshness
bash tools/ci-local.sh                    # all of the above, in CI order
```

---

## 7. Document map — which file answers which question

| Question | Document |
|---|---|
| What are we building, and why? | `BUSINESS_REQUIREMENTS.md` — **source of truth** (BR-1…67) |
| How is it engineered? Stack, modules, ADRs? | `SOFTWARE_ENGINEERING.md` |
| What rules must the code obey? | `ARCHITECTURE.md` (SOLID, ports, TDD/BDD, fitness) |
| What is the build order, and what is "done"? | `IMPLEMENTATION_PLAN.md` (§3 protocol, §3.2 DoD) |
| **Does this code already exist? Where?** | **`CODEMAP.md`** (generated — §2) |
| How is a release shipped? | `PLAY_STORE_PUBLISHING.md` |

Precedence when they disagree: BRD → SEDD → ARCH → plan. Do not resolve a
contradiction silently — record it and raise it.

---

## 8. Standing preferences

- **No AI attribution anywhere** — no `Co-Authored-By: Claude`, no "generated
  with" trailer, in commits, PRs, or code comments.
- **Do not claim completion without evidence.** §1.1 is the mechanism; a
  confident "done" with no output pasted is the failure it exists to catch.
- **Commit only when asked**, and never directly to `master` for feature work.
