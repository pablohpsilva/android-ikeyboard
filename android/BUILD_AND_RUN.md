# FeatherKey Android shell — build & run (Wave 5)

> **Read this first — honesty banner.** Everything under `android/` was **authored
> without an Android toolchain and has NOT been compiled, linted, or run.** Waves
> 0–4 (the Rust core under `crates/`) are verified and green; this shell is a
> careful, coherent scaffold you now build, fix, and verify on a real machine.
> Expect to iterate with the compiler — especially on the UniFFI-generated
> binding names and the touch-coordinate mapping. Where I was unsure I left a
> `⚠️` note or a `TODO` rather than guess silently.

## 0. What you have

A Gradle multi-module app wiring the verified Rust core to an Android IME:

```
app ─┬─ ime-service ─┬─ ffi-bridge ── (UniFFI bindings → featherkey-core cdylib)
     │               ├─ platform-services (Keystore key BR-62, EditorInfo BR-26)
     │               ├─ keyboard-view
     │               └─ accessibility-adapter
     ├─ onboarding (consent BR-22)
     └─ settings-ui (consent withdrawal + clear learned data)
```

The Rust→Kotlin surface is exported with **proc-macro UniFFI** on `featherkey-core`
(ADR-18). Its code lives in `ffi-bridge/rust-overlay/` and is applied into
`crates/featherkey-core/` on your machine (§3) — it is kept out of the verified
workspace so the sandbox build stays green and offline.

## 1. Prerequisites

- JDK 17, Android Studio (Ladybug+) or Gradle 8.9+, Android SDK (API 35), **NDK** (r27+).
- Rust targets: `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`
- `cargo install cargo-ndk` and `cargo install uniffi-bindgen` (or use the in-crate `uniffi-bindgen` bin from §3).

## 2. Generate the Gradle wrapper

There is intentionally no committed `gradlew` binary. From `android/`:
```
gradle wrapper --gradle-version 8.11
```
(The `android-shell` CI job is gated on `android/gradlew` existing, so it stays
dormant until this is done and pushed.)

## 3. Apply the Rust UniFFI overlay

Follow `ffi-bridge/rust-overlay/APPLY.md` exactly. In short:
1. Copy `ffi.rs`, `build.rs`, `uniffi.toml` into `crates/featherkey-core/`.
2. Add `#[cfg(feature = "uniffi")] mod ffi;` to `lib.rs`.
3. Add the `uniffi`/`thiserror` optional deps, the `uniffi` feature, `crate-type
   = ["lib","cdylib"]`, and the `uniffi-bindgen` bin to `Cargo.toml`.
4. Relax `featherkey-core`'s lint from workspace-inherited `unsafe_code = forbid`
   to its own `deny` (UniFFI scaffolding needs FFI `unsafe`; confine it to this
   one seam crate and record it as **ADR-19**).
5. **Sanity-check the verified core is still green:** `tools/ci-local.sh` (default
   features must stay unaffected — the `uniffi` feature is off by default).

## 4. Build the native library + bindings

From `crates/featherkey-core/` (with the overlay applied):
```
# Build the .so for each ABI into android/ffi-bridge/src/main/jniLibs/<abi>/
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o ../../android/ffi-bridge/src/main/jniLibs \
  build --release --features uniffi

# Generate the Kotlin bindings from the built library
cargo run --features uniffi --bin uniffi-bindgen -- generate \
  --library target/aarch64-linux-android/release/libfeatherkey_core.so \
  --language kotlin \
  --out-dir ../../android/ffi-bridge/src/main/kotlin
```
Then **reconcile** `ffi-bridge/.../FeatherKeyBridge.kt` against the actual
generated symbol names (constructor, error type, foreign-trait method casing).

## 5. Build, install, enable, verify

```
cd android && ./gradlew :app:installDebug
```
On the device/emulator: Settings → System → Languages & input → On-screen
keyboard → enable **FeatherKey** → switch to it in any text field.

**Manual acceptance checklist (the shell-side of the BRs):**
- [ ] Typing commits characters; space commits a word.
- [ ] BR-12: a real word is not clobbered; a typo is corrected on space.
- [ ] **BR-26 / E-2 (privacy Must):** type a word in a normal field → it becomes
      a suggestion later; type one in a **password field** → it never does.
      (This is the on-device confirmation of the property the Rust
      `tests/e2_sensitive_ordering.rs` already proves.)
- [ ] BR-22: first run shows plain-language consent; learning is OFF until opted
      in; Settings can withdraw consent and clear learned data.
- [ ] BR-62: uninstall/reinstall → the store cannot be decrypted with a new key
      (learned data is device-key-bound).
- [ ] BR-29/30/31: force a native error path → the host editor never crashes.

## 6. Known gaps you must close (flagged honestly)

1. **Layout — now a real QWERTY (closed the big one).** `layout-engine` exposes
   `Layout::qwerty()` (26-letter staggered block, 1000×360 logical), the core
   exposes `layout_keys()` + `use_alpha/numeric/symbols_layout()`, and
   `KeyboardView` renders those labeled keys and maps touches back into the same
   logical space — so what is drawn is what the core decodes. Remaining polish:
   - **No shift/caps** (all lowercase). `KeyId` is a character today; upper-case +
     a shift key is a kernel/layout follow-up (or handle caps shell-side).
   - **No layout for the digits/symbols glyphs beyond the single rows** the Rust
     fixtures provide; `?123` toggles alpha↔numeric only (extend to a symbols
     cycle). Long-press / accents / emoji are v1.x+ per the plan.
   - Tune key heights/margins and add key-press visual feedback for feel.
2. **UniFFI binding names** in `FeatherKeyBridge.kt` are best-effort guesses.
3. **Bundled lexicon** is a 6-word placeholder (`Lexicons.bundled`); package real
   sorted per-language word lists in `assets/`.
4. **Touch-model persistence** is not wired (Wave 4 note): only learned vocabulary
   survives restarts; tap-geometry does not (needs a `touch-model` serialize API).
5. **KeystoreKeyProvider** is unreviewed crypto; get a security pass (BR-28) before
   shipping.
6. **Accessibility / switch-access** is a minimal hook; TalkBack polish + BR-56 are
   v1.x depth per the plan.
