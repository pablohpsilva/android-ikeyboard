# FeatherKey — Android shell

The Kotlin/Android half of the keyboard: the thin, replaceable adapter layer
around the Rust core (see `SOFTWARE_ENGINEERING.md` §5.1 and `ARCHITECTURE.md`
§9 Ports & Adapters). All typing intelligence lives in the Rust core under
`../crates`; this shell only renders, captures touch, marshals across the FFI,
and commits text to the editor.

## Status: scaffold (not yet wired to a build)

> **These modules are not compiled by CI yet.** The repository's toolchain
> currently builds and tests the Rust core; the Android build requires a JDK,
> Gradle, and the Android SDK/NDK that are not present in the development
> sandbox. The CI job `android-shell` (`.github/workflows/ci.yml`) is defined
> but gated on `android/gradlew` existing, so it stays dormant until the Gradle
> wrapper and module `build.gradle.kts` files are completed and verified on a
> machine with the Android toolchain. Nothing here has been compiled — treat it
> as an interface sketch, not working code.

## Modules (tracer-bullet subset)

| Module | Single responsibility | SEDD ref |
|---|---|---|
| `keyboard-view` | Render keys; capture touch; hand coordinates to the bridge | §5.1 |
| `ffi-bridge` | Marshal calls between Kotlin and the Rust core (UniFFI/JNI) | §5.1 |
| `ime-service` | `InputMethodService` lifecycle; commit the decoded character | §5.1 |

The remaining shell modules from SEDD §5.1 (`settings-ui`, `onboarding`,
`accessibility-adapter`, `platform-services`) are added as their features land.

## The tracer-bullet path

```
MotionEvent (touch)          keyboard-view   → captures (x, y)
      │
      ▼
FeatherKeyBridge.decode(x,y) ffi-bridge      → calls into Rust core
      │                                         (featherkey-input-decoder)
      ▼
committed char               ime-service     → InputConnection.commitText(...)
```

The Rust side of this path is real, tested, and green:
`crates/input-decoder/tests/tracer_bullet.rs`. Wiring the Kotlin side to it via
UniFFI is the next milestone once the Android toolchain is available.
