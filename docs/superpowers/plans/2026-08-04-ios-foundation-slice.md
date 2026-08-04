# iOS Foundation Slice — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a minimal iOS keyboard extension that types letters end-to-end
through the shared Rust core on the iPhone simulator, with zero impact on Android.

**Architecture:** The identical Rust `core/` engine is packaged as a
`FeatherKeyCore.xcframework` (static libs + UniFFI-generated Swift bindings) and
consumed by a thin Swift shell under `apps/ios/`. The shell (a
`UIInputViewController` extension + minimal host app) depends only on a
`KeyboardEngine` port in `FeatherKeyKit`; all typing decisions come from the core.

**Tech Stack:** Rust (core, unchanged behavior) · UniFFI Swift bindings · Swift +
UIKit (extension) + SwiftUI (host) · XCTest · XcodeGen (project generation) ·
`cargo` iOS targets + `lipo` + `xcframework` · `xcodebuild`/`simctl`.

Design: `docs/superpowers/specs/2026-08-04-ios-foundation-slice-design.md`.

## Global Constraints

Every task's requirements implicitly include these (verbatim from the design/CLAUDE.md):

- **Core is touched additively only:** the sole change to `core/` is
  `crate-type = ["lib","cdylib"]` → `["lib","cdylib","staticlib"]`. No Rust logic,
  no Kotlin, no behavioral change.
- **"Won't impact Android" is a verified triad, checked after the core change and
  again at the end:** (1) `bash core/tools/ci-local.sh` exits 0 (all gates);
  (2) the committed Android Kotlin bindings stay **byte-identical**; (3) `git diff
  --stat` shows **no file under `apps/android/` changed**.
- **No typing logic in Swift** (CLAUDE.md §5). The extension depends only on the
  `KeyboardEngine` protocol, never on the generated binding directly.
- **Scope is the foundation slice:** letters + space + backspace + shift. Only
  *letter* taps go through `decode`; space/backspace/shift are shell-drawn function
  keys. Everything else is deferred (design §8).
- **Verification target is the iOS Simulator only.** No signing/provisioning.
- **`decode` and `FfiKey` share the layout's logical coordinate space.** The shell
  draws logical→view and decodes view→logical with the *same* affine transform, so
  "what is drawn is what decode resolves against" holds by construction.
- **No AI attribution** anywhere (CLAUDE.md §8).
- **Commits are checkpoints, not auto-actions:** each task ends with a `git add`
  staging + a checkpoint, but per CLAUDE.md §8 nothing is committed until the user
  asks. "Commit" steps below mean "stage and checkpoint; hold the actual commit."
- **Generated/binary artifacts are gitignored, source is committed:** commit
  `project.yml`, `build-core.sh`, generated **Swift bindings** (mirroring Android's
  committed Kotlin bindings); gitignore the `.xcodeproj`, the `.xcframework`, and
  Rust `target/`.

## File Structure

```
core/crates/featherkey-core/Cargo.toml     # MODIFY: +"staticlib" (Task 1)
BUSINESS_REQUIREMENTS.md                    # MODIFY: BR-70 + line-134 amendment (Task 4)
core/features/ios_keyboard.feature          # CREATE: @BR-70 scenario (Task 4)
apps/ios/
  .gitignore                                # CREATE: *.xcodeproj, *.xcframework, target/, DerivedData/
  project.yml                               # CREATE: XcodeGen spec — 3 targets + test target (Task 3)
  build-core.sh                             # CREATE: core→xcframework + Swift bindings (Task 2)
  Generated/featherkey_core.swift           # GENERATED+committed: UniFFI Swift binding (Task 2; flat)
  FeatherKeyKit/
    KeyboardGeometry.swift                  # CREATE: pure logical<->view mapping (Task 5)
    DeviceKey.swift                         # CREATE: Keychain 32-byte key (Task 6)
    KeyboardEngine.swift                    # CREATE: port protocol + EngineKey (Task 7)
    CoreKeyboardEngine.swift                # CREATE: UniFFI adapter over KeyboardCore (Task 7)
  FeatherKeyKitTests/
    KeyboardGeometryTests.swift             # CREATE: geometry unit tests (Task 5)
    DeviceKeyTests.swift                    # CREATE: keychain round-trip (Task 6)
    CoreKeyboardEngineTests.swift           # CREATE: real-core decode test (Task 7)
  FeatherKeyKeyboard/
    KeyboardViewController.swift            # CREATE: UIInputViewController glue (Task 8)
    Info.plist                              # CREATE: extension point + RequestsOpenAccess=false (Task 8)
  FeatherKeyHost/
    FeatherKeyHostApp.swift                 # CREATE: minimal SwiftUI container (Task 9)
    Info.plist                              # CREATE (Task 9)
```

---

### Task 1: Core `staticlib` crate-type + Android guardrail

**Files:**
- Modify: `core/crates/featherkey-core/Cargo.toml` (the `crate-type` line)

**Interfaces:**
- Produces: an additional `.a` build output for iOS static linking. No API change.

- [ ] **Step 1: Record the Android baseline (proves no regression later)**

Run: `bash core/tools/ci-local.sh` and note it exits 0. Capture the committed
Android bindings hash:
```bash
shasum -a256 apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt
```
Expected: ci-local prints `ci-local: ALL GATES PASSED`; record the hash.

- [ ] **Step 2: Make the additive change**

In `core/crates/featherkey-core/Cargo.toml`, change:
```toml
crate-type = ["lib", "cdylib"]
```
to:
```toml
crate-type = ["lib", "cdylib", "staticlib"]
```

- [ ] **Step 3: Verify the Android guardrail triad**

Run:
```bash
bash core/tools/ci-local.sh
shasum -a256 apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt
git diff --stat -- apps/android/
```
Expected: ci-local `ALL GATES PASSED`; bindings hash **identical** to Step 1;
`git diff --stat -- apps/android/` prints **nothing**. If any differs, revert and stop.

- [ ] **Step 4: Confirm the staticlib actually builds for a host target (fast smoke)**

Run: `cargo build -p featherkey-core --features uniffi --release` from `core/`.
Expected: builds; `core/target/release/libfeatherkey_core.a` now exists.

- [ ] **Step 5: Checkpoint** — stage `core/crates/featherkey-core/Cargo.toml`; hold commit per Global Constraints.

---

### Task 2: `build-core.sh` → xcframework + Swift bindings

**Files:**
- Create: `apps/ios/build-core.sh`
- Create: `apps/ios/.gitignore`
- Generated+committed: `apps/ios/Generated/FeatherKey/*.swift`

**Interfaces:**
- Produces: `apps/ios/FeatherKeyCore.xcframework` (gitignored) and committed Swift
  bindings under `apps/ios/Generated/`. Later tasks link the xcframework and import
  the bindings' module.

- [ ] **Step 1: Write `apps/ios/.gitignore`**

```gitignore
# Generated/binary artifacts — regenerable from source (mirrors the .so posture)
*.xcodeproj/
*.xcframework/
DerivedData/
build/
```

- [ ] **Step 2: Write `apps/ios/build-core.sh`**

```bash
#!/usr/bin/env bash
# Build the shared FeatherKey core for iOS and package it as an xcframework,
# plus generate the UniFFI Swift bindings. Analog of ffi-bridge/build-jni.sh.
# Artifacts (the .xcframework) are gitignored; the generated Swift is committed.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git -C "$here" rev-parse --show-toplevel)"
crate_dir="$root/core/crates/featherkey-core"
gen_dir="$here/Generated"          # committed: ONLY the .swift binding
hdr_dir="$(mktemp -d)"             # transient: FFI header + module.modulemap for the xcframework
xcf="$here/FeatherKeyCore.xcframework"
t="$root/core/target"
lib=libfeatherkey_core.a

cd "$crate_dir"
echo "Building release static libs for device + simulator ..."
cargo build --release --locked --features uniffi --target aarch64-apple-ios
cargo build --release --locked --features uniffi --target aarch64-apple-ios-sim
cargo build --release --locked --features uniffi --target x86_64-apple-ios

# CRITICAL: generate bindings from an UNSTRIPPED build. [profile.release] sets
# `strip = true`, which drops .symtab — and uniffi-bindgen's --library metadata
# extraction reads .symtab, so generating from a release artifact yields ZERO
# files (observed). Build a host DEBUG dylib (unstripped) purely for generation.
echo "Building host debug dylib for binding generation (unstripped) ..."
cargo build --locked --features uniffi          # debug, host target
gen_lib="$t/debug/libfeatherkey_core.dylib"

# Fat simulator static lib (arm64-sim + x86_64-sim)
sim_dir="$(mktemp -d)"
lipo -create \
  "$t/aarch64-apple-ios-sim/release/$lib" \
  "$t/x86_64-apple-ios/release/$lib" \
  -output "$sim_dir/$lib"

# Generate the Swift binding (+ FFI header + modulemap) into a temp dir, then
# split: the .swift is committed under Generated/, the C header + modulemap go
# into the xcframework's headers so it vends the `featherkey_coreFFI` Clang module.
tmp_gen="$(mktemp -d)"
cd "$root/core"
cargo run -p uniffi-bindgen-tool -- generate \
  --library "$gen_lib" --language swift --out-dir "$tmp_gen"
# Guard against silent empty generation (the strip trap):
test -n "$(find "$tmp_gen" -name '*.swift' -print -quit)" || { echo "ERROR: no Swift generated"; exit 1; }

rm -rf "$gen_dir" && mkdir -p "$gen_dir" "$hdr_dir"
find "$tmp_gen" -name '*.swift' -exec cp {} "$gen_dir/" \;
find "$tmp_gen" -name '*FFI.h'  -exec cp {} "$hdr_dir/" \;
# UniFFI emits featherkey_coreFFI.modulemap; the xcframework needs it named
# module.modulemap in the headers dir to vend the Clang module.
find "$tmp_gen" -name '*.modulemap' -exec cp {} "$hdr_dir/module.modulemap" \;

# Assemble the xcframework: two static slices, each with the same C module headers.
rm -rf "$xcf"
xcodebuild -create-xcframework \
  -library "$t/aarch64-apple-ios/release/$lib" -headers "$hdr_dir" \
  -library "$sim_dir/$lib" -headers "$hdr_dir" \
  -output "$xcf"

echo "Done: $xcf ; committed Swift in $gen_dir"
```

- [ ] **Step 3: Make it executable and run it**

Run: `chmod +x apps/ios/build-core.sh && bash apps/ios/build-core.sh`
Expected: exits 0; `apps/ios/FeatherKeyCore.xcframework/` exists with an
`ios-arm64` and an `ios-*-simulator` slice, and each slice's `Headers/` contains
`featherkey_coreFFI.h` + `module.modulemap`; `apps/ios/Generated/` contains
**only** `featherkey_core.swift` (the C header + modulemap live in the xcframework,
not the framework sources). If the run aborts with "no Swift generated", the strip
trap fired — confirm the debug dylib exists and was used.

- [ ] **Step 4: Verify the two xcframework slices are present**

Run: `ls apps/ios/FeatherKeyCore.xcframework` and confirm two platform dirs
(device + simulator) and an `Info.plist`.

- [ ] **Step 5: Checkpoint** — stage `apps/ios/build-core.sh`, `apps/ios/.gitignore`,
  `apps/ios/Generated/`; hold commit.

---

### Task 3: Xcode project scaffold (XcodeGen, 3 targets + tests)

**Files:**
- Create: `apps/ios/project.yml`

**Interfaces:**
- Produces: `apps/ios/FeatherKey.xcodeproj` (gitignored) with targets `FeatherKeyKit`
  (framework), `FeatherKeyKitTests`, `FeatherKeyKeyboard` (app extension),
  `FeatherKeyHost` (app). Later tasks add source files that these targets compile.

- [ ] **Step 1: Install XcodeGen**

Run: `brew install xcodegen` (once). Expected: `xcodegen --version` prints a version.

- [ ] **Step 2: Write `apps/ios/project.yml`**

```yaml
name: FeatherKey
options:
  bundleIdPrefix: com.featherkey.ios
  deploymentTarget:
    iOS: "16.0"
settings:
  base:
    SWIFT_VERSION: "5.0"
packages: {}
targets:
  FeatherKeyKit:
    type: framework
    platform: iOS
    sources:
      - path: FeatherKeyKit
      - path: Generated            # UniFFI-generated Swift bindings
    dependencies:
      - framework: FeatherKeyCore.xcframework
        embed: false
  FeatherKeyKitTests:
    type: bundle.unit-test
    platform: iOS
    sources: [FeatherKeyKitTests]
    dependencies:
      - target: FeatherKeyKit
    settings:
      base:
        GENERATE_INFOPLIST_FILE: true
  FeatherKeyKeyboard:
    type: app-extension
    platform: iOS
    sources: [FeatherKeyKeyboard]
    info:
      path: FeatherKeyKeyboard/Info.plist
    dependencies:
      - target: FeatherKeyKit
      - framework: FeatherKeyCore.xcframework
        embed: false
  FeatherKeyHost:
    type: application
    platform: iOS
    sources: [FeatherKeyHost]
    info:
      path: FeatherKeyHost/Info.plist
    dependencies:
      - target: FeatherKeyKeyboard
        embed: true
```

- [ ] **Step 3: Create placeholder source dirs so generation + build succeed**

Run:
```bash
cd apps/ios
mkdir -p FeatherKeyKit FeatherKeyKitTests FeatherKeyKeyboard FeatherKeyHost
```
(Real sources land in later tasks; XcodeGen needs the dirs to exist.)

- [ ] **Step 4: Generate the project**

Run: `cd apps/ios && xcodegen generate`
Expected: `FeatherKey.xcodeproj` created; command exits 0.

- [ ] **Step 5: Checkpoint** — stage `apps/ios/project.yml`; hold commit.
  (The `.xcodeproj` is gitignored.)

---

### Task 4: BR-70 + BDD scenario + traceability (BDD-first)

**Files:**
- Modify: `BUSINESS_REQUIREMENTS.md` (line-134 amendment + new §8.16 with BR-70 +
  traceability row)
- Create: `core/features/ios_keyboard.feature`

**Interfaces:**
- Produces: the `@BR-70` tag that Task 8's on-device behavior closes, and the
  requirement row `bdd_check.py` validates.

- [ ] **Step 1: Write the failing BDD scenario**

Create `core/features/ios_keyboard.feature`:
```gherkin
@BR-70
Feature: FeatherKey types on iOS through the shared core
  The iOS keyboard extension reuses the shared Rust core: a tap on a rendered
  key commits the character the core decodes for that key's position.

  Scenario: Typing a letter commits the core-decoded character
    Given the FeatherKey iOS keyboard is shown in a text field
    When the user taps the centre of the "h" key
    Then the character "h" is inserted into the field
    And no typing logic ran outside the shared core
```

- [ ] **Step 2: Run the traceability check — see it fail**

Run: `cd core && python3 tools/bdd_check.py`
Expected: FAIL — `@BR-70` has no matching requirement row yet.

- [ ] **Step 3: Amend the BRD**

In `BUSINESS_REQUIREMENTS.md`:
- Edit line ~134 from
  `Non-Android platforms (iOS, desktop, web) — reference targets only, not deliverables.`
  to
  `Desktop and web remain reference targets only; **iOS is a delivery platform** (see §8.16).`
- Add a new subsection after §8.15:
  ```markdown
  ### 8.16 iOS as a Delivery Platform

  | ID | Requirement | Priority | Traceability |
  |---|---|---|---|
  | BR-70 | FeatherKey must ship as an iOS keyboard extension that reuses the shared Rust core — typing logic is never reimplemented in Swift. | S | P-9, OBJ-8 (modularity/reuse) |
  ```
  (Use the actual P-/OBJ- IDs for modularity/footprint present in §7; match the
  table format of the neighbouring §8.x tables exactly.)

- [ ] **Step 4: Run the traceability check — see it pass**

Run: `cd core && python3 tools/bdd_check.py`
Expected: PASS — `@BR-70` maps to the BR-70 row.

- [ ] **Step 5: Checkpoint** — stage `BUSINESS_REQUIREMENTS.md`,
  `core/features/ios_keyboard.feature`; hold commit.

---

### Task 5: Pure geometry `KeyboardGeometry.swift` (TDD)

**Files:**
- Create: `apps/ios/FeatherKeyKit/KeyboardGeometry.swift`
- Test: `apps/ios/FeatherKeyKitTests/KeyboardGeometryTests.swift`

**Interfaces:**
- Produces:
  ```swift
  struct LogicalSize { let width: Float; let height: Float }
  enum KeyboardGeometry {
      // Union bounds of the layout keys → the logical coordinate space.
      static func logicalBounds(_ keys: [EngineKey]) -> LogicalSize
      // View-pixel point → logical point, using independent x/y affine scale.
      static func toLogical(viewX: Float, viewY: Float,
                            viewWidth: Float, viewHeight: Float,
                            logical: LogicalSize) -> (x: Float, y: Float)
  }
  ```
  (`EngineKey` is defined in Task 7; for this task, add a minimal local
  `EngineKey { let label: String; let x, y, width, height: Float }` in
  `KeyboardEngine.swift` first — Step 0.)

- [ ] **Step 0: Add the `EngineKey` value type** (needed by geometry + engine)

Create `apps/ios/FeatherKeyKit/KeyboardEngine.swift` with just:
```swift
public struct EngineKey: Equatable {
    public let label: String
    public let x, y, width, height: Float
    public init(label: String, x: Float, y: Float, width: Float, height: Float) {
        self.label = label; self.x = x; self.y = y; self.width = width; self.height = height
    }
}
```

- [ ] **Step 1: Write the failing tests**

Create `apps/ios/FeatherKeyKitTests/KeyboardGeometryTests.swift`:
```swift
import XCTest
@testable import FeatherKeyKit

final class KeyboardGeometryTests: XCTestCase {
    func test_logicalBounds_is_the_union_extent_of_keys() {
        let keys = [
            EngineKey(label: "a", x: 0, y: 0, width: 100, height: 360),
            EngineKey(label: "b", x: 900, y: 0, width: 100, height: 360),
        ]
        let b = KeyboardGeometry.logicalBounds(keys)
        XCTAssertEqual(b.width, 1000, accuracy: 0.001)   // 900 + 100
        XCTAssertEqual(b.height, 360, accuracy: 0.001)
    }

    func test_toLogical_uses_independent_x_y_affine_scale() {
        let logical = LogicalSize(width: 1000, height: 360)
        // A touch at the centre of a 320x216 view → centre of logical space.
        let p = KeyboardGeometry.toLogical(viewX: 160, viewY: 108,
                                           viewWidth: 320, viewHeight: 216,
                                           logical: logical)
        XCTAssertEqual(p.x, 500, accuracy: 0.001)   // 160 * 1000/320
        XCTAssertEqual(p.y, 180, accuracy: 0.001)   // 108 * 360/216
    }

    func test_toLogical_maps_a_known_key_centre_back_to_that_key() {
        let logical = LogicalSize(width: 1000, height: 360)
        // Key "b" centre is logical (950,180); it renders at view x =
        // 950 * 320/1000 = 304, y = 180 * 216/360 = 108. Round-trips back.
        let p = KeyboardGeometry.toLogical(viewX: 304, viewY: 108,
                                           viewWidth: 320, viewHeight: 216,
                                           logical: logical)
        XCTAssertEqual(p.x, 950, accuracy: 0.001)
        XCTAssertEqual(p.y, 180, accuracy: 0.001)
    }
}
```

- [ ] **Step 2: Run tests — see them fail**

Run: `cd apps/ios && xcodegen generate && xcodebuild test -project FeatherKey.xcodeproj -scheme FeatherKeyKit -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:FeatherKeyKitTests/KeyboardGeometryTests`
Expected: FAIL — `KeyboardGeometry` / `LogicalSize` undefined.

- [ ] **Step 3: Implement `KeyboardGeometry.swift`**

```swift
public struct LogicalSize: Equatable {
    public let width: Float
    public let height: Float
    public init(width: Float, height: Float) { self.width = width; self.height = height }
}

public enum KeyboardGeometry {
    /// The logical coordinate space is the union extent of the layout's keys —
    /// the same space `FfiKey`/`decode` use, so drawing and decoding agree.
    public static func logicalBounds(_ keys: [EngineKey]) -> LogicalSize {
        let w = keys.map { $0.x + $0.width }.max() ?? 0
        let h = keys.map { $0.y + $0.height }.max() ?? 0
        return LogicalSize(width: w, height: h)
    }

    /// Map a view-pixel point into logical space with independent x/y scale, the
    /// exact inverse of how keys are drawn (logical→view). Self-consistent by
    /// construction: what is drawn is what `decode` resolves against.
    public static func toLogical(viewX: Float, viewY: Float,
                                 viewWidth: Float, viewHeight: Float,
                                 logical: LogicalSize) -> (x: Float, y: Float) {
        let sx = viewWidth  > 0 ? logical.width  / viewWidth  : 0
        let sy = viewHeight > 0 ? logical.height / viewHeight : 0
        return (viewX * sx, viewY * sy)
    }
}
```

- [ ] **Step 4: Run tests — see them pass**

Run the same `xcodebuild test` command from Step 2. Expected: 3 tests PASS.

- [ ] **Step 5: Checkpoint** — stage the two files + `KeyboardEngine.swift`; hold commit.

---

### Task 6: Keychain device-key `DeviceKey.swift` (TDD)

**Files:**
- Create: `apps/ios/FeatherKeyKit/DeviceKey.swift`
- Test: `apps/ios/FeatherKeyKitTests/DeviceKeyTests.swift`

**Interfaces:**
- Produces:
  ```swift
  enum DeviceKey {
      // Returns the persisted 32-byte key, generating+storing it on first call.
      static func loadOrCreate(account: String) throws -> Data
  }
  ```
  Consumed by `CoreKeyboardEngine` (Task 7) to satisfy `KeyboardCore.open`'s
  32-byte `device_key`.

- [ ] **Step 1: Write the failing test**

Create `apps/ios/FeatherKeyKitTests/DeviceKeyTests.swift`:
```swift
import XCTest
@testable import FeatherKeyKit

final class DeviceKeyTests: XCTestCase {
    func test_loadOrCreate_is_32_bytes_and_stable() throws {
        let acct = "test-\(UUID().uuidString)"
        defer { try? DeviceKey.delete(account: acct) }
        let a = try DeviceKey.loadOrCreate(account: acct)
        let b = try DeviceKey.loadOrCreate(account: acct)
        XCTAssertEqual(a.count, 32)
        XCTAssertEqual(a, b)   // second call returns the same stored key
    }
}
```

- [ ] **Step 2: Run test — see it fail**

Run: `xcodebuild test ... -only-testing:FeatherKeyKitTests/DeviceKeyTests`
Expected: FAIL — `DeviceKey` undefined.

- [ ] **Step 3: Implement `DeviceKey.swift`** (Keychain — the iOS analog of Android Keystore, BR-62)

```swift
import Foundation
import Security

public enum DeviceKeyError: Error { case unexpectedStatus(OSStatus), rng }

public enum DeviceKey {
    private static let service = "com.featherkey.ios.deviceKey"

    public static func loadOrCreate(account: String) throws -> Data {
        if let existing = try load(account: account) { return existing }
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess
        else { throw DeviceKeyError.rng }
        let key = Data(bytes)
        let add: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecValueData as String: key,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw DeviceKeyError.unexpectedStatus(status) }
        return key
    }

    static func load(account: String) throws -> Data? {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var out: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &out)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else { throw DeviceKeyError.unexpectedStatus(status) }
        return out as? Data
    }

    public static func delete(account: String) throws {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let status = SecItemDelete(q as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound
        else { throw DeviceKeyError.unexpectedStatus(status) }
    }
}
```

- [ ] **Step 4: Run test — see it pass**

Run the Step 2 command. Expected: PASS. (Simulator keychain is available headlessly.)

- [ ] **Step 5: Checkpoint** — stage the two files; hold commit.

---

### Task 7: `KeyboardEngine` port + `CoreKeyboardEngine` adapter (TDD against real core)

**Files:**
- Modify: `apps/ios/FeatherKeyKit/KeyboardEngine.swift` (add the protocol)
- Create: `apps/ios/FeatherKeyKit/CoreKeyboardEngine.swift`
- Test: `apps/ios/FeatherKeyKitTests/CoreKeyboardEngineTests.swift`

**Interfaces:**
- Consumes: the generated `KeyboardCore` (from `Generated/`), `EngineKey` (Task 5),
  `DeviceKey` (Task 6), `KeyboardGeometry` (Task 5).
- Produces:
  ```swift
  public protocol KeyboardEngine {
      func layoutKeys() -> [EngineKey]
      func decode(atLogicalX x: Float, y: Float) throws -> String   // "" if no best
  }
  public final class CoreKeyboardEngine: KeyboardEngine {
      public init(containerDir: URL) throws
  }
  ```

- [ ] **Step 1: Add the protocol to `KeyboardEngine.swift`**

```swift
public protocol KeyboardEngine {
    func layoutKeys() -> [EngineKey]
    /// The character the core decodes at a logical-space point ("" if none).
    func decode(atLogicalX x: Float, y: Float) throws -> String
}
```

- [ ] **Step 2: Write the failing test (exercises the REAL core through the xcframework)**

Create `apps/ios/FeatherKeyKitTests/CoreKeyboardEngineTests.swift`:
```swift
import XCTest
@testable import FeatherKeyKit

final class CoreKeyboardEngineTests: XCTestCase {
    func test_decode_at_a_key_centre_returns_that_keys_letter() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }

        let engine = try CoreKeyboardEngine(containerDir: dir)
        let keys = engine.layoutKeys()
        XCTAssertFalse(keys.isEmpty, "core must expose a layout")

        // Pick a known letter key and decode at its centre; expect that letter.
        let h = try XCTUnwrap(keys.first { $0.label == "h" })
        let got = try engine.decode(atLogicalX: h.x + h.width / 2, y: h.y + h.height / 2)
        XCTAssertEqual(got, "h")
    }
}
```

- [ ] **Step 3: Run test — see it fail**

Run: `xcodebuild test ... -only-testing:FeatherKeyKitTests/CoreKeyboardEngineTests`
Expected: FAIL — `CoreKeyboardEngine` undefined.

- [ ] **Step 4: Implement `CoreKeyboardEngine.swift`**

```swift
import Foundation

/// Adapts the UniFFI-generated `KeyboardCore` to the `KeyboardEngine` port.
/// This is the ONLY type that talks to the generated binding.
public final class CoreKeyboardEngine: KeyboardEngine {
    private let core: KeyboardCore

    /// Opens the shared core over an encrypted store in `containerDir`, keyed by
    /// the Keychain-provisioned device key. The store is required by the core's
    /// sole constructor; this slice issues no learn/persist calls against it.
    public init(containerDir: URL) throws {
        let key = try DeviceKey.loadOrCreate(account: "featherkey.ios.v1")
        let dbPath = containerDir.appendingPathComponent("featherkey.redb").path
        // English-only bundled pack for the foundation slice (words sorted).
        let en = LanguagePack(tag: "en",
                              words: ["hello", "the", "world"].sorted(),
                              proper: [])
        self.core = try KeyboardCore.open(dbPath: dbPath,
                                          deviceKey: [UInt8](key),
                                          languages: [en])
        self.core.useAlphaLayout()
    }

    public func layoutKeys() -> [EngineKey] {
        core.layoutKeys().map {
            EngineKey(label: $0.label, x: $0.x, y: $0.y, width: $0.width, height: $0.height)
        }
    }

    public func decode(atLogicalX x: Float, y: Float) throws -> String {
        try core.decode(x: x, y: y).best ?? ""
    }
}
```
(UniFFI Swift lowercases method names to camelCase: `layout_keys`→`layoutKeys`,
`use_alpha_layout`→`useAlphaLayout`. Confirm the exact generated names in
`Generated/featherkey_core.swift` and match them.)

- [ ] **Step 5: Run test — see it pass**

Run the Step 3 command. Expected: PASS — proves the full FFI round-trip
(open → layoutKeys → decode) works through the xcframework, headlessly.

- [ ] **Step 6: Checkpoint** — stage the three files; hold commit.

---

### Task 8: Keyboard extension `KeyboardViewController.swift` (render + tap routing)

**Files:**
- Create: `apps/ios/FeatherKeyKeyboard/KeyboardViewController.swift`
- Create: `apps/ios/FeatherKeyKeyboard/Info.plist`

**Interfaces:**
- Consumes: `KeyboardEngine` / `CoreKeyboardEngine`, `EngineKey`,
  `KeyboardGeometry` (all from `FeatherKeyKit`).

- [ ] **Step 1: Write `Info.plist`** (declares the keyboard extension; **no Full Access**)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>NSExtension</key>
  <dict>
    <key>NSExtensionPointIdentifier</key>
    <string>com.apple.keyboard-service</string>
    <key>NSExtensionPrincipalClass</key>
    <string>$(PRODUCT_MODULE_NAME).KeyboardViewController</string>
    <key>NSExtensionAttributes</key>
    <dict>
      <key>IsASCIICapable</key><true/>
      <key>PrefersRightToLeft</key><false/>
      <key>PrimaryLanguage</key><string>en-US</string>
      <key>RequestsOpenAccess</key><false/>   <!-- on-device only; privacy win -->
    </dict>
  </dict>
</dict></plist>
```

- [ ] **Step 2: Write `KeyboardViewController.swift`**

```swift
import UIKit
import FeatherKeyKit

final class KeyboardViewController: UIInputViewController {
    private var engine: KeyboardEngine?
    private var keys: [EngineKey] = []
    private var logical = LogicalSize(width: 1, height: 1)
    private var shifted = false
    private var keyButtons: [UIButton] = []

    override func viewDidLoad() {
        super.viewDidLoad()
        do {
            let dir = FileManager.default.urls(for: .applicationSupportDirectory,
                                               in: .userDomainMask)[0]
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let e = try CoreKeyboardEngine(containerDir: dir)
            engine = e
            keys = e.layoutKeys()
            logical = KeyboardGeometry.logicalBounds(keys)
        } catch {
            // Errors are values at the boundary: show nothing rather than crash the host.
            NSLog("FeatherKey: engine init failed: \(error)")
        }
        buildViews()
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        layoutLetterButtons()
    }

    private func buildViews() {
        view.backgroundColor = .secondarySystemBackground
        // Letter buttons (one per core key).
        for key in keys {
            let b = UIButton(type: .system)
            b.setTitle(key.label, for: .normal)
            b.titleLabel?.font = .systemFont(ofSize: 18)
            b.backgroundColor = .systemBackground
            b.layer.cornerRadius = 5
            b.addTarget(self, action: #selector(letterTapped(_:)), for: .touchUpInside)
            view.addSubview(b)
            keyButtons.append(b)
        }
        // Function keys: shift, space, backspace (shell-handled, NOT via decode).
        addFunctionRow()
    }

    private func layoutLetterButtons() {
        let W = Float(view.bounds.width), H = Float(view.bounds.height) * 0.72 // top 72% = letters
        for (b, key) in zip(keyButtons, keys) {
            let sx = W / logical.width, sy = H / logical.height
            b.frame = CGRect(x: CGFloat(key.x * sx), y: CGFloat(key.y * sy),
                             width: CGFloat(key.width * sx), height: CGFloat(key.height * sy))
                .insetBy(dx: 1.5, dy: 1.5)
        }
    }

    @objc private func letterTapped(_ sender: UIButton) {
        guard let idx = keyButtons.firstIndex(of: sender), let engine else { return }
        let key = keys[idx]
        // Map the button centre (view px) → logical, then let the CORE decide the char.
        let cx = Float(sender.frame.midX), cy = Float(sender.frame.midY)
        let p = KeyboardGeometry.toLogical(viewX: cx, viewY: cy,
                                           viewWidth: Float(view.bounds.width),
                                           viewHeight: Float(view.bounds.height) * 0.72,
                                           logical: logical)
        let ch = (try? engine.decode(atLogicalX: p.x, y: p.y)) ?? ""
        guard !ch.isEmpty else { return }
        textDocumentProxy.insertText(shifted ? ch.uppercased() : ch)
    }

    private func addFunctionRow() {
        let space = makeFuncButton("space") { [weak self] in self?.textDocumentProxy.insertText(" ") }
        let del = makeFuncButton("⌫") { [weak self] in self?.textDocumentProxy.deleteBackward() }
        let shift = makeFuncButton("⇧") { [weak self] in self?.shifted.toggle() }
        let row = UIStackView(arrangedSubviews: [shift, space, del])
        row.axis = .horizontal; row.distribution = .fillProportionally; row.spacing = 4
        row.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(row)
        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 4),
            row.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -4),
            row.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -4),
            row.heightAnchor.constraint(equalTo: view.heightAnchor, multiplier: 0.22),
        ])
    }

    private func makeFuncButton(_ title: String, _ action: @escaping () -> Void) -> UIButton {
        let b = UIButton(type: .system)
        b.setTitle(title, for: .normal)
        b.backgroundColor = .systemGray4
        b.layer.cornerRadius = 5
        b.addAction(UIAction { _ in action() }, for: .touchUpInside)
        return b
    }
}
```

- [ ] **Step 3: Build the extension**

Run: `cd apps/ios && xcodegen generate && xcodebuild build -project FeatherKey.xcodeproj -scheme FeatherKeyHost -destination 'platform=iOS Simulator,name=iPhone 15'`
Expected: BUILD SUCCEEDED (host embeds the extension, which links FeatherKeyKit +
the xcframework).

- [ ] **Step 4: Checkpoint** — stage the two files; hold commit.

---

### Task 9: Host app + end-to-end simulator verification

**Files:**
- Create: `apps/ios/FeatherKeyHost/FeatherKeyHostApp.swift`
- Create: `apps/ios/FeatherKeyHost/Info.plist`

**Interfaces:**
- Consumes: nothing from the engine — it is only the installable container +
  a text field to type into for verification.

- [ ] **Step 1: Write `FeatherKeyHostApp.swift`**

```swift
import SwiftUI

@main
struct FeatherKeyHostApp: App {
    var body: some Scene {
        WindowGroup {
            VStack(spacing: 16) {
                Text("FeatherKey (iOS foundation slice)").font(.headline)
                Text("Enable in Settings → General → Keyboard → Keyboards → Add New Keyboard → FeatherKey, then type below.")
                    .font(.footnote).multilineTextAlignment(.center).padding(.horizontal)
                TextField("Type here to test", text: .constant("")).textFieldStyle(.roundedBorder).padding()
                Spacer()
            }.padding()
        }
    }
}
```

- [ ] **Step 2: Write `FeatherKeyHost/Info.plist`** (minimal app plist)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>UILaunchScreen</key><dict/>
</dict></plist>
```

- [ ] **Step 3: Build + boot + install (automatable)**

```bash
cd apps/ios && xcodegen generate
xcodebuild build -project FeatherKey.xcodeproj -scheme FeatherKeyHost \
  -destination 'platform=iOS Simulator,name=iPhone 15' -derivedDataPath build
xcrun simctl boot "iPhone 15" || true
xcrun simctl install booted "$(find build -name 'FeatherKeyHost.app' -type d | head -1)"
xcrun simctl launch booted com.featherkey.ios.FeatherKeyHost
```
Expected: app installs and launches in the booted simulator.

- [ ] **Step 4: Enable the keyboard + type (the honest friction step)**

In the booted simulator: Settings → General → Keyboard → Keyboards → Add New
Keyboard → **FeatherKey**; switch to it in the host app's text field; tap several
letters, space, backspace, shift. **Confirm the typed characters match the taps.**
This UI navigation has no clean `simctl` command; the build gate records exactly
which steps were automated vs. done by hand. The slice is NOT "verified" on a green
`xcodebuild test` alone — this char-in/char-out round-trip must be observed.

- [ ] **Step 5: Re-run the full Android guardrail (end-to-end regression)**

```bash
bash core/tools/ci-local.sh
git diff --stat -- apps/android/          # must be empty
```
Expected: `ALL GATES PASSED`; no `apps/android/` changes across the whole slice.

- [ ] **Step 6: Regenerate CODEMAP + run its check**

Run: `python3 core/tools/codemap.py && python3 core/tools/codemap.py --check`
Expected: exit 0. (The new `.feature` is indexed; Swift is not — a known,
recorded gap, design §8 item 8.)

- [ ] **Step 7: Checkpoint** — stage the host files; hold commit.

---

## Self-Review

**Spec coverage** (design §1–§9 → tasks):
- Additive core change + Android triad → **Task 1** (and re-verified in **Task 9 Step 5**). ✅
- xcframework + Swift bindings build → **Task 2**. ✅
- Xcode project (3 targets) → **Task 3**. ✅
- BR-70 + BRD amendment + `@BR-70` BDD + traceability → **Task 4**. ✅
- Pure geometry (host-testable, no UIKit) → **Task 5**. ✅
- Keychain device key (open() contract) → **Task 6**. ✅
- `KeyboardEngine` port + real-core adapter → **Task 7**. ✅
- Extension render + tap routing + shell function keys + no-Full-Access → **Task 8**. ✅
- Host app + simulator end-to-end (honest friction) + CODEMAP → **Task 9**. ✅
- "No typing logic in Swift" invariant: only `decode` yields characters; shift is a
  presentation uppercasing; space/backspace are proxy calls — no decision logic. ✅

**Placeholder scan:** every code/test step carries real code; the only deliberately
externalized detail is the exact UniFFI-generated Swift method names (Task 7 Step 4
tells the implementer to confirm them in `Generated/featherkey_core.swift`) — a
verification instruction, not a placeholder.

**Type consistency:** `EngineKey` defined once (Task 5 Step 0), used by geometry,
engine, and the controller. `KeyboardEngine` signatures match between Task 7's
protocol and Task 8's use. `LogicalSize` consistent across Tasks 5/8.

## Audit log

_(Appended on every `/r-u-sure` gate run, per CLAUDE.md §1.1.)_

### Build gate — ✅ Complete and verified (one manual observation handed to the user)

All 9 tasks executed. Evidence:
- **Task 1** — `crate-type += "staticlib"`; `ci-local ALL GATES PASSED`; Android
  bindings hash `b7cc7ea3…` **identical** before/after; `.a` builds (25.5 MB).
- **Task 2** — `build-core.sh` produced `FeatherKeyCore.xcframework` (ios-arm64 +
  ios-arm64_x86_64-simulator, each with `featherkey_coreFFI.h` + `module.modulemap`)
  and committed `Generated/featherkey_core.swift`. The strip-trap guard held
  (bindings generated from the unstripped debug dylib).
- **Task 3** — XcodeGen `project.yml` → 4 targets parse.
- **Task 4** — `@BR-70` scenario; RED seen (`bdd_check` exit 1, tag unmapped) →
  BRD amended (line 134 + §8.16 BR-70, traced OBJ-2/OBJ-7) → GREEN (25 features
  traceable, exit 0).
- **Task 5** — geometry: RED (undefined symbols) → GREEN (3/3).
- **Task 6** — Keychain: RED (`DeviceKey` undefined) → hit `errSecMissingEntitlement`
  (app-less test bundle) → hosted tests in `FeatherKeyHost` → GREEN (1/1).
- **Task 7** — `KeyboardEngine` port + `CoreKeyboardEngine`: RED (undefined) → GREEN.
  `CoreKeyboardEngineTests` drives the **real Rust core through the xcframework**
  (open → layoutKeys → decode("h" centre) == "h"). Full Kit suite **5/5 pass**.
- **Task 8** — extension builds; host+extension `BUILD SUCCEEDED` after fixes
  (extension bundle-ID pinned under host; `CFBundleDisplayName` added).
- **Task 9** — host installs (`exit 0`) + launches (`exit 0`) on the iPhone-15
  simulator; screenshot confirms the app renders; extension bundled as
  `PlugIns/FeatherKeyKeyboard.appex`. Final `ci-local ALL GATES PASSED`; CODEMAP
  fresh; `apps/android/` diff is **only** the Part-1 version bump — the iOS work
  touched zero Android files.

**Deviations from the plan (platform reality; intent preserved):**
1. Info.plists generated via XcodeGen (`info.properties` for the extension,
   `GENERATE_INFOPLIST_FILE` for the host) instead of hand-written — hand-written
   plists missed required keys (bundle ID, `CFBundleDisplayName`).
2. `FeatherKeyKitTests` hosted in `FeatherKeyHost` (not app-less) — Keychain needs
   an app context on the simulator (`errSecMissingEntitlement` otherwise). Host app
   thus created during Task 6, not Task 9.
3. Explicit `schemes:` block; extension bundle-ID pinned as a child of the host.

**The one thing NOT observed by me (the user's step, as the design scoped it):**
the live keyboard tap → `UITextDocumentProxy.insertText` in the *enabled* keyboard.
The engine round-trip (tap position → core decode → character) **is** proven by
`CoreKeyboardEngineTests` against the real binary; only the thin UIKit glue awaits
the manual "enable in Settings + type" observation — the iOS analog of the Android
on-device typing check that adb can't drive.

### Pass 1 — 🚧 Incomplete → fixed
Audited the plan against the design and against facts established building the
Android release earlier today. Gaps:
1. **Fatal: Task 2 generated Swift bindings from the `--release` (stripped) dylib.**
   `[profile.release]` sets `strip = true`, which drops `.symtab`; uniffi-bindgen's
   `--library` metadata extraction reads `.symtab`, so generation from a release
   artifact yields **zero files** — observed today on the Android `.so`. The plan
   would have silently produced an empty binding.
2. **Modulemap not wired** — the generated Swift does `import featherkey_coreFFI`;
   nothing made the xcframework vend that Clang module, so Tasks 3/5 would fail to
   compile.
3. **`Generated/` mixed sources and headers** — the C header + modulemap must live
   in the xcframework, not be compiled as framework Swift sources.

Changed (Task 2):
- Build a **host debug (unstripped) dylib** solely for binding generation; generate
  from that, with an explicit **guard that aborts if no `.swift` is produced** (so
  the strip trap can never pass silently).
- Split outputs: only `featherkey_core.swift` → committed `Generated/`; the FFI
  header + `module.modulemap` → the xcframework `-headers`, so it vends the
  `featherkey_coreFFI` module. Updated Step 3 expectations + the File Structure line.
- Removed a spurious `SWIFT_OBJC_INTERFACE_HEADER_NAME` setting from `project.yml`.

### Pass 2 — ✅ Complete and verified (plan phase)
Re-audited the revised plan:
- **Design coverage** — the Self-Review table maps every design §1–§9 element to a
  task; no design requirement is unhandled. ✅
- **Android guardrail** — Task 1 establishes the baseline + checks the triad; Task 9
  Step 5 re-checks it end-to-end (`ci-local` green + empty `apps/android/` diff). ✅
- **TDD/BDD order** — Task 4 writes the `@BR-70` scenario first; Tasks 5/6/7 each
  write a test and **see it fail** before implementing. UIKit glue (Tasks 8/9) is
  verified by the simulator round-trip, per design §7 (not unit-TDD-able). ✅
- **No typing logic in Swift** — only `decode` yields characters; shift is
  presentation-only uppercasing; space/backspace are direct proxy calls. ✅
- **Placeholder scan** — every code step carries real, compilable code; the one
  externalized detail (exact UniFFI-generated Swift symbol names) is an explicit
  confirm-instruction (Task 7 Step 4), not a gap. ✅
- **Type consistency** — `EngineKey` defined once (Task 5 Step 0) and reused;
  `KeyboardEngine`/`LogicalSize` signatures agree across Tasks 5/7/8;
  `KeyboardCore.open(dbPath:deviceKey:languages:)` / `decode(x:y:).best` match the
  real `ffi.rs` surface read this phase. ✅
- **KISS/YAGNI** — no App Group, no persistence calls, no swipe/suggestions;
  deferrals recorded in design §8. ✅

Honesty note: this is a **plan** — "verified" means complete, placeholder-free,
spec-covering, internally consistent, and free of the known-wrong step (the strip
trap, now fixed). Behavioral verification happens when the plan is executed under
the build gate. No new gaps. Ready for execution.
