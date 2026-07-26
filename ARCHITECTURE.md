# Architecture Document (ARCH)

**Project (working name):** FeatherKey — A Fast, Private, Modular Android Keyboard
**Document type:** Software Architecture — rules, structure, and "how-to"
**Version:** 0.3 (Draft)
**Date:** 2026-07-24
**Status:** Draft — for engineering review
**Document chain:**
- [`BUSINESS_REQUIREMENTS.md`](./BUSINESS_REQUIREMENTS.md) (BRD v0.7) — *what & why* (**source of truth**)
- [`SOFTWARE_ENGINEERING.md`](./SOFTWARE_ENGINEERING.md) (SEDD v0.8) — *the tech idea & decisions*
- **This document (ARCH)** — *the how-to: architecture, rules, conventions* (**normative**)

> **How to read this document.** It is **normative**: rules use **MUST / SHOULD / MUST NOT** (RFC-2119 sense). Where this document conflicts with the BRD, the **BRD wins**; where it refines the SEDD, it must stay consistent with the SEDD's ratified decisions (ADR-1/2/3). This document exists to make the BRD's modularity and maintainability requirements (**BR-38, BR-39, BR-40**) *enforceable* — not aspirational. Rules here are backed by CI "fitness functions" wherever possible (§13).

### Revision History

| Version | Date | Summary |
|---|---|---|
| 0.1 | 2026-07-24 | Initial architecture: Hexagonal/Clean style, SOLID rules, module registry, anti-god-file caps, mandatory TDD/BDD workflow, ports & adapters catalog, repo layout, add-a-module recipe, fitness functions, traceability |
| 0.2 | 2026-07-24 | Set minimum test coverage to **98%** line+branch (§7.3); added this revision history for consistency with the BRD/SEDD; synced document-chain references to BRD v0.7 / SEDD v0.6 (after the BR-17 resolution) |
| 0.3 | 2026-07-24 | Synced SEDD chain reference to v0.7 (adds ADR-12–16, the `contracts`/`featherkey-core` registry entries, and the two-domain writer split). No architectural change in this document. |
| 0.4 | 2026-07-24 | Reconciled the document with the as-built repo (per SEDD ADR-17): added `contracts` to the module registry (§5.4); updated the module anatomy (§5.2) and repository layout (§11) to the actual `crates/` workspace + centralized repo-root `features/` + flat single-concept `src/`; made per-crate `README.md` a stated requirement (now satisfied). No change to any architectural mandate (AM-1..AM-7) or fitness function. |
| 0.5 | 2026-07-26 | Restructured to an `apps/` + `core/` monorepo (SEDD **ADR-20**, superseding ADR-17): updated the repository layout (§11) and path pointers to `core/crates/*`, `core/features/`, `core/tools/`, `apps/android/`. No architectural-mandate or fitness-function change. |

---

## Table of Contents

1. [Purpose & Scope](#1-purpose--scope)
2. [Architectural Mandates (Non-Negotiables)](#2-architectural-mandates-non-negotiables)
3. [Architectural Style — Clean / Hexagonal](#3-architectural-style--clean--hexagonal)
4. [SOLID, Applied Concretely](#4-solid-applied-concretely)
5. [Module Architecture & Rules](#5-module-architecture--rules)
6. [Anti-God-File Rules & Code Organization](#6-anti-god-file-rules--code-organization)
7. [TDD Workflow (Test-First, Mandatory)](#7-tdd-workflow-test-first-mandatory)
8. [BDD Workflow (Behavior-First)](#8-bdd-workflow-behavior-first)
9. [Ports & Adapters Catalog](#9-ports--adapters-catalog)
10. [Cross-Cutting Architecture Rules](#10-cross-cutting-architecture-rules)
11. [Repository & Directory Layout](#11-repository--directory-layout)
12. [How to Add a New Module (Recipe)](#12-how-to-add-a-new-module-recipe)
13. [Enforcement & Governance (Fitness Functions)](#13-enforcement--governance-fitness-functions)
14. [Traceability — Rules → Requirements](#14-traceability--rules--requirements)
15. [Glossary](#15-glossary)

---

## 1. Purpose & Scope

This document specifies **how the project is structured and built**: the architectural style, the module rules, the SOLID conventions, the mandatory TDD/BDD workflow, the file-organization limits that forbid god-files, and the process for adding new code. It is the day-to-day rulebook for every contributor.

**In scope:** architecture style, layering, module boundaries, dependency rules, SOLID conventions, test-first workflow, BDD acceptance flow, naming/file conventions, repository layout, enforcement.

**Out of scope:** business rationale (BRD), technology selection rationale (SEDD ADRs), and implementation code itself. This document tells you *how to write* the code, not the code.

---

## 2. Architectural Mandates (Non-Negotiables)

These are the mandates the sponsor and BRD set for the architecture. Every rule in this document serves them.

| # | Mandate | Meaning | Backed by |
|---|---|---|---|
| AM-1 | **Completely modular** | The system is a set of independent modules with explicit boundaries; nothing reaches across a boundary except through a published port. | BR-38, BR-39 |
| AM-2 | **SOLID everywhere** | Every module, type, and function obeys the five SOLID principles (§4). | BR-38, BR-39 |
| AM-3 | **TDD-first** | No production code is written before a failing test that requires it (§7). | BR-5/6/12/26/29–31 (correctness/safety) |
| AM-4 | **BDD-first** | Every user-facing behavior begins as an executable Gherkin scenario traced to a BR (§8). | All user-facing BRs |
| AM-5 | **No god-files, no god-objects** | Files and types are small and single-purpose; hard size/complexity caps are CI-enforced (§6). | BR-38, BR-39, BR-40 |
| AM-6 | **One job per module** | Each module does exactly one well-rounded, well-specified job — a single *reason to change*. | BR-38 |
| AM-7 | **Easy to maintain** | Boundaries, tests, and conventions make any change local, obvious, and safe. | BR-39, BR-40 |

> **Mandate precedence** (when two pull apart): correctness/safety (AM-3) > modular boundaries (AM-1/AM-6) > SOLID form (AM-2) > file-size limits (AM-5). This mirrors the SEDD principle precedence and prevents "purity" from overriding a safety test.

---

## 3. Architectural Style — Clean / Hexagonal

FeatherKey uses **Hexagonal Architecture (Ports & Adapters)**, a.k.a. Clean Architecture. This is the canonical style for "modular + SOLID + testable-first," and it maps exactly onto the SEDD's Rust-core / Kotlin-shell split (SEDD §4).

### 3.1 The layers

```
        ┌───────────────────────────────────────────────┐
        │                  ADAPTERS                      │  (Kotlin shell + platform crates)
        │   IME service · rendering · settings UI ·      │
        │   Keystore · storage(redb) · system clipboard  │
        │        ▲                              ▲        │
        │        │ implements                   │ drives │
        │   ┌────┴──────────────────────────────┴────┐   │
        │   │               PORTS                     │   │  (traits owned by the core)
        │   │  driving ports (use-case APIs) +        │   │
        │   │  driven ports (SecureStore, Clock, …)   │   │
        │   │  ┌───────────────────────────────────┐  │   │
        │   │  │          APPLICATION              │  │   │  (use cases / orchestration)
        │   │  │   decode-keystroke · suggest ·    │  │   │
        │   │  │   learn · switch-language …       │  │   │
        │   │  │  ┌─────────────────────────────┐  │  │   │
        │   │  │  │          DOMAIN             │  │  │   │  (pure logic: decoding, LM,
        │   │  │  │  entities · value objects · │  │  │   │   autocorrect rules, layouts)
        │   │  │  │  domain services (no I/O)   │  │  │   │
        │   │  │  └─────────────────────────────┘  │  │   │
        │   │  └───────────────────────────────────┘  │   │
        │   └─────────────────────────────────────────┘   │
        └───────────────────────────────────────────────┘
```

### 3.2 The Dependency Rule (the one rule that governs all others)

> **Dependencies point inward, only.** Domain depends on nothing. Application depends only on Domain and Ports. Adapters depend on Ports (and the platform). **The Domain and Application MUST NOT know anything about Android, UI, storage engines, crypto libraries, or the network.**

- **Rust core** = Domain + Application + Ports (trait definitions). It has **zero Android dependencies** (SEDD §5.5 rule 2) and is fully testable on the host.
- **Kotlin shell + infra crates** = Adapters implementing the driven ports and calling the driving ports.
- **Composition root** (the only place that knows every concrete type) wires adapters to ports at startup (§9.3).

### 3.3 Why this style (traceability)

- **Modularity (AM-1, BR-38):** boundaries are physical (crates/packages) and enforced by the dependency rule.
- **Testability / TDD (AM-3):** the Domain is pure → unit-testable with no device, no mocks-of-frameworks. Ports let us test use cases with in-memory fakes.
- **OCP / swap-ability (AM-2):** the statistical→neural prediction upgrade (SEDD ADR-3) is a new **adapter** behind an existing **port** — no domain change.
- **Privacy/security (BR-20, BR-25):** the Domain literally cannot perform I/O or networking; only audited adapters can, so the "no keylogging" guarantee is structural.

---

## 4. SOLID, Applied Concretely

SOLID is not decoration here — each letter is a **rule with a concrete FeatherKey example** and an enforcement hook.

| Principle | Rule (MUST) | Concrete example | Enforcement |
|---|---|---|---|
| **S — Single Responsibility** | Every module/type/file has exactly **one reason to change**. | `autocorrect` decides corrections; it does **not** persist, render, or learn. Persistence is `secure-store`; learning is `personalization`. | Module-job registry (§5.4); file-size & cohesion review (§6) |
| **O — Open/Closed** | Extend behavior by **adding** a new implementation of a port/strategy, never by editing stable core code. | Adding the neural predictor = new `Predictor` impl; `dictionary`/`autocorrect` callers unchanged. | Port stability review; ADR required to change a port |
| **L — Liskov Substitution** | Any implementation of a port MUST be substitutable without breaking callers; same contracts, same invariants. | `StatisticalPredictor` and `NeuralPredictor` both satisfy `Predictor`; a shared contract test suite runs against both. | Shared "contract test" suite per port (§7.4) |
| **I — Interface Segregation** | Ports are **narrow and role-specific**; no fat interfaces. A consumer depends only on the methods it uses. | `secure-store` exposes `SecureStore` (put/get); it does **not** also expose migration or key-rotation on the same trait — those are separate ports. | Trait method-count lint; review |
| **D — Dependency Inversion** | High-level policy (Domain/Application) defines the **port**; low-level detail (adapter) implements it. Core never imports an adapter. | Domain defines `trait SecureStore`; the `redb`+AES adapter implements it; wiring at the composition root. | Crate-dependency DAG check (§13) forbids core→adapter edges |

### 4.1 SOLID at three scales

SOLID applies recursively — **module, type, and function**:

- **Module scale:** one job (SRP); extended via new sibling modules (OCP); communicates only through ports (DIP/ISP).
- **Type scale:** a struct/class models one concept; a trait is one role.
- **Function scale:** a function does one thing at one level of abstraction; if it needs an "and" to describe it, split it.

---

## 5. Module Architecture & Rules

### 5.1 What is a module

A **module** is a bounded unit that does **one well-specified job** (AM-6). In Rust it is a **crate**; in Kotlin it is a **Gradle module / package** with a published API. A module MUST declare:

1. **Its single job** — one sentence, recorded in the module registry (§5.4) and its `README`.
2. **Its public API** — the only surface other modules may touch (a façade + ports).
3. **Its ports** — driven ports it needs (as traits), driving ports it offers.
4. **Its tests** — unit + property tests co-located, and BDD features for user-facing behavior.

### 5.2 Module anatomy (every module looks the same)

```
<module>/
  README.md            # the module's ONE job, its ports, its invariants (required)
  Cargo.toml           # declares [package.metadata.featherkey] layer (fitness E-1)
  src/
    lib.rs             # public API + logic for small crates; a thin façade once a crate grows
    <feature>.rs       # small, single-concept files (split out as the crate grows)
  tests/               # integration + property tests (public API only), when the crate has them
```

- Every crate has a **`README.md`** stating its one job, ports, and invariants (§5.1).
- Ports are **not** re-declared per module: all port traits live in the shared **`contracts`** crate (ADR-12), so a domain crate implements/consumes a trait it imports rather than owning a `ports/` directory.
- **BDD `.feature` files are centralized** in the **`core/features/`** directory (one `<module>.feature` per crate), not nested per module — this keeps the BR↔scenario traceability check (§8.3) over a single tree.
- For a **small crate**, `src/lib.rs` may hold the crate's logic directly (one concept, within the §6 size caps); the `domain/`/`application/`/`ports/` subfolders are an option a crate adopts **only when it grows** past a single concept, not a mandatory scaffold. The size/complexity caps (§6.1), not the folder count, are what prevent god-files.
- Internal files are **private by default**; only the crate's public API is exported (ISP at module scale).

### 5.3 Inter-module rules (MUST)

1. A module MUST interact with another **only through that module's published API/ports** — never by reaching into internal files (Rust: no `pub(crate)` leakage across crates; Kotlin: `internal` visibility).
2. The module dependency graph MUST be a **DAG** (no cycles). CI enforces this (§13).
3. Dependencies MUST point **inward** per the Dependency Rule (§3.2): no Domain/Application crate may depend on an adapter crate.
4. Shared types crossing a boundary live in a small **`contracts`/`kernel`** crate (value objects, error types) that has **no dependencies** of its own.
5. A module MUST NOT grow a **second reason to change**. When it does, it is **split** (see §12 for the recipe) — this is how we prevent modules from silently becoming god-modules.

### 5.4 The module registry (single job per module)

Authoritative list; each row is one job (consistent with SEDD §5). Any new module MUST be added here with a one-sentence job before code is merged.

**Domain / Application (Rust core — pure, no platform):**

| Module | Its ONE job |
|---|---|
| `kernel` | Define shared value objects and error types crossing module boundaries (no logic, no deps). |
| `contracts` | Define the port traits (driven & driving) domain crates depend on instead of adapters (no logic, deps on `kernel` only; ADR-12). |
| `input-decoder` | Turn a touch + layout + touch-model into ranked intended keys. |
| `touch-model` | Maintain the per-user adaptive tap-distribution model. |
| `layout-engine` | Provide keyboard layout geometry (alpha/number/symbol/RTL/ergonomic). |
| `locale-manager` | Track active languages and identify the per-word language. |
| `dictionary` | Look words up in compact per-language lexicons. |
| `prediction` | Produce autocomplete / next-word suggestions. |
| `autocorrect` | Decide corrections and alternatives, never clobbering intended words. |
| `personalization` | Learn user vocabulary/habits and own the user dictionary. |
| `gesture` | Decode swipe paths into words. |
| `smart-typing` | Apply auto-cap / double-space-period / smart punctuation. |
| `editing` | Model cursor movement and text-selection operations. |
| `clipboard-core` | Model clipboard history with sensitivity + expiry rules. |
| `sensitive-context` | Decide whether the current field is sensitive (suppresses learning). |
| `diagnostics` | Maintain the opt-in, content-free local diagnostics buffer. |
| `neural-runtime` *(v1.x)* | Run small neural model inference behind the `Predictor` port. |
| `dictation` *(v2+)* | Provide privacy-preserving voice-to-text behind a port. |

**Adapters / Infrastructure (implement driven ports):**

| Module | Its ONE job |
|---|---|
| `secure-store` | Encrypt and persist personal data (implements `SecureStore`). |
| `crash-guard` | Isolate faults at the FFI seam and provide safe-mode fallback. |
| `featherkey-core` | Composition façade: compose the core crates and expose the UniFFI API. |

**Adapters (Kotlin shell):**

| Module | Its ONE job |
|---|---|
| `ime-service` | Drive the Android `InputMethodService` lifecycle and text commits. |
| `keyboard-view` | Render the keyboard and capture touch. |
| `settings-ui` | Present configuration screens. |
| `onboarding` | Run first-run and the enablement trust flow. |
| `accessibility-adapter` | Bridge keys/actions to TalkBack & switch access. |
| `platform-services` | Provide Keystore, backup rules, and system-clipboard adapters. |
| `ffi-bridge` | Marshal calls between Kotlin and the Rust core. |

---

## 6. Anti-God-File Rules & Code Organization

God-files and god-objects are the primary enemy of maintainability (AM-5, AM-7). These limits are **CI-enforced fitness functions**, not guidelines.

### 6.1 Hard limits (build fails if exceeded)

| Rule | Soft (warn) | Hard (fail CI) |
|---|---|---|
| Logical lines per file | 300 | **500** |
| Lines per function/method | 40 | **60** |
| Cyclomatic complexity per function | 10 | **15** |
| Public methods per type/trait | 7 | **10** (triggers SRP split review) |
| Parameters per function | 4 | **6** (use a value object instead) |
| Nesting depth | 3 | **4** |

> Numbers are the initial standard; they may be tuned by ADR, but a file **MUST NOT** be exempted ad hoc. If a file needs to be bigger, that is a signal to **split responsibilities**, not to raise the cap.

### 6.2 One-concept-per-file (MUST)

- A file contains **one primary type** (struct/class/trait) and its tightly-bound helpers.
- The module façade (`lib.rs` / package `api`) contains **no logic** — re-exports only.
- No "utils"/"helpers"/"misc"/"common" dumping-ground files. Utilities live with the concept they serve or in a **named, single-purpose** module.
- No "manager/handler/processor" god-objects: a type whose name is a vague verb-noun and which accumulates unrelated methods MUST be decomposed.

### 6.3 Naming conventions

| Thing | Convention | Example |
|---|---|---|
| Crate / Gradle module | kebab-case, noun of the job | `input-decoder` |
| Rust type | UpperCamelCase | `KeyCandidates` |
| Rust trait (a role/port) | UpperCamelCase, role noun | `SecureStore`, `Predictor` |
| File | snake_case, the concept | `key_candidate.rs` |
| Kotlin class | UpperCamelCase | `ImeService` |
| Test | `describes_expected_behavior` | `never_overrides_whitelisted_word` |
| BDD feature | kebab, behavior | `autocorrect-preserves-intended-word.feature` |

### 6.4 Folder conventions

Every module follows the anatomy in §5.2. No module invents its own layout. This uniformity is itself a maintainability feature: any engineer can open any module and know where things are.

---

## 7. TDD Workflow (Test-First, Mandatory)

**AM-3: production code MUST be preceded by a failing test that requires it.** This is a hard cultural + CI rule, not a preference.

### 7.1 The loop (Red → Green → Refactor)

1. **Red:** write the smallest failing test that expresses the next required behavior.
2. **Green:** write the minimum code to pass it.
3. **Refactor:** clean up under green, keeping SOLID and the file limits (§6).

Commits SHOULD reflect this rhythm; a PR that adds behavior with **no accompanying test** is rejected in review and flagged by coverage gates.

### 7.2 Test taxonomy & placement

| Test kind | Tool | Lives in | Scope |
|---|---|---|---|
| Unit | `cargo test` / JUnit | next to the code (`#[cfg(test)]` / `src/test`) | one type/function |
| Property | `proptest` | module `tests/` | invariants over generated inputs |
| Contract (per port) | shared suite | port's crate `tests/` | every adapter of a port (LSP) |
| Fuzz | `cargo-fuzz` | `fuzz/` | untrusted inputs (security) |
| Integration | `cargo test` / instrumented | module `tests/` | public API only |
| Acceptance (BDD) | `cucumber` / Cucumber-JVM | `core/features/` | user-facing behavior (§8) |

### 7.3 Coverage & gates

- Domain/Application crates: **high line + branch coverage** required — **minimum 98%** line + branch; CI gates block merge below threshold. (The 98% denominator is *meaningful logic*: trivial/generated code — derives, façade re-exports, exhaustive-match arms that are unreachable — may be explicitly annotated as excluded, so the bar drives real test depth rather than gaming trivial lines.)
- Safety-critical invariants are **property tests**, not example tests (e.g., BR-12 "never clobber a whitelisted word", BR-26 "never learn a password field").
- Every fixed defect gets a **regression test** (especially reliability faults, P-4 class).

### 7.4 Contract tests enforce Liskov (per port)

Each **port** owns a reusable **contract test suite**. Every adapter implementing that port MUST pass it. Example: `SecureStore` contract asserts round-trip integrity, tamper detection, and namespace isolation — run against the `redb` adapter and any in-memory test fake. This makes LSP (§4) mechanically checked, not assumed.

---

## 8. BDD Workflow (Behavior-First)

**AM-4: every user-facing behavior starts as an executable Gherkin scenario traced to a BR.** BDD is our **executable acceptance criteria** — it connects the BRD directly to running tests.

### 8.1 Flow

1. Take a BR (or a slice of one).
2. Write a `.feature` file in **Given/When/Then** describing the observable behavior; tag it with the BR ID.
3. The scenario fails (no implementation).
4. Drive the implementation with TDD (§7) until the scenario passes.
5. The scenario is now a permanent, living acceptance test.

### 8.2 Tooling

- **Rust core behavior:** the `cucumber` crate runs `.feature` files against the core's public API (host-run, fast).
- **Kotlin/end-to-end behavior:** Cucumber-JVM / Kotest with the on-device harness for IME-level scenarios.

### 8.3 Traceability tag (MUST)

Every scenario is tagged with the requirement it verifies, e.g. `@BR-12`. A CI check maps scenarios ↔ BRs and reports which user-facing BRs still lack an acceptance scenario.

### 8.4 Example — BR-12 (autocorrect must never clobber an intended word)

```gherkin
@BR-12 @autocorrect
Feature: Autocorrect preserves the word the user intended

  Scenario: A whitelisted custom word is never replaced
    Given "Ferrari" is in the user's dictionary
    When the user types "Ferrari"
    Then autocorrect does not replace it
    And "Ferrari" is committed exactly as typed

  Scenario: An out-of-dictionary word is offered, not forced
    Given the user types "gonna" which is not in the active dictionary
    When autocorrect runs
    Then the original "gonna" remains selectable as the primary candidate
    And any correction is offered only as an alternative
```

### 8.5 Example — BR-16 (concurrent languages, no manual switch)

```gherkin
@BR-16 @multilingual
Feature: Type two languages at once without switching

  Scenario: Portuguese word inside an English sentence
    Given English and Portuguese are both active
    When the user types "let's meet at the padaria"
    Then "padaria" is accepted as Portuguese
    And no language switch was required
```

Scenarios like these are the **contract with the BRD** — when they pass, the requirement is demonstrably met.

---

## 9. Ports & Adapters Catalog

### 9.1 Driving ports (use-case APIs the shell calls)

These are the core's inbound API (the UniFFI surface via `featherkey-core`). Kept narrow (ISP).

| Port | Purpose | Serves |
|---|---|---|
| `DecodeKeystroke` | touch → committed key/candidates | BR-5, BR-6, BR-46 |
| `Suggest` | context → completions/next-word | BR-10, BR-42 |
| `Correct` | token → correction/alternatives | BR-12, BR-45 |
| `SwitchLanguage` / `ActiveLanguages` | manage concurrent languages | BR-16–19b |
| `LearnFromInput` | feed observed input to learning | BR-7, BR-11 |
| `ManageUserDictionary` | view/add/remove/import words | BR-9, BR-13, BR-14, BR-57 |

### 9.2 Driven ports (the core needs; adapters implement)

| Port | Implemented by (adapter) | Serves |
|---|---|---|
| `SecureStore` | `secure-store` (redb + AES-GCM) | BR-8, BR-23, BR-62 |
| `KeyProvider` | `platform-services` (Android Keystore) | BR-62 |
| `SensitiveContextSource` | `platform-services` (`EditorInfo`) | BR-26 |
| `SystemClipboard` | `platform-services` | BR-50 |
| `Clock` | shell (test-injectable) | determinism/tests |
| `HapticSink` / `SoundSink` | `keyboard-view` | BR-52 |
| `TextCommitter` | `ime-service` (`InputConnection`) | BR-31 |

> Driven ports are why the Domain stays pure: it *asks* for storage/keys/clock through a trait; it never *knows* it's redb, Keystore, or Android.

### 9.3 The composition root

- **One** place wires concrete adapters to ports: the app's startup path (`featherkey-core` façade for the Rust side + the Kotlin composition module).
- It is the **only** place allowed to name every concrete type; everywhere else depends on ports (DIP).
- This keeps wiring visible and testable, and lets tests swap real adapters for fakes trivially.

---

## 10. Cross-Cutting Architecture Rules

| Concern | Rule (MUST) | Serves |
|---|---|---|
| **Errors** | Core functions return `Result<T, DomainError>`; **no panics cross the FFI**; panics caught at the seam and converted (`crash-guard`). Error types live in `kernel`. | BR-29, EP-6 |
| **Concurrency** | The input path is single-threaded and non-blocking; no I/O, crypto, or lock contention on it. Cross-thread handoff uses immutable snapshots + bounded channels. | BR-1, BR-46 |
| **Immutability** | Domain entities are immutable value objects where practical; state mutation is confined to the owning module (one-writer rule, SEDD §5.5). | AM-1, BR-38 |
| **No hidden I/O** | Domain/Application MUST NOT perform I/O, time, or randomness directly — only via injected ports (`Clock`, `SecureStore`, …). Makes everything deterministic & testable. | AM-3, BR-8 |
| **Logging** | Structured, content-free; routed to `diagnostics`; release builds compile out content-bearing logs. | BR-20, BR-60 |
| **Config / flags** | Optional capabilities are Cargo features off by default (`neural-runtime`, `dictation`); no remote config (would need a network channel). | BR-4, BR-59 |
| **Public API surface** | A module's public API is the smallest set that satisfies its consumers; widening it requires review. | ISP, BR-38 |

---

## 11. Repository & Directory Layout

A **monorepo**: a Cargo workspace (Rust core) plus a Gradle multi-module build (Android shell), so boundaries are physical and the dependency DAG is machine-checkable.

```
featherkey/                          # monorepo root
  ARCHITECTURE.md  BUSINESS_REQUIREMENTS.md  SOFTWARE_ENGINEERING.md
  IMPLEMENTATION_PLAN.md  PLAY_STORE_PUBLISHING.md  README.md
  apps/                             # deployable applications
    android/                        # Gradle multi-module keyboard app (built in Wave 5)
      app/                          # composition root (Kotlin) + manifest
      ime-service/  keyboard-view/  settings-ui/  onboarding/
      accessibility-adapter/  platform-services/  ffi-bridge/
    web/                            # website app (placeholder; design pending)
  core/                             # the Rust engine — a single Cargo workspace
    Cargo.toml                      # workspace: lists all core crates
    deny.toml  rust-toolchain.toml
    crates/                         # one crate per module
      kernel/                       # shared value objects + errors (no deps)
      contracts/                    # port traits, deps on kernel only (ADR-12)
      input-decoder/  touch-model/  layout-engine/  locale-manager/
      dictionary/  prediction/  autocorrect/  personalization/
      smart-typing/  editing/  sensitive-context/  diagnostics/
      secure-store/                 # adapter: persistence/crypto
      crash-guard/                  # adapter: fault isolation
      featherkey-core/              # composition: façade + UniFFI-ready surface (Wave 4; ADR-18)
      # planned, not yet built: gesture, clipboard-core, neural-runtime (v1.x), dictation (v2+)
      <crate>/README.md  <crate>/src/  <crate>/tests/   # (each follows §5.2 anatomy)
    features/                       # ALL BDD .feature files, one <module>.feature per crate
    tools/                          # fitness/ (§13), bdd_check.py (§8.3), ci-local.sh
  # planned: build-logic/ (Gradle convention plugins), fuzz/ (cargo-fuzz targets)
```

- The Rust core lives under **`core/`** (a single Cargo workspace at `core/crates/*`); the Android app lives under **`apps/android/`**, with **`apps/web/`** reserved for the website. BDD features are **centralized** under **`core/features/`** rather than nested per crate (§5.2). The move from the original flat repo-root layout to this `apps/` + `core/` monorepo is recorded as **ADR-20** (which supersedes **ADR-17**).
- Each `core/crates/*` crate follows the **same anatomy** (§5.2) and carries its own `README.md`; the DAG and layer rules are enforced by `core/tools/fitness` (§13).
- Crates are added to the workspace **as they are implemented** (TDD-first), so the workspace member list is the source of truth for what exists today; the §5.4 registry is the full target map, including modules not yet built.

---

## 12. How to Add a New Module (Recipe)

The canonical, mandatory sequence — this *is* the process (satisfies AM-1..AM-7 by construction):

1. **Name the one job.** Write a single sentence. If it needs "and," it's two modules. Add it to the registry (§5.4) and a `README`.
2. **Write the BDD feature(s).** Express the behavior in Gherkin, tagged with the BR(s) it serves (§8). They fail.
3. **Define the ports.** Declare the driven ports it needs (traits) and the driving port it offers. Keep them narrow (ISP).
4. **Write failing unit/contract tests (TDD Red).** Including property tests for any invariant, and the port contract suite if it defines a port.
5. **Implement the Domain (Green).** Pure logic, no I/O; one concept per file, within the size caps (§6).
6. **Add the adapter(s)** for any driven port, in the adapter layer — never in the Domain.
7. **Wire at the composition root** (§9.3) — the only place that names concrete types.
8. **Refactor under green;** confirm the crate DAG stays acyclic and inward-pointing (CI, §13).
9. **Update traceability** (§14) so the new module maps to its BR(s).

A change that would make an **existing** module grow a second job follows the same recipe to **split** it — never bolt the second job on.

---

## 13. Enforcement & Governance (Fitness Functions)

Architecture rules are worthless unless enforced. CI runs **fitness functions** that fail the build on violation.

| Rule enforced | How | Mandate |
|---|---|---|
| Acyclic, inward-pointing module DAG | Cargo/Gradle dependency graph check; forbids core→adapter and any cycle | AM-1, DIP |
| No cross-module internal access | Rust visibility + crate boundaries; Kotlin `internal`; API-lint | AM-1, ISP |
| File/function/complexity caps | `clippy`/`detekt` + custom lints (§6.1) | AM-5 |
| Test-first / coverage | Coverage gates on domain crates; PR-review rule | AM-3 |
| Every user-facing BR has a scenario | BDD tag ↔ BR mapping check (§8.3) | AM-4 |
| Port stability | Changing a published port requires an ADR | AM-2, OCP |
| No network on core | Dependency + manifest scan (SEDD EP-1) | BR-20, BR-59 |
| No `unsafe` outside audited FFI | `#![forbid(unsafe_code)]` on logic crates | BR-25 |

### 13.1 Governance

- **Architecture Decision Records (ADRs):** any change to a port, a module boundary, or a §6 limit requires an ADR (continuing the SEDD's ADR log).
- **Definition of Done** for any change includes: BDD scenario green, TDD tests green, coverage gate met, DAG check green, traceability updated.
- **Fitness functions run on every PR** — architecture erosion is caught mechanically, not at review-time only.

---

## 14. Traceability — Rules → Requirements

How this document's rules satisfy the BRD's architecture-relevant requirements and stay consistent with the SEDD.

| Requirement (BRD) | Architecture rule(s) that satisfy it |
|---|---|
| **BR-38** (single-responsibility modules) | AM-1/AM-6, module registry (§5.4), SRP (§4), anti-god-file caps (§6), DAG enforcement (§13) |
| **BR-39** (modularity supports evolution) | Hexagonal ports (§3, §9), OCP (§4), add-module recipe (§12), ADR governance (§13.1) |
| **BR-40** (sustained tiny footprint) | Feature-gated optional modules (§10), small files/functions (§6), no dumping-grounds |
| **BR-12, BR-26** (safety invariants) | Property + contract tests mandatory (§7.3), BDD acceptance (§8.4) |
| **BR-20, BR-59** (no keylogging / offline) | Dependency Rule keeps Domain I/O-free (§3.2); no-network fitness function (§13) |
| **BR-25** (hardening) | `forbid(unsafe_code)`, fuzz targets, audited adapters only (§13) |
| **BR-29–31** (reliability) | Error-as-value + FFI panic isolation (§10), `crash-guard` module (§5.4) |
| **All user-facing BRs** | BDD scenarios tagged per BR with a coverage check (§8.3) |

> **Consistency with the SEDD:** module names, the Rust-core/Kotlin-shell split, the ports (SEDD §5.4 interface sketches), the phased prediction seam (ADR-3), and the CI gates all match the SEDD. This document **refines** the SEDD's "what modules" into "how they are structured, bounded, tested, and enforced." No new technology decisions are introduced here; any that arise must go through the SEDD's ADR process.

---

## 15. Glossary

| Term | Definition |
|---|---|
| Hexagonal / Ports & Adapters | Architecture where the core defines ports (interfaces) and the outside world plugs in via adapters |
| Clean Architecture | Equivalent layered style with the inward Dependency Rule |
| Port | An interface (Rust trait / Kotlin interface) owned by the core; a seam |
| Driving port | Inbound API the outside calls to use the core (use cases) |
| Driven port | Outbound interface the core needs; implemented by an adapter (storage, keys, clock) |
| Adapter | Concrete implementation of a port that touches the platform/world |
| Composition root | The single place that instantiates and wires concrete types |
| Domain | Pure business/logic layer with no I/O or framework knowledge |
| Contract test | A shared test suite every implementation of a port must pass (enforces LSP) |
| Fitness function | An automated check that fails the build when an architecture rule is violated |
| God-file / god-object | An oversized file/type doing many jobs — forbidden (AM-5) |
| TDD | Test-Driven Development — failing test first, then code |
| BDD | Behavior-Driven Development — executable Given/When/Then specs |
| Gherkin | The Given/When/Then language for BDD `.feature` files |
| DAG | Directed Acyclic Graph — the required shape of the module dependency graph |
| SRP/OCP/LSP/ISP/DIP | The five SOLID principles (§4) |

---

*End of Architecture Document (Draft v0.3). Document chain: BUSINESS_REQUIREMENTS.md (BRD v0.7, source of truth) → SOFTWARE_ENGINEERING.md (SEDD v0.8) → this.*
