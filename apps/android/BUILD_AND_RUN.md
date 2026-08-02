# FeatherKey Android shell — build & run (Wave 5)

> **Read this first — honesty banner.** Everything under `android/` was **authored
> without an Android toolchain and has NOT been compiled, linted, or run.** Waves
> 0–4 (the Rust core under `core/crates/`) are verified and green; this shell is a
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
(ADR-18). Its code — `src/ffi.rs`, `src/ffi/ffi_types.rs`, `build.rs`, `uniffi.toml`,
and the `uniffi`/`crate-type`/bin entries in `Cargo.toml` — is **committed in-tree** on
`core/crates/featherkey-core/`. There is **nothing to apply**: a fresh clone already
has the shim. See §3 for why `ffi-bridge/rust-overlay/` still exists and why you must
NOT copy it over the committed sources.

## 1. Prerequisites

- JDK 17, Android Studio (Ladybug+) or Gradle 8.9+, Android SDK (API 35), **NDK** (r27+).
- Rust targets: `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`
- `cargo install cargo-ndk` and `cargo install uniffi-bindgen` (or use the workspace's own `uniffi-bindgen-tool` crate from §3).

## 2. Generate the Gradle wrapper

There is intentionally no committed `gradlew` binary. From `android/`:
```
gradle wrapper --gradle-version 8.11
```
(The `android-shell` CI job is gated on `apps/android/gradlew` existing, so it stays
dormant until this is done and pushed.)

## 3. The Rust UniFFI shim is committed — do NOT apply the overlay

> ⚠️ **Do not copy `ffi-bridge/rust-overlay/*` over the core crate.** The shim is
> already committed in-tree (see below). The overlay is a **stale historical seed**
> (its `ffi.rs` is an early 14-method snapshot); copying it clobbers the real,
> tested shim and breaks the build (`decode` needs `&mut`, no `correct` method,
> `learn_word` arity). If a `git ls-files` check ever makes you think a file is
> untracked, include the **full path** (`core/crates/featherkey-core/src/ffi.rs`,
> not `…/featherkey-core/ffi.rs`) before concluding anything is missing.

The following are committed on `master` and present in every fresh clone — nothing
to apply, no manual step:

- `core/crates/featherkey-core/src/ffi.rs` — the UniFFI `KeyboardCore` shim (with
  its inline test modules)
- `core/crates/featherkey-core/src/ffi/ffi_types.rs` — the FFI value types
- `#[cfg(feature = "uniffi")] mod ffi;` in `src/lib.rs`
- `build.rs`, `uniffi.toml`
- `Cargo.toml`: the `uniffi`/`thiserror` optional deps, the `uniffi` feature,
  and `crate-type = ["lib", "cdylib"]`
- `core/tools/uniffi-bindgen-tool/` — the standalone `uniffi-bindgen` bin, split
  out of `featherkey-core` so the shipped crate does not carry the `cli` feature
  tree (it generates from the built `.so` via `--library`, so it has no Rust
  dependency on `featherkey-core`)
- the crate's own `unsafe_code = "deny"` relaxation (**ADR-19**) — UniFFI
  scaffolding needs FFI `unsafe`, confined to this one seam crate

The `uniffi` feature is **off by default**, so the verified core stays green and
offline without it. Sanity-check any time with `tools/ci-local.sh` (from `core/`).

`ffi-bridge/rust-overlay/` and its `APPLY.md` are retained only as the historical
record of how the shim was first introduced — treat them as read-only documentation,
never as a build step.

## 4. Build the native library + bindings

From `core/crates/featherkey-core/` (no overlay step — the shim is already in-tree,
§3). The convenience script `apps/android/ffi-bridge/build-jni.sh` runs the ABI build
for you; the commands below are the manual equivalent:
```
# Build the .so for each ABI into apps/android/ffi-bridge/src/main/jniLibs/<abi>/
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o ../../../apps/android/ffi-bridge/src/main/jniLibs \
  build --release --features uniffi

# Generate the Kotlin bindings from the built library.
# NOTE: the workspace target dir is core/target/, NOT this crate's own dir — pass an
# ABSOLUTE --library path (a relative target/... resolves wrong and fails to open).
cargo run -p uniffi-bindgen-tool -- generate \
  --library "$(git rev-parse --show-toplevel)/core/target/aarch64-linux-android/release/libfeatherkey_core.so" \
  --language kotlin \
  --out-dir ../../../apps/android/ffi-bridge/src/main/kotlin
```
Then **diff the regenerated bindings against the committed
`ffi-bridge/.../generated/featherkey_core.kt`**: they must be **byte-identical**.
UniFFI method checksums embed the `///` doc-comment text verbatim, so any wording
drift breaks the checksum → dead bridge → no typing. A clean diff is the correctness
gate for a `.so` rebuild; if it differs, the committed Kotlin bindings won't link
against the new `.so`. Then reconcile the hand-written `FeatherKeyBridge.kt` wrapper
only if the generated symbol names actually changed.

## 5. Build, install, enable, verify

```
cd apps/android && ./gradlew :app:installDebug
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
