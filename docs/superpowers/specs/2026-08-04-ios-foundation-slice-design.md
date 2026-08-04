# iOS Foundation Slice — Design

**Status:** design phase (gated by `/r-u-sure`).
**Slug:** `ios-foundation-slice`
**Date:** 2026-08-04

**Goal (one sentence):** Bring FeatherKey to iOS as a real-but-minimal keyboard
extension that types end-to-end through the **shared Rust core** on the iPhone
simulator, proving the FFI + extension architecture, with zero impact on the
shipped Android app.

---

## 1. Problem & scope

FeatherKey today ships only on Android. The entire typing engine already lives in
`core/` as platform-neutral Rust (host-testable, no Android types — fitness
enforced) and is exposed to Kotlin through a UniFFI proc-macro FFI surface
(`KeyboardCore`). The same FFI surface can emit **Swift** bindings
(`uniffi-bindgen-tool --language swift` — verified present). Nothing about iOS
requires re-deriving the engine; it requires a **thin Swift shell** analogous to
`apps/android/`.

This design covers **only the foundation slice**. Later, separately-gated slices
add the richer features (see §8 Deferred).

### In scope (this slice)

- A new `apps/ios/` Xcode project with three targets: a **Keyboard extension**
  (`UIInputViewController`), a minimal **host app** (required to install/enable an
  extension), and a **`FeatherKeyKit`** framework (the Swift adapter over the core).
- Rendering a QWERTY layout from the core's `layout_keys()`.
- Committing typed characters by routing taps → core `decode(x, y)` →
  `UITextDocumentProxy`. Keys: **letters, space, backspace, shift/caps**.
- A reproducible `apps/ios/build-core.sh` that builds the core for the iOS targets,
  packages a `FeatherKeyCore.xcframework`, and generates the Swift bindings.
- Provisioning the core's mandatory 32-byte device key (iOS **Keychain**) and an
  encrypted-store path (the extension's own container) — see §5.
- **Verification target:** iPhone simulator only (no Apple Developer account,
  provisioning, or signing).

### Out of scope (this slice — deferred, §8)

Swipe/gesture typing, suggestion strip, autocorrect-on-space, neural
re-ranker/LM, on-device **learning** + persistence wiring, numbers/symbols page,
long-press accents, voice, emoji, physical-device install, App Store/TestFlight.

---

## 2. Requirements closed

| BR | Statement | Status |
|---|---|---|
| **BR-70** (new) | FeatherKey ships as an iOS keyboard extension that **reuses the shared Rust core** — no reimplementation of typing logic in Swift. | Introduced by this slice; foundation established here, parity is later slices. |

**BRD amendment (per the agreed scope decision).** The BRD currently excludes
iOS as a deliverable:

- Line 134: *"Non-Android platforms (iOS, desktop, web) — reference targets only,
  not deliverables."*
- §8.10 (BR-41–46) uses iOS as a **feature benchmark for Android**, not a run target.

**Placement matters — BR-70 is a different axis from §8.10.** §8.10 means "match the
iOS keyboard's *features* on Android"; BR-70 means "*run on* iOS." Folding BR-70 into
§8.10 would conflate the two. This design therefore:

- amends line 134 to remove iOS from "not deliverables"; and
- adds a **new subsection §8.16 "iOS as a Delivery Platform"** holding **BR-70**
  (priority **S** — Should; the foundation is committed, full parity is later
  slices), with a traceability row (P-/OBJ- links to the modularity/reuse
  objective the shared-core approach serves).

The BRD edit is part of this gated phase.

> Precedence note (CLAUDE.md §7): the user's direct instruction to target iOS
> overrides the BRD's prior scope; rather than build against a contradictory
> source-of-truth, we record the new requirement explicitly.

---

## 3. Existing code consulted (CLAUDE.md §2)

Queried before designing; this slice **reuses**, does not duplicate:

| Existing | Role in this design |
|---|---|
| `core/` Rust engine (`FeatherKeyCore`) | The typing engine — **unchanged behavior**. |
| `core/crates/featherkey-core/src/ffi.rs` — `KeyboardCore` | The FFI surface iOS consumes. Methods used this slice: `open`, `layout_keys`, `decode`. Frozen — not modified. |
| UniFFI proc-macro setup (`build.rs`, `uniffi.toml`, `crate-type`) | Already emits Swift; reused as-is except the one additive `crate-type` touch (§4). |
| `core/tools/uniffi-bindgen-tool` | Generates the Swift bindings (already supports `--language swift`). |
| `apps/android/ffi-bridge/build-jni.sh` | Template for `apps/ios/build-core.sh`. |
| `apps/android/keyboard-view/.../KeyboardGeometry.kt` | Template for the pure, host-testable Swift coordinate/geometry helper. |
| `apps/android/.../KeyboardView.kt` (touch→logical mapping, cell layout) | Reference for the extension's tap routing (behavioral parity, re-expressed in Swift/UIKit). |

**No existing iOS/Swift/Xcode code exists** (verified) — this is greenfield under
`apps/ios/`.

---

## 4. The one shared-core change — and the Android guardrail

The core is touched in exactly **one additive way**:

```
# core/crates/featherkey-core/Cargo.toml
crate-type = ["lib", "cdylib"]            →  ["lib", "cdylib", "staticlib"]
```

`staticlib` makes cargo **also** emit a `.a` for iOS static linking (the standard
UniFFI-on-iOS packaging path, an `xcframework` of static libs). The existing
`cdylib` (`.so`) that Android consumes is **byte-unaffected** — adding a crate-type
does not change the other outputs.

**"Won't impact Android" is a verified contract, not an assertion.** After the
change, all three must hold:

1. `bash core/tools/ci-local.sh` stays green (all gates).
2. The committed Android Kotlin bindings stay **byte-identical** (ci-local's
   bindings gate; the FFI surface is untouched, so checksums are unchanged).
3. **No file under `apps/android/` changes.**

If any of the three regresses, the change is wrong and is reverted. No Kotlin
changes, no behavioral core changes — iOS is strictly additive.

---

## 5. Architecture

Dependencies point inward only (ARCHITECTURE.md §4):

```
core/  (Rust engine — behavior UNCHANGED)
  └─ UniFFI KeyboardCore FFI surface            (existing; emits Swift)
       └─ FeatherKeyCore.xcframework            (static libs: device + sim; + generated Swift bindings)
            └─ apps/ios/FeatherKeyKit           (thin Swift adapter over KeyboardCore + pure geometry)
                 ├─ FeatherKeyKeyboard          (App Extension: UIInputViewController, renders keys, routes taps)
                 └─ FeatherKeyHost              (minimal container app to install + enable the extension)
```

### 5.1 Layer responsibilities (ports & invariants)

- **`FeatherKeyKit`** — the only code that talks to the generated `KeyboardCore`.
  It exposes **one port protocol** to the extension:

  ```swift
  protocol KeyboardEngine {                 // the sole seam FeatherKeyKeyboard depends on
      func layoutKeys() -> [EngineKey]      // rects + labels, from core layout_keys()
      func decode(atLogicalX x: Float, y: Float) throws -> String   // core decode(x,y) → character
  }
  ```

  Its concrete implementation wraps the UniFFI `KeyboardCore` (constructed per
  §5.2). The extension programs against `KeyboardEngine`, never the generated
  binding directly (DIP — the adapter is swappable, e.g. a fake in tests). It
  contains **no typing logic** (CLAUDE.md §5 smell test: typing logic that appears
  in Swift is a design smell). `EngineKey` is a plain value type (label + logical
  rect), UIKit-free.
- **Coordinate/geometry mapping is a pure function** — no UIKit types — so it is
  unit-testable headlessly, mirroring `KeyboardGeometry.kt`. It maps a touch in
  view pixels into the logical coordinate space `decode(x, y)` expects.

  > **This mapping is the single biggest risk** (BUILD_AND_RUN.md §6 warned the
  > touch-coordinate mapping is where Android iteration concentrated). `decode`'s
  > doc says "surface-local pixel (x, y)", but Android's `KeyboardView.kt` maps
  > device pixels into a **logical space** (per `layout-engine`, ~1000×360) that
  > `FfiKey` rects also live in, before calling `decode`. The **exact logical
  > scale and origin are not assumed here** — the plan's first task is to read the
  > precise contract from `KeyboardView.kt` + `layout-engine` and encode it as the
  > pure Swift mapping, tested against known key-center coordinates.
- **`FeatherKeyKeyboard`** owns only platform concerns: `UIInputViewController`
  lifecycle, drawing keys, hit-testing touches, and committing via
  `UITextDocumentProxy` (`insertText` / `deleteBackward`). Shift/caps is a
  view-state concern in the shell; the *character* still comes from the core.

### 5.2 Core construction (mandated by `open()`)

The core's **sole constructor** is:

```rust
KeyboardCore::open(db_path: String, device_key: Vec<u8> /*32*/, languages: Vec<LanguagePack>)
```

It always opens an encrypted store (`RedbSecureStore`) keyed by a 32-byte key —
there is **no stateless decode-only path**. Therefore even a typing-only slice
must provide:

- **`device_key`** — 32 bytes provisioned via the iOS **Keychain**, the direct
  analog of Android's Keystore-backed key (BR-62). Minimal, standard Keychain
  code; generated once, reloaded thereafter.
- **`db_path`** — a file in the **extension's own container** (`NSHomeDirectory()`
  inside the extension). No **App Group** is needed this slice, because nothing is
  yet shared between host and extension; App Group is deferred to the persistence
  slice.
- **`languages`** — a single bundled language pack (English), mirroring how
  Android seeds `LanguagePack`.

The store is opened but **unused** this slice: no `learn_word`, `suggest`,
`persist`, or correction calls are issued. Persistence/learning *behavior* is
deferred; only the constructor's contract is satisfied.

---

## 6. Repository structure (new)

```
apps/ios/
  FeatherKey.xcodeproj                 # 3 targets: Host, Keyboard ext, FeatherKeyKit
  FeatherKeyHost/                       # minimal SwiftUI container app
  FeatherKeyKeyboard/                   # UIInputViewController extension
  FeatherKeyKit/                        # Swift adapter + pure geometry (+ generated Swift bindings)
  FeatherKeyKitTests/                   # headless XCTest (geometry, key lookup)
  build-core.sh                         # analog of build-jni.sh (see below)
  .gitignore                            # xcframework + build artifacts
```

**`build-core.sh`** (iOS analog of `build-jni.sh`):

1. `cargo build --release --locked --features uniffi` for
   `aarch64-apple-ios` (device), `aarch64-apple-ios-sim`, `x86_64-apple-ios`.
2. `lipo` the two simulator slices into one fat static lib.
3. Assemble `FeatherKeyCore.xcframework` (device slice + fat-sim slice + headers +
   module map).
4. Generate Swift bindings:
   `uniffi-bindgen-tool generate --library <built dylib> --language swift`.

**Commit policy mirrors Android exactly:** the **generated Swift bindings are
committed** (pure-Swift iteration needs no Rust toolchain), the **xcframework
binary is gitignored** (build artifact, regenerable from source — same
supply-chain posture that keeps `.so` out of the tree).

---

## 7. Testing (TDD/BDD-first — CLAUDE.md §3)

Order is strict: BDD scenario → failing unit tests seen to fail → minimal impl.

- **BDD:** new `core/features/ios_keyboard.feature`, scenario tagged **`@BR-70`** —
  "typing on the iOS keyboard commits characters decoded by the shared core."
  Expressed as observable behavior (tap a key position → the decoded character is
  inserted).
- **TDD (host-testable, no simulator/UIKit):** `FeatherKeyKitTests` XCTest covering
  the **pure geometry**: view-pixel → logical-coordinate mapping, and key lookup /
  layout arithmetic. Seen to fail before implementation. (The core's `decode`
  correctness is already covered by Rust tests — not re-tested here.)
- **BDD ↔ requirement traceability** (`bdd_check.py`): the `@BR-70` scenario must
  map to the new BR-70 row.
- **Verification I run myself (honest about the friction):**
  - `build-core.sh` producing the xcframework — fully automatable, evidence pasted.
  - `xcodebuild build` of all three targets, and `xcodebuild test` of
    `FeatherKeyKitTests` (headless, no simulator UI) — fully automatable, evidence
    pasted (pass/fail counts).
  - Boot a simulator + install the host app via `simctl` — automatable.
  - **The end-to-end "type a sentence" check is the friction point:** enabling a
    third-party keyboard extension requires navigating the simulator's
    Settings → General → Keyboard → Keyboards → Add New Keyboard UI; there is **no
    clean `simctl` command** for it. Unlike a physical touchscreen this UI is
    deterministic and scriptable via the simulator, so it is more tractable than
    Android device typing — but it is **not** a one-liner and may need manual UI
    steps. The build gate will state exactly which parts were automated vs. done by
    hand; the slice is not claimed "verified" on a green `xcodebuild test` alone —
    the char-in/char-out round-trip must be observed.

**Known tooling gaps (recorded, not silently ignored):**

- **DoD ≥98% line coverage (§3) is a Rust metric** — `ci-local`/fitness measure
  `core/`, not Swift. This slice's *testable* Swift (the pure geometry + key
  lookup) is covered by `xcodebuild test` with coverage enabled and held to the
  same bar; the thin UIKit glue (`UIInputViewController` drawing/hit-testing) is
  verified by the simulator round-trip, not line coverage. The plan states the
  Swift coverage target explicitly so "98%" is not silently dropped.
- **CODEMAP does not index Swift.** `codemap.py` derives from Rust/Kotlin/`.feature`
  /Cargo/`settings.gradle.kts`; new `apps/ios/*.swift` symbols won't appear (the
  `@BR-70` `.feature` will). Extending `codemap.py` to Swift is a **deferred
  tooling item** (noted in §8), not a blocker for this slice.

---

## 8. Deferred (recorded, not built — CLAUDE.md §4 KISS)

Each becomes its own gated slice, in roughly this order:

1. **Numbers/symbols page + shift polish** (`use_numeric_layout` / `use_symbols_layout`).
2. **Suggestion strip** (`suggest` / `rank_suggestions`) + inline prediction (BR-42).
3. **Autocorrect-on-space** (`choose_correction` / `observe_autocorrect_outcome`).
4. **On-device learning + persistence** (`learn_word`, `persist`, `import_*`) —
   needs **App Group** (host↔extension sharing) + Keychain-backed key, consent
   (BR-22), and field-sensitivity gating (BR-26). This is the big one; explicitly
   not in the foundation.
5. **Swipe/gesture typing** (BR-41) — the pointer/gesture model, iOS-side.
6. **Long-press accents, emoji, voice** (BR-43/44).
7. **Physical device + App Store** (signing, entitlements, privacy manifest).
8. **Tooling: extend `codemap.py` to index Swift** so `apps/ios/` symbols are
   DRY-checkable like Rust/Kotlin (today they are invisible to CODEMAP).

---

## 9. Alternatives rejected

| Alternative | Why rejected |
|---|---|
| **Reimplement typing logic in Swift** | Violates DRY and CLAUDE.md §5 ("typing logic belongs in `core/`; if it's being written in Kotlin[/Swift], that's a design smell"). Two engines drift; the whole monorepo thesis is one shared engine. |
| **Shared core via hand-written C FFI (skip UniFFI)** | Reinvents what UniFFI already generates, diverges from the proven Android path, and adds `unsafe` surface for no benefit. |
| **Dynamic-framework (`.dylib`) instead of static `xcframework`** | App extensions strongly prefer static linking; a static `xcframework` is the standard, lowest-friction UniFFI-iOS packaging and avoids embedded-dylib load pitfalls in extensions. |
| **Full feature parity in one push** | Rejected per KISS and the agreed scope; high-risk, hard to verify, several features carry iOS-specific platform work (Keychain, App Group, Full-Access rules). Slice it. |
| **App Group in the foundation slice** | YAGNI this slice — nothing is shared between host and extension yet. Added when persistence lands. |

---

## Audit log

_(Appended on every `/r-u-sure` gate run, per CLAUDE.md §1.1.)_

### Pass 1 — 🚧 Incomplete → fixed
Audited the design against CLAUDE.md §1.2 (design must name port traits, modules,
invariants, alternatives) and the BRD's iOS scoping. Gaps found:
1. **Port trait unnamed** — §1.2 requires it; §5.1 was prose-only.
2. **BR-70 BRD placement ambiguous** — §8.10 is the "parity-on-Android" benchmark,
   a different axis from "run on iOS"; conflation risk.
3. **Verification over-claimed** — §7 implied enabling the keyboard on the
   simulator is frictionless; there is no clean `simctl` command for it.
4. **DoD ≥98% coverage & CODEMAP don't extend to Swift** — unrecorded tooling gaps
   that would silently weaken the DoD.
5. **decode coordinate-space contract under-specified** — the primary risk area
   (pixels vs. logical ~1000×360) was asserted, not pinned to a plan action.

Changed:
- §5.1 now defines the concrete `KeyboardEngine` Swift port protocol (+ `EngineKey`
  value type, DIP rationale).
- §2 now places BR-70 in a **new BRD §8.16 "iOS as a Delivery Platform"**, priority
  S, distinct from §8.10, with the amendment spelled out.
- §7 rewritten to separate automatable steps from the manual simulator-enable
  friction; the slice is explicitly not "verified" on `xcodebuild test` alone.
- §7 adds a "Known tooling gaps" block (Swift coverage bar stated; CODEMAP-Swift
  invisibility recorded); §8 adds the `codemap.py`-Swift extension as a deferred item.
- §5.1 now makes the coordinate mapping the plan's **first task** (read the exact
  contract from `KeyboardView.kt` + `layout-engine`), not an assumption.

### Pass 2 — ✅ Complete and verified (design phase)
Re-audited the revised spec against §1.2 and the BRD:
- **Problem / scope** — §1 (in/out lists explicit). ✅
- **Requirements (BR IDs)** — BR-70 defined + BRD amendment located precisely (§2). ✅
- **Modules + whether they exist** — §3 table names every reused existing symbol
  (`KeyboardCore`, `layout_keys`/`decode`/`open`, `build-jni.sh`, `KeyboardGeometry.kt`,
  `uniffi-bindgen-tool`); confirms iOS is greenfield. ✅
- **Port traits** — `KeyboardEngine` named with signatures (§5.1). ✅
- **Invariants** — no-typing-logic-in-Swift, pure UIKit-free geometry, inward-only
  deps, additive-only core change (§4/§5). ✅
- **Alternatives rejected** — §9, five entries with reasons. ✅
- **Android-impact contract** — §4's verified triad (ci-local green + bindings
  byte-identical + zero `apps/android/` diff). ✅
- **TDD/BDD-first** — §7 orders BDD `@BR-70` → failing XCTest → impl. ✅
- **KISS deferrals recorded** — §8. ✅

Evidence supporting the design's factual claims (gathered this phase, not asserted):
- `uniffi-bindgen-tool` `--language` includes `swift` (verified via `--help`).
- Toolchain present: Xcode 26.4 / iOS SDK 26.4; rust targets `aarch64-apple-ios`,
  `aarch64-apple-ios-sim`, `x86_64-apple-ios`; iPhone simulators available.
- No pre-existing iOS/Swift/Xcode files (greenfield confirmed).
- `KeyboardCore::open(db_path, device_key[32], languages)` is the sole constructor
  (read from `ffi.rs`), which is why §5.2 mandates Keychain key + container path.

No new gaps. Design is internally consistent and maps every requirement. This is a
**design** artifact — "verified" here means audited against the BRD and grounded in
checked facts; behavioral verification belongs to the build gate. Advancing to the
plan phase.
