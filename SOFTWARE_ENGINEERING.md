# Software Engineering Design Document (SEDD)

**Project (working name):** FeatherKey — A Fast, Private, Modular Android Keyboard
**Document type:** Software Engineering / Technical Design
**Version:** 0.7 (Draft — expanded)
**Date:** 2026-07-24
**Status:** Draft — for engineering review
**Source of truth:** [`BUSINESS_REQUIREMENTS.md`](./BUSINESS_REQUIREMENTS.md) (BRD v0.7)

### Revision History

| Version | Date | Summary |
|---|---|---|
| 0.1 | 2026-07-24 | Initial technical design: stack, architecture, module decomposition, ADRs, traceability |
| 0.2 | 2026-07-24 | Full expansion of every section — enforcement detail, interface sketches, sequence flows, STRIDE threat model, data schemas, test cases, CI stages, additional ADRs |
| 0.3 | 2026-07-24 | Closed audit gaps: §15 rebuilt as a full per-BR traceability table (69 rows: owner + verification method + roadmap phase); linear proofread pass (validated all internal §/ADR/EP refs, table integrity); clarified a BRD cross-reference |
| 0.4 | 2026-07-24 | Coherence-review fixes: reconciled the MVP accuracy bar (keystroke accuracy = MVP/beat-iOS; prediction quality = competitive at MVP, beat-iOS in v1.x) in ADR-3 + BR-10 traceability; tightened the `no_std` claim; added a note on cohesive module grouping vs strict BR-38 |
| 0.5 | 2026-07-24 | Added the `kernel` crate (shared value objects + error types) to §5.2, for consistency with the Architecture Document (ARCH v0.1) |
| 0.6 | 2026-07-24 | Set minimum coverage to **98%** (§12.5); moved BR-17 to **MVP** in the §15 traceability (consistent with BRD v0.7); synced BRD reference to v0.7 |
| 0.7 | 2026-07-24 | Added ADR-12–16 (proposed: `contracts` port crate; `locale-manager`→`dictionary` edge; two-domain writer split; `input-decoder` signature break; RTL deferral); added `contracts` + `featherkey-core` to §5.2; two-domain writer split rewrite (§5.5 r1, §5.4); doc-fidelity fixes — BR-37 into `accessibility-adapter` §5.1, BR-38 marked structural, BR-62 secure-store/platform-services split (§5.1/§5.2/§15). Supports IMPLEMENTATION_PLAN.md Wave 0.5. |

> **Purpose & relationship to the BRD.** The BRD defines *what* and *why* (business requirements BR-1…BR-67, objectives OBJ-1…9, problems P-1…10). **This document defines *how*** — the concrete technologies, architecture, module decomposition, and engineering practices we will use to satisfy those requirements. The BRD is the **source of truth**: where this document and the BRD conflict, the BRD wins and this document must be corrected (or the BRD explicitly revised). Every significant choice here traces back to one or more BR IDs.
>
> **What this document is not:** it is not code, and not a project plan/schedule. Effort estimates and staffing are out of scope.

---

## Table of Contents

1. [Engineering Principles](#1-engineering-principles)
2. [Key Architecture Decisions (Summary)](#2-key-architecture-decisions-summary)
3. [Technology Stack](#3-technology-stack)
4. [System Architecture](#4-system-architecture)
5. [Module Decomposition](#5-module-decomposition)
6. [Cross-Cutting Concerns](#6-cross-cutting-concerns)
7. [Data Model & Storage](#7-data-model--storage)
8. [Performance Engineering](#8-performance-engineering)
9. [Security Architecture](#9-security-architecture)
10. [Privacy & Data-Handling Architecture](#10-privacy--data-handling-architecture)
11. [Reliability & Failure Isolation](#11-reliability--failure-isolation)
12. [Testing Strategy](#12-testing-strategy)
13. [Build, CI/CD & Reproducibility](#13-build-cicd--reproducibility)
14. [Architecture Decision Records (ADRs)](#14-architecture-decision-records-adrs)
15. [Requirement → Component Traceability](#15-requirement--component-traceability)
16. [Open Technical Questions](#16-open-technical-questions)
17. [Glossary](#17-glossary)

---

## 1. Engineering Principles

These principles are derived directly from the BRD and govern every downstream decision. Each is paired with the **enforcement mechanism** that makes it a checkable engineering rule rather than an aspiration — because a principle with no enforcement is a wish.

| # | Principle | Derived from | Enforcement mechanism |
|---|---|---|---|
| EP-1 | **On-device by default.** No network dependency for any core feature; the app ships with *no* analytics SDK and *no* internet permission on the critical path. | BR-20, BR-23, BR-59; OBJ-3 | CI check fails the build if the core module declares `INTERNET` permission or links a networking crate; manifest lint |
| EP-2 | **Tiny is a budget, not an aspiration.** APK size, RAM, CPU, and battery have hard numeric budgets. | BR-4, BR-40; OBJ-7 | Size & benchmark gates in CI (§8.2) fail the build on regression |
| EP-3 | **Single-responsibility modules.** Each module does exactly one thing; features are added as modules, never by bloating a monolith. | BR-38, BR-39; OBJ-7 | Module dependency graph is acyclic and linted; a module exceeding its responsibility fails review; per-crate public API is reviewed |
| EP-4 | **Memory-safe by construction.** Security-critical code is memory-safe Rust; the input path never touches unsafe C/C++. | BR-25, BR-62; OBJ-4 | `#![forbid(unsafe_code)]` on all logic crates; `unsafe` allowed only in audited FFI crates and flagged in review |
| EP-5 | **Latency is sacred.** The touch→glyph path has a strict frame budget and never blocks on I/O, crypto, or the network. | BR-1, BR-2, BR-46; OBJ-2 | `criterion` latency gates; a lint forbidding blocking I/O on the input-thread call graph |
| EP-6 | **Fail soft, never silent.** Any internal error degrades gracefully to a working keyboard; a fault never requires a phone restart. | BR-29, BR-30, BR-31; OBJ-8 | Fault-injection tests; every FFI seam wrapped in `catch_unwind`; safe-mode reachability test |
| EP-7 | **Verifiable by anyone.** Open-source, reproducible builds so privacy/security claims can be independently checked. | BR-24, BR-67; OBJ-3, OBJ-4 | Reproducible-build job in CI diffs the artifact against a clean rebuild |
| EP-8 | **Privacy-preserving measurement only.** Any telemetry is opt-in, aggregated, content-free, and on-device-first. | BR-60, BR-61; OBJ-3 | `diagnostics` crate has no network capability; consent-gated export only; code review checklist |

### 1.1 Principle Precedence

When two principles pull in opposite directions, resolve in this order (higher wins):

1. **Privacy & Security (EP-1, EP-4, EP-7, EP-8)** — the trust promise is the product; never trade it for a feature.
2. **Reliability (EP-6)** — a working keyboard beats a feature-rich broken one.
3. **Latency (EP-5)** — responsiveness is the most-felt quality.
4. **Footprint (EP-2)** — kept as a hard budget, but a small, justified size increase may buy reliability or safety.
5. **Feature richness** — last; breadth phases in per the roadmap.

*Example:* if adding a neural LM (feature/accuracy) would breach the footprint budget (EP-2) or require networked training (EP-1), the principle order forbids it until it fits — which is exactly why prediction is phased (ADR-3).

---

## 2. Key Architecture Decisions (Summary)

Three foundational decisions were ratified with the sponsor (full rationale in [§14 ADRs](#14-architecture-decision-records-adrs)):

| Decision | Choice | Rationale (short) | ADR |
|---|---|---|---|
| **Build base** | **Hybrid** — greenfield modular architecture, reuse permissively-licensed linguistic data (dictionaries, layouts) | Full control over the tiny/modular/secure/beautiful design that is our differentiator, without re-creating expensive, low-differentiation language data | ADR-1 |
| **Core language** | **Rust core + Kotlin IME shell** | One decision serves three MUST pillars at once: memory-safety (security), native speed (latency), small binaries (footprint) | ADR-2 |
| **Prediction engine** | **Hybrid, phased** — statistical (n-gram + FST) + small neural touch model at MVP; pluggable neural LM in v1.x | Keeps MVP tiny and shippable while leaving a clean seam for the accuracy upgrade; matches the BRD roadmap | ADR-3 |

### 2.1 How the three decisions reinforce each other

These are not independent — they compound:

- **Rust core (ADR-2)** is what makes the **greenfield hybrid (ADR-1)** affordable to own: a memory-safe, testable, Android-independent core is cheap to unit-test and fuzz on the host, so building it ourselves carries less risk than owning an equivalent C++/Kotlin core.
- **Greenfield architecture (ADR-1)** is what makes the **phased prediction engine (ADR-3)** clean: because we control the module boundaries, `prediction` exposes a stable interface behind which the statistical MVP engine is swapped for/augmented by the neural engine with no change to callers.
- **Phased prediction (ADR-3)** is what keeps **Rust core (ADR-2)** honest on footprint: the heavy dependency (`neural-runtime`) is quarantined in one optional crate, so MVP ships tiny.

### 2.2 Decisions deliberately deferred

Not every choice needs making now. The following are **intentionally deferred** to keep options open until we have real data (tracked in §16): final `minSdk`, initial language set, the neural model's exact size/quality envelope, and whether on-device hardware acceleration (NNAPI/GPU) is needed for the accuracy bar. Deferring these is a decision, not an omission — each has a clear trigger for when it must be resolved.

---

## 3. Technology Stack

Fine-grained "what we will use." Specific versions are pinned at implementation time; crates/libraries named are the intended defaults with alternatives recorded in ADRs. **Selection criteria for every dependency:** permissive license (Apache-2.0/MIT, compatible with our open-source license and F-Droid), no network/telemetry, small footprint, active maintenance, and — for anything on the input path — no unsafe surprises.

### 3.1 Languages & Runtime

| Layer | Technology | Why | Alternatives considered |
|---|---|---|---|
| Android IME shell | **Kotlin** (JVM, Android) | Idiomatic Android, first-class `InputMethodService` support, coroutines for off-thread work | Java (more boilerplate, no coroutines) |
| Performance/security core | **Rust** (stable toolchain; `std` core — key deps like `redb`/`fst`/`tract` require it — with `no_std` leaf crates where practical) | Memory safety (EP-4), speed & small footprint (EP-2, EP-5); one core reused across ABIs | C++ (unsafe), pure Kotlin (GC/footprint) — see ADR-2 |
| Rust ⇄ Kotlin bridge | **UniFFI** (Mozilla) for the API surface; hand-written **JNI** only for the hot decode path if profiling demands it | Safe, generated bindings reduce FFI bugs; JNI escape hatch protects latency | Raw JNI everywhere (error-prone), `jni` crate directly (more boilerplate) |
| Settings/app UI | **Jetpack Compose** | Modern, less boilerplate; *not* on the latency-critical keyboard surface | XML Views (verbose) |
| Keyboard rendering surface | **Custom `View` + hardware-accelerated `Canvas`** | Tight control over draw/latency and overdraw; Compose avoided here for the input surface (see ADR-4) | Compose keyboard, `SurfaceView`+render thread |
| Async on shell | **Kotlin Coroutines** + `Dispatchers` | Structured concurrency for background persistence/learning without blocking the IME thread | `Executor`s, RxJava (heavier) |

### 3.2 Core Libraries (Rust)

| Concern | Crate(s) | Notes |
|---|---|---|
| Dictionary / lexicon | **`fst`** (finite-state transducer) | Extremely compact, fast prefix/fuzzy lookup; memory-mapped, ideal for tiny footprint |
| Fuzzy match | `fst` Levenshtein automata / custom | Bounded edit-distance candidate generation for autocorrect |
| Statistical LM | Custom n-gram over quantized, compressed tables | Small, deterministic, no runtime dependency; back-off smoothing |
| Neural inference (v1.x) | **`tract`** or **`candle`** (Rust-native) | Keeps inference in-core; avoids shipping a heavy separate runtime; alternative LiteRT in ADR-5 |
| Crypto | **RustCrypto `aes-gcm`** (+ `hkdf`, `zeroize`); keys via Android Keystore | App-layer AES-256-GCM for data at rest (BR-62); `zeroize` wipes key material from memory |
| Embedded storage | **`redb`** (pure-Rust embedded KV) | Tiny, no C dependency; ACID; encrypted at the app layer |
| Serialization | **`bincode`** / `postcard` (compact, no_std) | Fast, small binary encoding for model blobs; avoids JSON bloat |
| Unicode / bidi | **`unicode-segmentation`**, **`unicode-bidi`**, `unicode-normalization` | Grapheme clusters, RTL/bidirectional support (BR-53), NFC/NFD normalization |
| Concurrency | `crossbeam` channels (shell-worker handoff) | Lock-free/bounded queues off the input path |
| Panic/FFI safety | `std::panic::catch_unwind` at every FFI boundary | Panics never cross into the JVM/IME (BR-29) |
| Supply-chain | `cargo-deny`, `cargo-audit` (CI) | License + advisory gating (BR-65) |

### 3.3 Android Platform APIs

| Concern | API | Serves |
|---|---|---|
| IME lifecycle | `InputMethodService`, `InputConnection`, `EditorInfo`, `InputMethodManager` | BR-29–31 |
| Rendering | Hardware-accelerated `Canvas`, `Choreographer` (frame timing), `VelocityTracker` (gestures) | BR-1, BR-41 |
| Secure key storage | **Android Keystore** (hardware-backed / StrongBox where available), `KeyGenParameterSpec` | BR-62 |
| At-rest (shell side) | Jetpack Security `EncryptedSharedPreferences`, `DataStore` | BR-62 |
| Backup exclusion | `dataExtractionRules` (Android 12+) / `fullBackupContent`; `android:allowBackup` scoping | BR-63 |
| Accessibility | Accessibility framework, **TalkBack**, `AccessibilityNodeInfo`, `sendAccessibilityEvent` | BR-55, BR-56 |
| Sensitive fields | `EditorInfo.inputType` masks, `IME_FLAG_NO_PERSONALIZED_LEARNING`, autofill flags | BR-26 |
| Haptics/sound | `HapticFeedbackConstants`, `Vibrator`/`VibratorManager`, `AudioManager` keypress sounds | BR-52 |
| Clipboard | `ClipboardManager` (system), with our own encrypted history store | BR-50 |

### 3.4 Build & Tooling

| Concern | Tool |
|---|---|
| Android build | **Gradle** (Kotlin DSL), Android Gradle Plugin |
| Rust build & NDK cross-compile | **Cargo** + **`cargo-ndk`** / **`rust-android-gradle`**; ABIs: `arm64-v8a`, `armeabi-v7a`, `x86_64` |
| Min / target SDK | **minSdk 26 (Android 8.0)**, targetSdk = latest stable (see ADR-6) |
| Size control | ABI splits / Android App Bundle; R8 full-mode shrinking + resource shrinking; Rust `panic=abort`, `opt-level="z"`, `codegen-units=1`, thin LTO, `strip=symbols` |
| Static analysis | `clippy` (Rust), `detekt`/`ktlint` (Kotlin), Android Lint |
| CI | GitHub Actions (or equivalent) with a device/emulator matrix |
| Reproducible builds | Deterministic toolchain pinning; SOURCE_DATE_EPOCH; F-Droid-compatible build (BR-24, BR-67) |

### 3.5 Internationalization & Linguistic Resources

- **App UI strings:** standard Android resource localization (`res/values-<locale>/`), with RTL layout mirroring (`android:supportsRtl="true"`).
- **Typing resources (the reused OSS data per ADR-1):** per-language word frequency lists and n-gram tables compiled into `fst`/binary assets at build time; layout definitions (QWERTY/AZERTY/QWERTZ/…, plus RTL and locale variants) as declarative data files. Sources vetted for permissive licensing (§16 open item).
- **Build-time compilation:** a `build.rs`/Gradle task compiles raw word lists → compact `fst` + quantized n-gram blobs, so shipped assets are already in the runtime format (no on-device conversion cost).

### 3.6 Module Wiring & Dependency Management

- The Rust core exposes one **façade crate** (`featherkey-core`) that composes the internal crates and presents the UniFFI-typed API; the shell depends only on the façade, never on internal crates directly (preserves EP-3 boundaries).
- Kotlin-side wiring is **manual/constructor injection** (no heavy DI framework, for footprint); a light service-locator seam allows test doubles.
- Internal crate graph is enforced **acyclic** by a CI check.

### 3.7 Logging & Observability (privacy-safe)

- **No third-party analytics/crash SDK** (Firebase/Crashlytics/etc.) — would violate EP-1/EP-8.
- Rust core uses `tracing` with a custom subscriber that writes only to the in-memory `diagnostics` ring buffer (no content, no PII); nothing is emitted off-device unless the user explicitly exports (BR-60, BR-61).
- Debug builds may log verbosely to logcat; **release builds compile out** content-bearing logs via feature flags.

---

## 4. System Architecture

### 4.1 Layered View

```
┌──────────────────────────────────────────────────────────────┐
│                     ANDROID SHELL (Kotlin)                     │
│  ime-service · keyboard-view · settings-ui · onboarding        │
│  accessibility-adapter · platform-services · ffi-bridge        │
└───────────────────────────▲──────────────────────────────────┘
                            │  UniFFI (typed, safe)  │  JNI (hot path only)
┌───────────────────────────▼──────────────────────────────────┐
│                       FEATHERKEY CORE (Rust)                   │
│  input-decoder · touch-model · layout-engine · locale-manager  │
│  dictionary · prediction · autocorrect · personalization       │
│  gesture · smart-typing · editing · clipboard-core             │
│  sensitive-context · secure-store · crash-guard · diagnostics  │
│  neural-runtime (v1.x) · dictation (v2+)                       │
└──────────────────────────────────────────────────────────────┘
```

**Rationale for the split:** the Kotlin shell owns only what *must* be Android-specific (IME lifecycle, rendering, system APIs). All logic that is performance-critical, security-critical, or benefits from portability/testability lives in the Rust core, which is pure logic with no Android dependency and can be unit-tested and fuzzed on the host. This directly serves EP-3, EP-4, and EP-5.

### 4.2 Process & Memory Model

- The IME runs **in its own process** as a bound system service (`InputMethodService`), separate from the host app being typed into — the OS guarantees this isolation. The keyboard therefore cannot read the host app's memory, and vice versa.
- **Single process, few threads** (see §4.3) — no background services when the keyboard is dismissed (EP-2: zero idle cost).
- The Rust core is loaded as a native library into the IME process; it holds the models memory-mapped (`fst` assets) so read-only language data is shared via the page cache and not duplicated on the heap.
- **Memory pressure:** on `onTrimMemory`/low-memory signals, the core drops caches (recomputable) and keeps only the minimal working set; models are re-`mmap`ped lazily.

### 4.3 Threading & Data-Flow Model

| Thread | Owns | Rules |
|---|---|---|
| **IME/main thread** | Touch intake, frame render, `InputConnection` commits, UI | Must fit the frame budget (§8.1); no blocking I/O, crypto, or network — ever |
| **Decode worker** (high priority) | `input-decoder`, `prediction`, `autocorrect` when they don't fit inline | Results posted back to main within the same/next frame; bounded queue drops stale requests |
| **Background executor** (single, low priority) | `personalization` updates, `secure-store` writes, `diagnostics` flush | Fully async; never on the input path; batched to spare battery |
| **Watchdog** (lightweight timer) | Liveness checks on the input view (`crash-guard`) | Triggers re-init/safe mode if the main thread stalls |

- **Decode path:** touch → `input-decoder` runs **inline** when it fits the budget (target < 5 ms, common case); heavier work (large candidate sets, neural inference) goes to the decode worker.
- **Back-pressure:** the decode-request queue is bounded and coalescing — if the user out-types the worker, stale in-flight requests are dropped rather than queued, so suggestions never lag behind the caret.
- **Golden rule:** the input path is allocation-light and lock-free where feasible; persistence and crypto are always asynchronous; shared state uses immutable snapshots handed to the worker (no locks on the hot path).

### 4.4 IME Lifecycle States

The shell models the IME as an explicit state machine so lifecycle bugs (a common source of P-4 crashes) are impossible to reach silently:

```
DISABLED ──enable──► ENABLED ──select as default──► ACTIVE
   ▲                                                  │
   │                              onStartInput ◄──────┤
   │                                   │              │
   └──────────────── uninstall    ┌────▼─────┐   onFinishInput
                                   │ TYPING   │◄───────┘
                                   └────┬─────┘
                                        │ unrecoverable error
                                   ┌────▼─────┐
                                   │ SAFE_MODE│  (always renders a basic keyboard)
                                   └──────────┘
```

- `onStartInput`/`onFinishInput` bind/unbind the editor; `SAFE_MODE` is reachable from any state and always yields a usable keyboard (BR-30).

### 4.5 Representative Sequence Flows

**A. Keystroke (fast path)** — see §5.3.

**B. Concurrent multi-language word (BR-16, BR-18, BR-19b)**
1. User (Maria) types a Portuguese word mid-English sentence.
2. `locale-manager` scores the in-progress token against each active language's model → picks `pt` for this word without any manual switch.
3. `dictionary`/`prediction`/`autocorrect` use the `pt` resources; the English model stays loaded for the next token.
4. No UI flicker, no "loading," no language toggle — the switch is invisible.

**C. Swipe / slide-to-type (BR-41)**
1. `keyboard-view` collects the gesture path via `VelocityTracker`.
2. `gesture` scores the path against the active lexicons → ranked word candidates.
3. Top candidate committed; alternatives shown in the suggestion strip; `personalization` notes the choice — asynchronously.

**D. Crash recovery (BR-29–31)**
1. A bug triggers a panic inside `prediction`.
2. `catch_unwind` at the FFI seam converts it to a typed error; the JVM never sees an unwind.
3. `ime-service` catches the error, disables only the prediction feature for this session, and keeps typing working.
4. If the failure is in a core-critical path, the watchdog flips to `SAFE_MODE` — a basic but fully working keyboard — with **no restart** and no user-visible crash.

---

## 5. Module Decomposition

Every module has **one responsibility** (BR-38). Each entry lists its job, the requirements it serves, and its key technology. Modules communicate through narrow, typed interfaces.

### 5.1 Android Shell Modules (Kotlin)

| Module | Single responsibility | Serves | Key tech |
|---|---|---|---|
| `ime-service` | `InputMethodService` lifecycle; bind editor; commit text/cursor ops | BR-29–31 | `InputMethodService`, `InputConnection` |
| `keyboard-view` | Render the keyboard; capture touch; animations | BR-1, BR-32–34 | Custom `View` + `Canvas` |
| `settings-ui` | Configuration screens (dictionary, autocorrect, theme, haptics, consent) | BR-9, BR-14, BR-15, BR-22, BR-36, BR-52 | Jetpack Compose |
| `onboarding` | First-run; the IME-enablement **trust** flow | BR-35, BR-58 | Compose |
| `accessibility-adapter` | Expose keys/actions to TalkBack & switch access | BR-37, BR-55, BR-56 | Accessibility framework |
| `platform-services` | Keystore access, storage paths, system clipboard, backup exclusion | BR-62 (key provisioning), BR-63 | Android Keystore, backup rules |
| `ffi-bridge` | Marshal calls between Kotlin and the Rust core | (glue) | UniFFI / JNI |

### 5.2 Rust Core Modules

| Module (crate) | Single responsibility | Serves | Key tech |
|---|---|---|---|
| `kernel` | Define shared value objects and error types that cross module boundaries (no logic, no dependencies) | (all) | Plain Rust types |
| `contracts` | Define the port traits (driven & driving) that domain crates depend on instead of adapters (no logic, no dependencies) | (all) | Plain Rust traits |
| `input-decoder` | Map touch coordinates + key geometry → intended key & candidate set (the accuracy engine) | BR-5, BR-6, BR-46 | Geometry + probabilistic scoring |
| `touch-model` | Per-user adaptive model of tap distributions; improves targeting over time | BR-7, BR-46 | On-device incremental learning |
| `layout-engine` | Key layouts & geometry: alpha, number/symbol, RTL, ergonomic variants | BR-47, BR-51, BR-53 | Data-driven layout defs |
| `locale-manager` | Active languages; concurrent multi-language; per-word language detection | BR-16, BR-17, BR-18, BR-19, BR-19a, BR-19b | Language-ID scoring |
| `dictionary` | Compact per-language lexicons; prefix/fuzzy lookup | BR-10, BR-12 | `fst` |
| `prediction` | Autocomplete & next-word; inline predictions | BR-10, BR-11, BR-42 | n-gram (MVP) + neural (v1.x) |
| `autocorrect` | Correction candidates; alternative-word choices; **no-clobber** policy | BR-12, BR-15, BR-45 | Edit-distance + LM scoring |
| `personalization` | Learn user vocabulary/habits; user dictionary; whitelist; import | BR-7, BR-9, BR-11, BR-13, BR-14, BR-57 | Incremental counts, `secure-store` |
| `gesture` | Decode swipe/slide-to-type paths → words | BR-41 | Path scoring vs lexicon |
| `smart-typing` | Auto-capitalization, double-space-period, smart punctuation | BR-48 | Rule engine, per-locale |
| `editing` | Cursor movement & text-selection operations | BR-49 | Cursor state machine |
| `clipboard-core` | Clipboard history model; sensitive exclusion; auto-expiry | BR-50 | `secure-store` (encrypted) |
| `sensitive-context` | Detect password/sensitive fields; suppress learning/prediction | BR-26 | `EditorInfo` flags via shell |
| `secure-store` | Encrypted persistence of all personal data; key-use interface | BR-8, BR-23, BR-62 (at-rest encryption) | `redb` + AES-256-GCM |
| `crash-guard` | Isolate panics at FFI; watchdog; safe-mode fallback keyboard | BR-29, BR-30, BR-31 | `catch_unwind`, watchdog |
| `diagnostics` | Opt-in, content-free local diagnostics ring buffer; user-exportable | BR-60, BR-61 | In-memory ring + optional export |
| `neural-runtime` *(v1.x)* | Small neural model inference for prediction/decoding | BR-11 | `tract`/`candle` |
| `dictation` *(v2+)* | Optional voice-to-text, privacy-preserving | BR-43 | On-device / consented only |
| `featherkey-core` | Composition façade: wire concrete adapters to ports at the composition root, compose the core crates, and present the UniFFI-typed API to the shell | (glue) | Manual DI, UniFFI |

> **On BR-38 (single-responsibility modules):** this is a **structural** requirement satisfied by the whole decomposition and enforced by fitness functions (EP-3, §5.5, ARCH §13), not owned by any single crate — hence `kernel`/`contracts` show `(all)` above and §15 lists `§5, EP-3` as its owner.

### 5.3 Module Interaction — Keystroke Example

1. `keyboard-view` captures a touch → hands coordinates to `ffi-bridge`.
2. `input-decoder` (with `touch-model` + `layout-engine`) produces the intended key + candidates.
3. `locale-manager` tags the active language(s); `dictionary` + `prediction` + `autocorrect` produce suggestions, unless `sensitive-context` says this is a password field.
4. Result returns through `ffi-bridge`; `ime-service` commits text; `keyboard-view` renders suggestions.
5. Asynchronously, `personalization` updates the user model and `secure-store` persists it — off the input path.
6. `crash-guard` wraps steps 2–3; any panic yields a safe fallback rather than a crash.

### 5.4 Interface Sketches (illustrative)

Narrow, typed interfaces make the single-responsibility boundaries concrete. Signatures below are illustrative Rust (final API pinned in code), showing the *shape* of each contract, not implementation.

```rust
// input-decoder — pure function of geometry + touch; no I/O, no state mutation
pub trait InputDecoder {
    fn decode(&self, touch: TouchEvent, layout: &Layout, model: &TouchModel)
        -> KeyCandidates; // ranked keys with confidence
}

// prediction — stable interface; statistical impl at MVP, neural impl in v1.x, same signature
pub trait Predictor {
    fn suggest(&self, ctx: &TypingContext, langs: &ActiveLanguages)
        -> Suggestions; // completions + next-word, ranked
}

// autocorrect — MUST never clobber an exact/whitelisted word (BR-12)
pub trait AutoCorrect {
    fn correct(&self, token: &Token, ctx: &TypingContext)
        -> Correction; // { primary, alternatives[], applied: bool }
}

// personalization — the only writer of *lexical* learned state (tap-geometry is owned by touch-model); sensitive contexts excluded upstream
pub trait Personalization {
    fn observe(&mut self, event: TypingEvent);        // async, off input path
    fn export(&self) -> UserModelSnapshot;            // for user view/reset (BR-9)
    fn import(&mut self, other: ForeignDictionary);   // migration (BR-57)
}

// secure-store — the only component that touches persistence/crypto
pub trait SecureStore {
    fn put(&self, ns: Namespace, key: &[u8], val: &[u8]) -> Result<()>; // encrypts
    fn get(&self, ns: Namespace, key: &[u8]) -> Result<Option<Vec<u8>>>;
}
```

**Boundary invariants enforced by these interfaces:**
- `input-decoder` and `Predictor::suggest` are **read-only** — they cannot mutate learned state or persist, so the hot path has no write/crypto cost.
- `personalization` is the **sole writer** of *lexical* learned data and `touch-model` the **sole writer** of the tap-geometry model (one writer per data domain, ADR-14); `secure-store` is the **sole** component that encrypts/persists. Nothing else can leak or corrupt personal data.
- `sensitive-context` gates calls *before* they reach `personalization`/`prediction`, so password fields structurally cannot be learned (BR-26).

### 5.5 Module Design Rules

1. **One writer per data domain.** Exactly one module owns mutation of each kind of state. Learned state is split by domain: **lexical/vocabulary learning** (user dictionary, whitelist, personal n-grams) is owned solely by `personalization`; the **per-user tap-geometry model** is a separate data domain owned solely by `touch-model` (ADR-14). Persistence/crypto is owned solely by `secure-store`. Others read immutable snapshots.
2. **No Android types in the core.** Rust crates never import Android APIs; the shell adapts. This keeps the core host-testable and portable (EP-3).
3. **Errors are values, not panics.** Core functions return `Result`; panics are reserved for truly unreachable states and are caught at the FFI seam (EP-6).
4. **Acyclic dependencies.** The crate graph is a DAG; the façade composes, internal crates don't cross-import. CI-enforced.
5. **Feature-flag the optional weight.** `neural-runtime`, `dictation` are Cargo features off by default so MVP builds exclude them entirely (EP-2).

> **On "one thing only" (BR-38):** a few modules group tightly-cohesive responsibilities behind one owner — e.g., `keyboard-view` (render + touch capture + press animation) and `personalization` (learn + user-dictionary + whitelist + import). These are deliberately kept together because they share state and a single reason-to-change; splitting them would create chatty cross-module coupling that works *against* modularity. "One responsibility" is applied at the level of *cohesive domain*, not literal single function. Where a grouping later grows a second reason-to-change, it is split.

---

## 6. Cross-Cutting Concerns

### 6.1 Multilingual Concurrency (BR-16–19b)

- `locale-manager` holds an **ordered set of active languages** (≥2 at MVP, architected toward 3). All active languages' lexicons/n-grams are loaded simultaneously (memory-mapped, so cheap).
- **Per-word language identification:** as a token forms, it is scored against each active language's model; the highest-scoring language supplies the dictionary, prediction, and autocorrect for that token. This yields code-switching *without* a manual toggle (the iOS behavior we match).
- **Tie-breaking & stickiness:** short/ambiguous tokens keep the previous word's language to avoid flip-flopping; confidence hysteresis prevents jitter.
- **Manual switch (BR-17)** remains available (globe key / spacebar swipe) and is instantaneous because all languages are already loaded — no load, no "ready" delay.
- **Footprint guard:** number of concurrently loaded languages is bounded; adding a language beyond the bound prompts the user rather than silently ballooning memory (EP-2).

### 6.2 Scripts & Bidirectional Text (BR-53, BR-54)

- **RTL/bidi at MVP** for any shipped RTL language: `unicode-bidi` resolves the visual order; `keyboard-view` mirrors layout; the caret and text-editing (`editing`) respect bidi runs so cursor movement is logical, not visual-only.
- **Grapheme correctness:** all editing/deletion operates on grapheme clusters (`unicode-segmentation`), so emoji and combining marks delete as one unit.
- **Complex scripts (v2+):** Indic reordering and CJK conversion are added as **pluggable input-method modules** behind the same `Predictor`/decoder interfaces, so the core need not change to gain them.

### 6.3 Accessibility (BR-55, BR-56)

- **MVP:** `accessibility-adapter` exposes each key as an `AccessibilityNodeInfo` with proper labels and actions, and announces key/gesture results, so **TalkBack** users can type; explore-by-touch supported.
- **v1.x:** high-contrast theme, large/adjustable key sizing, and compatibility with **switch access** and other accessibility input services for motor-impaired users.
- **Tested**, not assumed — see §12 (accessibility test tooling).

### 6.4 Theming & Design System (BR-32, BR-33, BR-36)

- A **token-based design system** (color, spacing, type, motion, key shape, elevation) drives `keyboard-view`; light and dark are first-class, with premium defaults meeting the iOS-class bar.
- Motion tokens define the smoothness budget (press feedback, popup, transitions) so "smooth" is specified, not left to taste.
- Theme changes are data, not code — new themes ship without touching the render path.

### 6.5 Offline Guarantee (BR-59, BR-23)

- The core links **no networking library**; the base app declares **no `INTERNET` permission**. Enforced by the CI manifest/dependency check (EP-1).
- Any future networked capability (e.g., optional cloud sync, v2+) is a **separate, independently-permissioned, opt-in module** the user must explicitly enable — the default product is fully functional airplane-mode.

### 6.6 Configuration & Feature Flags

- Runtime user settings live in `DataStore`/`EncryptedSharedPreferences`; compile-time capability flags are Cargo features (§5.5 rule 5).
- No remote config (that would imply a network dependency and a control channel — both forbidden by EP-1). All behavior is determined by the shipped build + local settings.

---

## 7. Data Model & Storage

### 7.1 What we store (all on-device, all encrypted at rest — BR-62)

| Data | Store | Encryption | Backup |
|---|---|---|---|
| User dictionary / whitelist | `redb` table | AES-256-GCM, key in Keystore | **Excluded** (BR-63) |
| Learned touch model | Binary blob in `secure-store` | AES-256-GCM | **Excluded** |
| Learned language model (personal n-grams) | `secure-store` | AES-256-GCM | **Excluded** |
| Clipboard history | `redb` table, TTL'd | AES-256-GCM | **Excluded** |
| Settings/preferences | Jetpack `DataStore` (non-sensitive) or `EncryptedSharedPreferences` (sensitive) | Platform | Configurable |
| Bundled dictionaries/layouts | Read-only assets | n/a (public data) | n/a |

### 7.2 Logical schema (per `redb` namespace)

| Namespace | Key | Value (decrypted) | Notes |
|---|---|---|---|
| `user_dict` | normalized word | `{ locale, freq, added_at, source: user\|imported }` | Whitelist = words the user confirmed; never autocorrected away |
| `touch_model` | layout-id | serialized adaptive tap-distribution params | Small, per-layout |
| `personal_lm` | context-hash | quantized n-gram counts | Personal vocabulary/habits |
| `clipboard` | monotonic-id | `{ content, created_at, ttl, pinned }` | TTL-swept; sensitive entries never inserted |
| `meta` | `schema_version` | integer | Drives migrations |

All values are stored **encrypted** (AES-256-GCM); keys/namespaces are opaque. Content is never indexed in plaintext.

### 7.3 Key management

- Data-encryption keys are generated in and never leave the **Android Keystore** (hardware-backed / StrongBox when available), created with `KeyGenParameterSpec` (no auth-bound requirement so typing works on the lock screen where the OS permits the IME).
- `secure-store` uses a **Keystore-wrapped data key**: the master key lives in Keystore; a per-namespace data key is wrapped/unwrapped on demand. Plaintext key material is `zeroize`d after use and never persisted in app storage.
- **Password/sensitive contexts** (`sensitive-context`) are never learned, stored, or predicted (BR-26) — enforced upstream of `personalization`.

### 7.4 Retention, deletion & user control (BR-9, BR-22)

- **User-initiated:** Settings offers "view learned words," "reset learning," and "delete all my data" — the last wipes every personal namespace and rotates keys.
- **Clipboard:** entries auto-expire by TTL (default short; user-configurable) and can be cleared instantly; pinned entries are explicit opt-in.
- **Uninstall:** all app-private storage is removed by the OS; because personal data is excluded from backup (BR-63), nothing survives in the cloud.

### 7.5 Schema migration

- `meta.schema_version` gates migrations; the core runs forward-only, **fail-safe** migrations on load.
- If a store is unreadable or a migration fails, `secure-store` **quarantines** the corrupt data and rebuilds an empty store rather than crashing (ties into §11 self-healing) — learning resets, typing never breaks.

### 7.6 Storage-size budget

- Bundled read-only assets (dictionaries/n-grams/layouts) dominate size and are counted against the APK footprint budget (§8.2); personal stores are tiny and grow slowly, with caps and LRU eviction on the personal LM to bound on-device growth.

---

## 8. Performance Engineering

Serves BR-1, BR-2, BR-4, BR-40, BR-46; OBJ-2, OBJ-7.

### 8.1 Latency budget

| Path | Target |
|---|---|
| Touch → glyph committed | **< 20 ms** end-to-end |
| Frame render | Within one refresh interval (**8.3 ms @120 Hz**, 16.7 ms @60 Hz) |
| Keyboard first-appearance (warm) | **< 50 ms**, no visible "loading" |
| Inline decode (typical) | **< 5 ms** on the input thread |

**Must-beat target (BR-46):** correctness under fast typing — no dropped/mis-ordered keystrokes — is a launch gate, explicitly benchmarked, because it is exactly the regression iOS 26 shipped.

### 8.2 Footprint budget (enforced in CI — EP-2)

| Metric | Target (per-ABI) |
|---|---|
| APK download size | **< 10 MB** (ABI-split); app bundle preferred |
| Idle RAM | Minimal resident set; no background service when keyboard is closed |
| Battery | No wakelocks; zero network; work bounded to active typing |

### 8.3 Techniques

- **Allocation-light input path:** pre-allocated, reused buffers for candidates/suggestions; no per-keystroke heap churn; arena/scratch buffers for decode.
- **Compact models:** FST dictionaries (memory-mapped) + quantized, compressed n-grams keep both RAM and APK small.
- **Lock-free hot path:** the decode worker reads immutable model snapshots; no locks between input and learning.
- **Rendering:** minimize overdraw; dirty-rect invalidation; cache glyph/key bitmaps; drive frames off `Choreographer`; avoid layout passes during typing.
- **Rust release profile:** `opt-level="z"`, thin LTO, `codegen-units=1`, `panic=abort`, `strip=symbols`.
- **Startup:** lazy-load non-critical models; memory-map assets rather than parse; defer neural-runtime init until first use (v1.x).

### 8.4 Measurement Methodology

Budgets are meaningless without measurement. We measure at three levels:

| Level | How | Gate |
|---|---|---|
| Micro | `criterion` benchmarks on core functions (decode, suggest, correct) on host + device | CI fails on >X% regression vs baseline |
| Frame | `Choreographer` frame timing + jank stats on device instrumentation | Track dropped frames during scripted typing |
| End-to-end | Instrumented touch-injection → measured commit latency on the device matrix | p50/p95/p99 latency tracked over time |

- **Percentiles, not averages:** p95/p99 latency is what users feel; budgets are stated at p95.
- **Fast-typing harness (BR-46):** a scripted rapid-input test asserts zero dropped/reordered keystrokes — the explicit must-beat-iOS-26 gate.
- Baselines are stored; regressions block merge.

### 8.5 Cold-Start Budget Breakdown

Target warm appearance < 50 ms (BR-2); cold start (first invocation after process spawn) is budgeted by phase:

| Phase | Budget | Technique |
|---|---|---|
| Native lib load + core init | small | Minimal static init; `mmap` not parse |
| Layout + theme ready | small | Precompiled layout data |
| First frame drawn | within budget | Cached key bitmaps |
| Models available | deferred | Alpha typing works before neural/full models finish loading |

- The keyboard is **interactive before every model is loaded** — basic typing never waits on prediction assets.

---

## 9. Security Architecture

Serves BR-25–28, BR-62–66; OBJ-4.

### 9.1 Threat model

Primary adversary from the BRD: **a compromised device / malware attempting to use the keyboard as a data-collection channel** (P-9). Secondary: supply-chain compromise; data-at-rest extraction; a malicious host app.

**Trust boundaries:**

| Boundary | Trusted side | Untrusted side | Control |
|---|---|---|---|
| IME process ↔ host app | Our IME | The app being typed into | OS process isolation; we expose nothing; we don't read host memory |
| Shell (Kotlin) ↔ core (Rust) | Both ours | — | Typed FFI; `catch_unwind`; input validation at the seam |
| Core ↔ persisted data | Core logic | On-disk bytes (could be tampered if device rooted) | Authenticated encryption (GCM) detects tampering |
| App ↔ bundled assets | Runtime | Imported dictionaries / layout data | Parsed defensively; fuzzed (§12) |
| Build ↔ dependencies | Our source | Third-party crates/libs | `cargo-deny`/`cargo-audit`, pinning, SBOM |

**STRIDE analysis (summary):**

| Threat | Example | Mitigation |
|---|---|---|
| **S**poofing | Malware poses as our IME | Signed releases; users enable a specific, verifiable IME |
| **T**ampering | Altered on-disk learned data | AES-GCM authentication → tamper detected, store quarantined (§7.5) |
| **R**epudiation | — (no accounts, no server logs by design) | N/A — we hold no user data to dispute |
| **I**nfo disclosure | Exfiltrate keystrokes | No network on core (EP-1); at-rest encryption; sensitive-field exclusion; **the headline threat, mitigated by design** |
| **D**enial of service | Crash the keyboard | Fail-soft/safe-mode (§11) — DoS degrades to basic typing, never a dead keyboard |
| **E**levation of privilege | Escape IME sandbox | Memory-safe Rust; minimal permissions; no native unsafe on input path |

### 9.2 Controls

| Control | Mechanism | Serves |
|---|---|---|
| Memory safety | Rust core; no unsafe C/C++ on the input path; `#![forbid(unsafe_code)]` where feasible | BR-25 |
| Minimal attack surface | No internet permission on core; least-privilege manifest, each permission justified | BR-27, BR-59 |
| Sensitive-input protection | `sensitive-context` disables learning/prediction/clipboard capture in password & secure fields | BR-26 |
| Encryption at rest | AES-256-GCM, Keystore-backed keys (§7) | BR-62 |
| Backup leak prevention | Learned/personal data excluded from cloud backup by default | BR-63 |
| Supply-chain hygiene | Minimal, pinned dependencies; `cargo-audit`/`cargo-deny` + Gradle dependency verification in CI; SBOM published | BR-65 |
| Fuzzing | `cargo-fuzz` on decoders/parsers (untrusted geometry, gesture paths, imported dictionaries) | BR-25 |
| Disclosure | Published vulnerability-disclosure policy; optional bug bounty | BR-64 |
| Secure updates | Signed releases via Play + F-Droid; reproducible builds let users verify | BR-66, BR-24 |

### 9.3 Vulnerability Disclosure & Incident Response (BR-64)

- **Published policy:** a `SECURITY.md` and security contact with a stated response SLA; safe-harbor language for good-faith researchers; optional bug bounty.
- **Response flow:** triage → reproduce → fix on a private branch → coordinated disclosure → expedited release across Play/F-Droid (§13) → public advisory + credit.
- **Because we hold no server-side user data**, the blast radius of most incidents is the on-device app itself — there is no central database to breach. This is a structural security advantage of the no-backend design.

### 9.4 Permissions Posture (BR-27)

- **Baseline app: no `INTERNET`, no contacts, no location, no storage-broad permissions.** The IME binding itself is the primary capability.
- Each additional permission (e.g., vibration for haptics, microphone *only if* dictation is enabled in v2+) is **feature-gated and justified in-manifest and in docs**; microphone is never requested unless the user turns on dictation.
- CI asserts the permission set against an allowlist so a dependency can't silently add one.

---

## 10. Privacy & Data-Handling Architecture

Serves BR-20–24, BR-59, BR-60, BR-61, BR-67; OBJ-3.

### 10.1 Principles in code

- **No keylogging, ever (BR-20):** typing content never leaves the device. There is no code path that transmits keystrokes; enforced by the no-internet-on-core design and reviewable in open source.
- **No default collection (BR-21):** ships with no analytics SDK. `diagnostics` is **opt-in**, **content-free**, and **on-device-first** — a local ring buffer the user may export, not an automatic uplink (BR-60, BR-61).
- **Full function offline (BR-23, BR-59):** typing, accuracy, learning, prediction, autocorrect all work with zero connectivity.
- **Transparency & control (BR-22):** plain-language consent screens; user can view/reset/delete learned data (`personalization` + `settings-ui`, BR-9).
- **Verifiability (BR-24, BR-67):** open-source + reproducible builds are the *mechanism* that makes every claim above checkable, not merely asserted.

### 10.2 Data inventory (what exists, where, and whether it can leave)

| Data | Exists? | Where | Leaves device? |
|---|---|---|---|
| Keystrokes / typed content | Transient | In-memory during composition only | **Never** |
| Learned dictionary / touch model / personal LM | Yes | Encrypted on-device (§7) | **Never** (backup-excluded, BR-63) |
| Clipboard history | Yes | Encrypted on-device, TTL'd | **Never** |
| Settings/preferences | Yes | On-device | Only via user-controlled OS backup if they opt in |
| Diagnostics (opt-in) | Only if enabled | In-memory ring buffer | **Only** on explicit user export (content-free) |
| Account / identity | **None** | — | N/A (no accounts, BR — no login) |

This inventory is the source for the truthful **Google Play Data-Safety declaration** (BRD §14): the honest answer is "no data collected, no data shared."

### 10.3 Consent & onboarding trust flow (BR-58)

The Android system shows a scary warning when any IME is enabled ("this keyboard may collect all the text you type"). We turn that into a trust moment:

1. Onboarding **pre-empts** the OS dialog with a plain-language explanation: *this warning is shown for every keyboard; here is why ours cannot do what it warns about — no internet permission, open source, verifiable.*
2. Links to the source repo and the reproducible-build verification.
3. Any optional data feature (diagnostics export) is **off** and requires an explicit, revocable opt-in with a clear description (BR-22).

### 10.4 Privacy by construction, not by policy

The guarantees above are enforced by **architecture** (no network on core, encryption, sensitive-field exclusion) and **verifiability** (open source + reproducible builds), not merely by a privacy policy. A user — or a security researcher — can confirm them independently. That is the whole differentiator versus the closed, telemetry-on-by-default incumbents (BRD §15.6).

---

## 11. Reliability & Failure Isolation

Serves BR-29, BR-30, BR-31; OBJ-8. This directly answers the BRD's most severe pain (P-4: silent crash forcing a phone restart).

### 11.1 Layered defense

1. **Panic isolation at the FFI boundary.** Every Rust→Kotlin call is wrapped in `catch_unwind`; a core panic returns a typed error, never unwinds into the JVM. The Rust core builds with `panic=abort` for release *except* at controlled FFI seams that convert panics to errors.
2. **Kotlin-side guard.** `ime-service` wraps core calls in structured error handling; an error triggers **safe mode** rather than a dead keyboard.
3. **Safe-mode fallback (`crash-guard`).** A minimal, dependency-free basic keyboard that is *always* renderable, so the user can always type — no restart required (BR-30).
4. **Watchdog.** Detects an unresponsive input view and re-initializes it in place.
5. **Self-healing persistence.** Corrupt learned-data stores are detected, quarantined, and rebuilt from empty rather than crashing (learning degrades, typing does not).

### 11.2 Failure taxonomy & responses

| Failure class | Example | Response | User impact |
|---|---|---|---|
| Recoverable feature fault | `prediction` panics | Feature disabled for the session; typing continues | Loses suggestions temporarily; no crash |
| Core-critical fault | decoder in a bad state | Watchdog → re-init core; if it recurs, `SAFE_MODE` | Brief; basic typing preserved |
| Data-store corruption | learned model unreadable | Quarantine + rebuild empty (§7.5) | Learning resets; typing unaffected |
| Resource exhaustion | OOM / low memory | Drop caches; shrink working set | Momentary; recovers |
| Unknown/last-resort | anything unhandled | `SAFE_MODE` basic keyboard | Always able to type; **no phone restart (BR-30)** |

- **No failure path ends in "keyboard gone."** Every branch terminates at a usable keyboard.
- **Idempotent recovery:** re-initialization is safe to run repeatedly; the watchdog can retry without side effects.

### 11.3 Watchdog details

- A lightweight timer checks that the input view responded within a bound after the last input event.
- On stall: capture a content-free diagnostic (opt-in), re-initialize the input view in place; escalate to `SAFE_MODE` only if re-init fails.
- The watchdog itself is minimal and dependency-free so it cannot be the thing that fails.

### 11.4 Reliability gate

MVP exit requires **zero** silent-failure/restart incidents across the test device matrix and soak/stress tests (aligns with the BRD MVP exit criteria). Fault-injection tests deliberately trigger each failure class above and assert the correct response and that the keyboard remains usable.

---

## 12. Testing Strategy

### 12.1 Philosophy

- **Push logic down to test it cheaply.** Because the core is Android-independent Rust, the bulk of behavior is covered by fast host-run unit/property tests — the expensive on-device tests focus on integration, latency, and accessibility.
- **Invariants over examples.** Safety-critical behavior (never clobber a whitelisted word; never learn in a password field; never emit to the network) is asserted as **property tests and CI checks**, not just example cases.
- **Every fixed bug gets a regression test.** Especially reliability faults (P-4 class).

### 12.2 Test layers

| Layer | Tooling | Targets |
|---|---|---|
| Rust unit tests | `cargo test` | Decoder, autocorrect, locale-ID, personalization logic |
| Property-based | `proptest` | Decoder/autocorrect invariants (e.g., never clobber an exact dictionary/whitelisted word — BR-12) |
| Fuzzing | `cargo-fuzz` | Untrusted inputs: gesture paths, imported dictionaries, layout defs (security) |
| Benchmarks | `criterion` | Latency & footprint regression gates (BR-1, BR-4, BR-46) |
| Kotlin unit | JUnit | Shell logic, FFI marshaling |
| Instrumented/UI | Espresso, UIAutomator | IME lifecycle, commit/cursor behavior, onboarding |
| Accessibility | `espresso-accessibility`, Accessibility Scanner | TalkBack/switch-access conformance (BR-55, BR-56) |
| Visual/design | Screenshot tests | Premium look-and-feel, light/dark parity (BR-32, BR-36) |
| Reliability | Soak/stress, fault-injection | No silent failure / no restart (BR-29–31) |
| Device matrix | Emulators + representative low/mid/high physical devices | Perf on low-to-mid range (BR-3) |

### 12.3 Key correctness invariants asserted by tests

- An exact dictionary or whitelisted word is *never* autocorrected away (BR-12, BR-13).
- Password/sensitive fields are never learned, predicted, or clipboard-captured (BR-26).
- No code path emits typing content over the network (verified by permission + dependency checks in CI, BR-20).
- Fast typing never drops or reorders keystrokes (BR-46) — the scripted rapid-input harness.
- Every failure-injection class leaves the keyboard usable (BR-29–31).
- Deleting "all my data" leaves no readable personal bytes (BR-9) — verified by post-wipe store inspection.
- Concurrent-language input applies the correct per-word language (BR-18, BR-19b) on a mixed-language corpus.

### 12.4 Device & configuration matrix

| Dimension | Coverage |
|---|---|
| API level | minSdk (26) · a mid target · latest |
| Device tier | low-end, mid-range, flagship (physical where possible) — validates BR-3 |
| Screen | phone sizes, high refresh (90/120 Hz), density variants |
| Locale/script | LTR, RTL (Arabic/Hebrew), a bilingual pair (e.g., pt+en) |
| Accessibility | TalkBack on, large font, high contrast |
| Theme | light / dark |

### 12.5 Coverage & quality targets

- **Minimum 98% line + branch coverage** on core logic crates (decoder, autocorrect, locale-ID, personalization) — the parts where correctness is safety-relevant. (Enforced per ARCH §7.3; trivial/generated code may be annotated as excluded so the bar drives real depth.)
- Property tests and fuzz targets run in CI (fuzz smoke per-PR, longer fuzz nightly).
- Latency/size benchmarks gate merges (§8.4).
- Accessibility scan must pass with no critical issues before release.

---

## 13. Build, CI/CD & Reproducibility

### 13.1 Pipeline stages

| Stage | Runs | Fails build if… |
|---|---|---|
| 1. Lint/format | `clippy`, `detekt`/`ktlint`, Android Lint | Style/lint errors, `unsafe` outside audited crates |
| 2. Unit + property | `cargo test`, `proptest`, JUnit | Any test/invariant fails |
| 3. Fuzz smoke | `cargo-fuzz` (short) | New crash found |
| 4. Supply-chain | `cargo-deny`, `cargo-audit`, Gradle dep verification, SBOM | Disallowed license, known advisory, unverified dep |
| 5. Permission/network guard | manifest + dependency scan | Core declares `INTERNET` or links networking (EP-1) |
| 6. Build | Rust core per-ABI (`cargo-ndk`) + Android app | Build/link error |
| 7. Instrumented | Espresso/UIAutomator + accessibility scan on emulator matrix | IME/lifecycle/a11y failures |
| 8. Benchmark & size gates | `criterion`, APK size check | Latency or footprint regression beyond threshold (§8) |
| 9. Reproducibility | clean rebuild + artifact diff | Non-deterministic output |
| 10. Sign & publish | Release signing | — |

### 13.2 Reproducible builds (BR-24, BR-67)

The technical backbone of the "verifiable privacy" promise:

- **Pinned toolchains:** exact Rust, NDK, Gradle, and dependency versions (lockfiles committed).
- **Determinism:** `SOURCE_DATE_EPOCH`, stripped/normalized timestamps and paths, sorted inputs, fixed `codegen-units` where needed.
- **Verification job:** CI rebuilds from a clean checkout and diffs against the published artifact; **F-Droid** independently builds from source, giving users a third-party attestation that the binary matches the code.
- **Why it matters:** a privacy claim you can't verify is just marketing (BRD §15.6). Reproducibility lets *anyone* confirm the shipped app is the audited source.

### 13.3 Release channels & signing

| Channel | Format | Notes |
|---|---|---|
| Google Play | App Bundle (per-ABI) | Truthful Data-Safety declaration = "no data collected/shared"; note Play re-signing when reasoning about reproducibility (§16) |
| F-Droid | Built from source | Natural home for privacy audience; independent reproducible build |
| Direct APK | Signed APK on project site | For advanced users; checksums + signature published |
| Source repo | — | Public, contributor-facing; reproducible-build instructions |

- Release signing keys held securely (HSM/hardware where possible); key rotation policy documented.

### 13.4 Versioning & branching

- **SemVer** for the app (`MAJOR.MINOR.PATCH`), aligned to the BRD roadmap phases (MVP = 1.0, parity = 1.x, breadth = 2+).
- Trunk-based development with short-lived feature branches; security fixes on a private branch until coordinated disclosure (§9.3).
- Every release is tagged and reproducible from that tag.

### 13.5 Supply-chain hygiene (BR-65)

- Minimal, pinned dependencies; new dependencies require review (license, maintenance, footprint, safety).
- SBOM published per release; `cargo-deny` enforces the license allowlist so nothing incompatible with our open-source license or F-Droid slips in.

---

## 14. Architecture Decision Records (ADRs)

### ADR-1 — Hybrid build base (greenfield core + reuse OSS data)
**Decision:** Build a greenfield, modular architecture; reuse permissively-licensed linguistic data (word lists, layouts) rather than recreating them.
**Why:** The differentiators — tiny/modular architecture (BR-38), iOS-class design (BR-32), and security control (BR-25) — require owning the architecture; forking an existing keyboard would inherit its design/polish gap (the very opportunity per BRD §15.6). Language *data*, however, is expensive to build and low-differentiation, so we reuse it.
**Alternatives:** Fully greenfield (slower, must build data); fork HeliBoard/FlorisBoard (fast but inherits architecture/tech-debt/design gap).
**Consequences:** We must vet licenses of reused data; we own more code but with a cleaner module boundary.

### ADR-2 — Rust core + Kotlin IME shell
**Decision:** Performance/security-critical logic in Rust; Android-specific shell in Kotlin; bridge via UniFFI (JNI for hot path if needed).
**Why:** A single choice satisfies three MUST pillars — memory safety (BR-25), latency (BR-1), footprint (BR-4).
**Alternatives:** Pure Kotlin (simpler, but GC/JIT latency + footprint cost, weaker safety story); Kotlin + C++ (mature but memory-unsafe, contradicting the security headline).
**Consequences:** FFI complexity and a cross-language build; mitigated by UniFFI and CI. Team needs Rust proficiency.

### ADR-3 — Hybrid, phased prediction engine
**Decision:** MVP = statistical (n-gram + FST) + a small neural touch/decoding model; add a pluggable neural LM in v1.x via `neural-runtime`.
**Why:** Keeps MVP tiny (BR-4, BR-40) and shippable while creating a clean seam for the accuracy upgrade that is the project's hardest bet (BRD Risk: on-device accuracy).
**Alternatives:** Statistical-only (tiniest, weaker prediction at launch); neural-from-start (best prediction, footprint/complexity risk).
**Consequences:** The MVP accuracy bar is deliberately **split** (and reflected in BRD §17.2 exit criteria): MVP beats iOS on **keystroke/touch accuracy** (BR-5, BR-6, BR-46) and ships **competitive, relevant** statistical prediction (BR-10); **beating iOS on AI prediction/autocorrect quality is a v1.x goal**, delivered by the neural LM. This avoids over-scoping MVP to a predictive-intelligence bar that a tiny statistical engine cannot be expected to clear. The neural module must fit the footprint budget when it lands, and MVP prediction quality is validated against the iOS bar early to size the gap.

### ADR-4 — Custom View rendering for the keyboard surface
**Decision:** Render the keyboard with a custom `View` + hardware-accelerated `Canvas`; use Compose only for settings/onboarding.
**Why:** The input surface is latency-critical (BR-1, BR-46); a custom View gives precise control over draw and overdraw. Compose is fine off the hot path.
**Alternatives:** Full Compose keyboard (simpler, but less latency control today); `SurfaceView`/dedicated render thread (added only if profiling requires).

### ADR-5 — Rust-native neural inference (`tract`/`candle`) over a bundled runtime
**Decision:** When the neural module lands, prefer a Rust-native inference crate over shipping a separate heavy runtime.
**Why:** Footprint (BR-4) and modularity (BR-38) — avoids a large extra native dependency.
**Alternatives:** LiteRT (Google) — very capable and hardware-accelerated, but a heavier dependency; revisit if on-device acceleration (NNAPI/GPU) proves necessary for the quality bar.

### ADR-6 — minSdk 26 (Android 8.0)
**Decision:** Support Android 8.0+; target the latest stable SDK.
**Why:** Balances reach on low-to-mid-range devices (BR-3) with access to modern Keystore/security APIs (BR-62).
**Alternatives:** Higher floor (fewer devices, stronger APIs) or lower floor (more devices, weaker security primitives). Revisit against launch-market device data (BRD Open Question: min device tier).

### ADR-7 — `redb` + app-layer AES-GCM for storage
**Decision:** Use the pure-Rust `redb` embedded KV with app-layer AES-256-GCM, keys in Keystore.
**Why:** No C dependency (footprint, safety, reproducibility), ACID, and encryption we fully control and can audit.
**Alternatives:** SQLCipher (C dependency, larger, but battle-tested encryption); `sled` (less mature persistence guarantees). Revisit if a relational query need emerges.

### ADR-8 — No third-party analytics/crash SDK
**Decision:** Ship zero analytics/crash SDKs; diagnostics are opt-in, content-free, on-device (`diagnostics` crate).
**Why:** Any such SDK would violate EP-1/EP-8 and the no-collection promise (BR-21), and would undermine the Data-Safety "no data collected" claim.
**Alternatives:** Privacy-respecting analytics (still a network dependency and a trust liability) — rejected.
**Consequences:** We give up convenient aggregate telemetry; we gain a defensible, verifiable privacy claim. KPI measurement must use the privacy-preserving methods in BR-60.

### ADR-9 — Manual DI over a framework
**Decision:** Wire modules with constructor injection / a light service locator; no Hilt/Dagger.
**Why:** Footprint and simplicity (EP-2); the module graph is small and explicit.
**Alternatives:** Hilt/Dagger (ergonomic at scale, but added size/build cost). Revisit only if wiring complexity grows materially.

### ADR-10 — Statistical language identification for concurrent languages
**Decision:** Per-word language selection via lightweight statistical scoring across active languages, with hysteresis.
**Why:** Fast, tiny, deterministic, works offline (BR-16, BR-18, BR-19b) without a heavy model.
**Alternatives:** Neural language-ID (heavier); explicit per-key language tagging (defeats the "no manual switching" goal).

### ADR-11 — Custom keypress feedback pipeline
**Decision:** Implement haptic/sound feedback via platform primitives (`HapticFeedbackConstants`, `Vibrator`, keypress sounds), user-configurable including fully off.
**Why:** Table-stakes tactile quality (BR-52) with zero added dependencies; respects battery and user preference.
**Alternatives:** Third-party haptics libraries (unnecessary footprint).

### ADR-12 — Port traits live in a dependency-free `contracts` crate
**Decision:** Put all port traits (`SecureStore`, `Predictor`, `AutoCorrect`, `SensitiveContextSource`, `KeyProvider`, …) in a new `contracts` crate — a peer of `kernel` with zero dependencies. Domain/application crates depend on `contracts` for the trait; adapters depend on it to implement.
**Why:** The Dependency Rule (ARCH §3.2, DIP) requires a domain crate to depend on a *port*, not an adapter — e.g. `personalization` needs `SecureStore` but must never see `secure-store`. A single port-only crate makes the legal edge the easy one. Kept separate from `kernel` because `kernel` is logic-free *data* while `contracts` is *behaviour* — two reasons to change (§5.5 r5).
**Alternatives:** Traits in `kernel` (merges data + behaviour, forces recompiles on any port change); a trait per consumer (no single home, breaks shared contract-tests, re-creates domain→adapter coupling).
**Consequences:** A second universal DAG sink beside `kernel` (`contracts` → nothing; everything → `contracts`); gives fitness rule **E-1** its concrete "port" layer. Serves BR-38, BR-39.

### ADR-13 — `locale-manager` depends on `dictionary` (Wave 2)
**Decision:** `locale-manager` depends on `dictionary`; it is a Wave-2 module, not a `kernel`-only leaf.
**Why:** Per-word language identification (ADR-10, §6.1) scores the in-progress token against *each active language's lexicon*, so locale scoring must read `dictionary`; §5.3 step 3 and the §5.4 `suggest(ctx, langs)` sketch imply the edge; §15 co-owns BR-16/BR-18 across these crates.
**Alternatives:** Invert the edge (puts language policy inside a pure lexicon leaf — wrong); a third scorer crate (needless module for a §6.1 relationship).
**Consequences:** Locks `dictionary` (W1) → `locale-manager` (W2) → `prediction` (W3). Serves BR-16, BR-18, BR-19b.

### ADR-14 — Two-domain writer split (`touch-model` vs `personalization`)
**Decision:** Split learned state into two single-writer domains: `touch-model` owns *only* the tap-geometry model (`touch_model` namespace, §7.2); `personalization` owns *only* lexical learning (user dictionary, whitelist, personal n-grams). "Sole writer" is scoped per-domain, not global.
**Why:** §5.5 r1 and the §5.4 invariant call `personalization` the *sole* writer of "learned data," yet §5.2/§15 give `touch-model` incremental tap learning (BR-7, BR-46) — a literal contradiction. Tap-geometry is a distinct domain (different namespace, consumer, reason-to-change) from vocabulary, so one-writer-per-domain preserves r1's intent while letting both learn.
**Alternatives:** Fold tap-geometry into `personalization` (merges unrelated concerns, SRP §4); keep `personalization` globally sole (forbids `touch-model` from learning, contradicts BR-7/46).
**Consequences:** Requires the §5.5 r1 + §5.4 rewrites made in this revision. Serves BR-7, BR-46.

### ADR-15 — `input-decoder::decode` gains a `model` parameter in Wave 2 (the one scheduled API break)
**Decision:** Evolve `decode(touch, layout)` → `decode(touch, layout, model: &TouchModel)` per §5.4 when `touch-model` lands (Wave 2), providing an unbiased default `TouchModel` so the Wave-0 tracer tests keep passing with identical behaviour. This is the **single** scheduled public-API break in the MVP plan.
**Why:** Wave 0 shipped `input-decoder` deliberately sans touch-model, so integrating it is *breaking*, not additive. Planning it as a known break with a neutral default keeps the tracer/BDD green and honours plan R3 (no unscheduled breaks) + DoD interface-fidelity to §5.4.
**Alternatives:** Add a second `decode_with_model` (permanent drift from the §5.4 single signature); break during shell integration (Wave 5) — ripples into live shell wiring (Risk R-1). Doing it in Wave 2, before any shell consumes the port, contains the blast radius.
**Consequences:** The Wave-0 "unbiased nearest-key" note becomes the default-model path. Serves BR-6, BR-7 (targeting), BR-46.

### ADR-16 — RTL/bidi (BR-53) deferred until the launch language set is fixed; port stays RTL-ready
**Decision:** Keep `layout-engine`'s port and layout-data model RTL-ready (`unicode-bidi` reserved, §6.2) but defer *implementing and shipping* bidi until the initial language set (§16 Q2 / BRD §18) is fixed. BR-53 stays **MVP\*** — conditional on an RTL language being in the launch set.
**Why:** §15 already marks BR-53 conditional; §16 Q2 lists the language set as unresolved. Building full bidi (visual reordering, caret logic across bidi runs, mirrored layouts) before knowing whether any RTL language ships is speculative against an undecided requirement; an RTL-ready port costs nothing now (OCP, ARCH §4).
**Alternatives:** Ship full bidi unconditionally at MVP (spends the hardest i18n budget on a maybe-language; violates principle precedence §1.1); drop RTL from the port (forces a breaking change if an RTL language is later added).
**Consequences:** Scopes Wave-1 `layout-engine` finalize to LTR + number/symbol/punctuation (BR-47); complex-script depth (BR-54) stays v2+. Trigger: resolution of §16 Q2 / BRD §18.

> **ADR status:** ADR-1/2/3 are **ratified** (sponsor-approved). ADR-4–11 and **ADR-12–16** are **proposed** — recorded here with rationale for engineering review, pending sponsor ratification; each can be revisited with data before implementation locks it in.

---

## 15. Requirement → Component Traceability

Every one of the BRD's 69 requirements is listed below on its own row with its **owning component(s)**, the **verification method** that proves it is met, and its **roadmap phase** (from BRD §17). This is the proof that the design covers the BRD completely and testably.

| BR | Requirement (short) | Owning component(s) | Verification method | Phase |
|---|---|---|---|---|
| BR-1 | Instant responsiveness (≤ iOS latency) | `keyboard-view`, `input-decoder`, §8.1 | e2e latency benchmark (p95 gate) | MVP |
| BR-2 | Fast keyboard appearance | §8.5 cold-start path | cold-start benchmark | MVP |
| BR-3 | Perf on low/mid-range devices | §8, §12.4 matrix | device-matrix benchmark | v1.x |
| BR-4 | Tiny footprint | EP-2, §8.2 | CI size gate | MVP |
| BR-5 | Register intended key | `input-decoder` | decoder accuracy test set | MVP |
| BR-6 | Consistent/decisive accuracy | `input-decoder`, `touch-model` | property test (stability) | MVP |
| BR-7 | Learn user's typing style | `touch-model`, `personalization` | learning-improvement test | MVP |
| BR-8 | On-device learning, zero egress | `secure-store`, §10 | network-guard CI check | MVP |
| BR-9 | View/reset/delete learned data | `personalization`, `settings-ui` | post-wipe store inspection | v1.x |
| BR-10 | Relevant autocomplete/prediction (*competitive* at MVP; beating iOS prediction quality is a v1.x goal via the neural LM — ADR-3) | `prediction`, `dictionary` | prediction-quality eval | MVP |
| BR-11 | Prediction improves with use | `prediction`, `personalization`, `neural-runtime` | longitudinal eval | v1.x |
| BR-12 | Never clobber intended word | `autocorrect` | **property test** (whitelist never overridden) | MVP |
| BR-13 | Trivial whitelisting | `personalization`, `settings-ui` | UI test | MVP |
| BR-14 | Review/edit/remove dict words | `personalization`, `settings-ui` | UI test | v1.x |
| BR-15 | Adjustable autocorrect | `autocorrect`, `settings-ui` | settings behavior test | v1.x |
| BR-16 | ≥2 concurrent languages | `locale-manager`, `dictionary` | mixed-language corpus test | MVP |
| BR-17 | Instant manual switch (near-free: active languages preloaded, §6.1) | `locale-manager`, `keyboard-view` | switch-latency test | MVP |
| BR-18 | Prediction across active langs | `locale-manager`, `prediction`, `autocorrect` | mixed corpus test | MVP |
| BR-19 | Architected for more languages | `locale-manager`, §3.5 | architecture review | v2+ (breadth) |
| BR-19a | Toward 3 concurrent languages | `locale-manager` | design review | v1.x |
| BR-19b | Auto per-word language detection | `locale-manager` (ADR-10) | per-word language test | MVP |
| BR-20 | Never keylog / transmit content | §10, EP-1 | network + permission guard CI | MVP |
| BR-21 | No default collection | §10, ADR-8 | dependency scan (no analytics SDK) | MVP |
| BR-22 | Transparency & consent control | `settings-ui`, `onboarding`, §10.3 | UI test | MVP |
| BR-23 | Core works with zero egress | EP-1, §10 | offline functional test | MVP |
| BR-24 | Independently verifiable | EP-7, §13.2 | reproducible-build CI job | v1.x |
| BR-25 | Security-hardened | Rust core, §9, `cargo-fuzz` | fuzzing + security review | MVP |
| BR-26 | Sensitive fields protected | `sensitive-context` | **property test** (never learn password) | MVP |
| BR-27 | Minimal permissions | manifest, §9.4 | permission-allowlist CI | MVP |
| BR-28 | Security review/threat modeling | process, §9, §13 | review gate | v1.x |
| BR-29 | No silent failures | `crash-guard`, `ime-service` | fault-injection test | MVP |
| BR-30 | Never require phone restart | `crash-guard` safe-mode | safe-mode reachability test | MVP |
| BR-31 | Available across states | `ime-service`, §4.4 | lifecycle test | MVP |
| BR-32 | Premium/beautiful design | `keyboard-view`, design system | screenshot test | MVP |
| BR-33 | Smooth/polished interactions | `keyboard-view`, §6.4 | frame/jank test | MVP |
| BR-34 | Comfortable key sizing | `keyboard-view`, `layout-engine` | layout test | MVP |
| BR-35 | Dead-simple setup/use | `onboarding` | usability test | MVP |
| BR-36 | Theming (light/dark) | `keyboard-view`, `settings-ui` | theme test | v1.x |
| BR-37 | Baseline accessibility | `accessibility-adapter` | accessibility scan | v1.x |
| BR-38 | Single-responsibility modules | §5, EP-3 | crate-graph DAG check | MVP |
| BR-39 | Modularity supports evolution | §5, EP-3 | architecture review | v1.x |
| BR-40 | Sustained tiny footprint | EP-2, §8.2 | CI size gate | MVP |
| BR-41 | Swipe/gesture typing | `gesture` | gesture accuracy test | v1.x |
| BR-42 | Inline predictive text | `prediction`, `keyboard-view` | UI test | v1.x |
| BR-43 | Privacy-preserving dictation | `dictation` | privacy test (on-device/consented) | v2+ |
| BR-44 | Fast emoji entry/search | `keyboard-view`, `layout-engine` | UI test | v2+ |
| BR-45 | Alternative-word autocorrect | `autocorrect` | autocorrect eval | v1.x |
| BR-46 | Must-beat iOS regressions (fast typing) | `input-decoder`, §8.1 | **fast-typing harness** (no drop/reorder) | MVP |
| BR-47 | Number/symbol/punctuation layouts | `layout-engine` | layout test | MVP |
| BR-48 | Smart-typing behaviors | `smart-typing` | rule tests (per-locale) | MVP |
| BR-49 | Cursor control & text editing | `editing` | editing tests | MVP |
| BR-50 | Clipboard history (sensitive-safe) | `clipboard-core`, `secure-store` | clipboard test (sensitive exclusion) | v1.x |
| BR-51 | Ergonomic/one-handed modes | `layout-engine`, `keyboard-view` | layout mode test | v1.x |
| BR-52 | Haptic/sound feedback | `keyboard-view`, `settings-ui` (ADR-11) | feedback test | v1.x |
| BR-53 | RTL/bidirectional text | `layout-engine`, `unicode-bidi`, §6.2 | bidi correctness test | MVP* |
| BR-54 | Complex-script architecture | `layout-engine`, §6.2 | architecture review | v2+ |
| BR-55 | TalkBack compatibility | `accessibility-adapter` | accessibility scan | MVP |
| BR-56 | High-contrast/large/switch-access | `accessibility-adapter` | accessibility test | v1.x |
| BR-57 | Import existing dictionary | `personalization`, `settings-ui` | import round-trip test | v1.x |
| BR-58 | Onboarding trust flow | `onboarding`, §10.3 | UI test | MVP |
| BR-59 | Fully offline core | EP-1, §10 | offline functional test | MVP |
| BR-60 | Privacy-preserving measurement principle | `diagnostics`, §10 | design review + no-network check | MVP |
| BR-61 | Opt-in, content-free diagnostics | `diagnostics` | opt-in gating test | v1.x |
| BR-62 | Encryption at rest | `secure-store` (at-rest), `platform-services` (keys), §7 | encryption test | MVP |
| BR-63 | Backup exclusion of personal data | `platform-services`, §7 | backup-rule test | MVP |
| BR-64 | Vulnerability disclosure policy | §9.3, process | policy exists (`SECURITY.md`) | v1.x |
| BR-65 | Supply-chain management | §9, §13.5 | `cargo-deny`/`audit` CI | v1.x |
| BR-66 | Secure, timely updates | §13.3 release | release-process check | MVP |
| BR-67 | Open-source | EP-7, §13.2 | public repo + reproducible build | MVP |

\* **BR-53** is MVP *only if* an RTL language is in the launch set (per BRD §17.2); otherwise it moves to the release that first ships an RTL language.

**Coverage guarantee:** every one of the BRD's 69 requirements (BR-1…BR-67 plus BR-19a/19b) appears above with a named owner. This mapping is mechanically checkable — a CI/doc script can diff the BR IDs defined in the BRD against those referenced here and fail if any requirement is unowned. No requirement is left without an implementing component.

---

## 16. Open Technical Questions

These require input before or during implementation; several mirror the BRD's Open Questions (BRD §18). Each has an explicit **decision trigger** — the point by which it must be resolved.

| # | Question | Why it matters | Options | Must decide by |
|---|---|---|---|---|
| Q1 | Min device tier / final `minSdk` (ADR-6) | Sets available security APIs vs device reach (BR-3, BR-62) | 26 / 29 / higher | Before first perf baseline |
| Q2 | Initial language set & concurrent pair(s) | Drives which dictionaries/layouts we source and test first (BR-16, BRD §18) | pt+en beachhead / other | Before dictionary asset build |
| Q3 | Neural LM footprint envelope | The size/quality trade for the v1.x model; must fit §8.2 | quantization level, model size cap | Before v1.x neural work starts |
| Q4 | Key-management detail | Wrapped keys vs per-op ephemeral; StrongBox availability (BR-62) | Keystore-wrapped data key (leaning) | Before `secure-store` impl |
| Q5 | Reproducible-build parity with Play | Play App Bundle re-signing complicates bit-for-bit (BR-24) | Verify via F-Droid + APK checksums; document Play caveat | Before first public release |
| Q6 | Design-system source | Build in-house vs adapt an existing token set (BR-32) | in-house / adapt OSS tokens | Before UI build ramps |
| Q7 | Reused-data licensing (ADR-1) | License compatibility with our OSS license + F-Droid | vet each source | Before shipping any bundled data |
| Q8 | Hardware acceleration need | Whether NNAPI/GPU is required to hit the accuracy bar within latency (BR-1 vs prediction quality) | CPU-only (leaning) vs accelerated | During v1.x neural evaluation |
| Q9 | On-screen accuracy tuning data | How to improve the touch model without collecting content (BR-60 tension) | on-device only / opt-in aggregate | During accuracy validation |

**Note:** Q8/Q9 touch the BRD's hardest bet (on-device accuracy at tiny footprint). They are flagged as the highest-risk technical unknowns and should be de-risked early with prototypes, per the BRD risk register (BRD §13).

---

## 17. Glossary

| Term | Definition |
|---|---|
| IME | Input Method Editor — Android's term for a keyboard app (`InputMethodService`) |
| FFI | Foreign Function Interface — the Rust ⇄ Kotlin boundary |
| UniFFI | Mozilla tool that generates safe cross-language bindings |
| JNI | Java Native Interface — lower-level native bridge, used only on the hot path |
| FST | Finite-State Transducer — compact structure for dictionaries/lexicons |
| n-gram | Statistical language model over word/character sequences |
| LM | Language Model (statistical or neural) |
| Keystore | Android's hardware-backed key storage |
| StrongBox | Dedicated secure-element backing for Keystore on supported devices |
| AES-256-GCM | Authenticated symmetric encryption used for data at rest |
| ABI | Application Binary Interface (CPU target, e.g., `arm64-v8a`) |
| SBOM | Software Bill of Materials |
| Safe mode | Minimal always-available fallback keyboard (reliability) |
| ADR | Architecture Decision Record |
| STRIDE | Threat-modeling taxonomy: Spoofing, Tampering, Repudiation, Information disclosure, DoS, Elevation of privilege |
| SemVer | Semantic Versioning (`MAJOR.MINOR.PATCH`) |
| NNAPI | Android Neural Networks API (on-device ML hardware acceleration) |
| StrongBox | Dedicated secure element backing Keystore on supported devices |
| `catch_unwind` | Rust mechanism to stop a panic at a boundary and convert it to an error |
| `mmap` | Memory-mapping a file so it is paged in on demand rather than fully loaded |
| Grapheme cluster | A user-perceived character (may be multiple Unicode code points) |
| Hysteresis | Resistance to rapid switching; used to stabilize per-word language selection |
| DAG | Directed Acyclic Graph — used to describe the module dependency graph |
| DSR | Data Subject Request (view/delete personal data) |
| p95/p99 | 95th/99th percentile — latency measured at the tail, not the average |
| Back-pressure | Dropping/coalescing stale work when a consumer can't keep up |

---

*End of Software Engineering Design Document (Draft v0.7 — expanded). Source of truth: BUSINESS_REQUIREMENTS.md (BRD v0.7).*
