# FeatherKey — Android shell

The Kotlin/Android half of the keyboard: the thin, replaceable adapter layer
around the Rust core (see `SOFTWARE_ENGINEERING.md` §5.1 and `ARCHITECTURE.md`
§9 Ports & Adapters). All typing intelligence lives in the Rust core under
`../crates`; this shell only renders, captures touch, marshals across the FFI,
commits text, and owns Android-specific concerns (Keystore, EditorInfo, consent,
accessibility).

## Status: authored (Wave 5), NOT yet compiled

> **These modules have not been built, linted, or run.** The dev sandbox has no
> JDK/Gradle/Android SDK/NDK, and UniFFI binding generation needs the NDK. Waves
> 0–4 (the Rust core) are verified and green; this shell is a coherent scaffold
> to build and verify on a machine with the Android toolchain. **Start with
> [`BUILD_AND_RUN.md`](BUILD_AND_RUN.md)** — it has the exact steps, the honesty
> banner, and the list of known gaps. The `android-shell` CI job stays dormant
> until `android/gradlew` exists (generate it per BUILD_AND_RUN §2).

## Modules (SEDD §5.1)

| Module | Single responsibility |
|---|---|
| `app` | Composition root: manifest, IME + settings entry points |
| `ime-service` | `InputMethodService`: touch → native decode → commit; word-boundary correct + **gated learning (E-2/BR-26)** |
| `keyboard-view` | Render keys; capture touch; map to the Rust layout's logical space |
| `ffi-bridge` | Curated wrapper over the UniFFI-generated `KeyboardCore` bindings |
| `platform-services` | Driven-port impls: Keystore key provisioning (**BR-62**), `EditorInfo` sensitivity (**BR-26**) |
| `onboarding` | First-run plain-language consent, opt-in learning (**BR-22**) |
| `settings-ui` | Withdraw consent, clear learned data (**BR-22**) |
| `accessibility-adapter` | Minimal TalkBack announce hook |

## The input path

```
MotionEvent (x,y)        keyboard-view   → maps to logical layout coords
      │
      ▼
FeatherKeyBridge.decode  ffi-bridge      → UniFFI → featherkey-core (Rust)
      │                                     decode / suggest / correct / learn
      ▼
commitText(...)          ime-service     → InputConnection; learn is E-2-gated
```

The Rust side of every step is real, tested, and green under `../crates`
(notably `featherkey-core/tests/e2_sensitive_ordering.rs` for the BR-26 gate).
Wave 5 is wiring the Kotlin side to it and verifying on device.

## The Rust ⇄ Kotlin seam (ADR-18)

The core surface is exported with **proc-macro UniFFI** directly on
`featherkey-core`. The export code is staged in
[`ffi-bridge/rust-overlay/`](ffi-bridge/rust-overlay/APPLY.md) and applied into
`crates/featherkey-core/` at build time so the sandbox-verified workspace stays
green and offline-buildable. The old `ffi-bridge/src/featherkey.udl` (Wave-0
tracer) is superseded.
