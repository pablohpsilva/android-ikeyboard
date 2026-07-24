# Rust overlay — activating the UniFFI surface on `featherkey-core`

⚠️ **Authored, not compiled.** These edits were written without a UniFFI/NDK
toolchain. Expect to iterate with the compiler on your machine. They are kept
here (not applied to `crates/`) so the sandbox-verified workspace stays green
and offline-buildable. Apply them where you have network + the Android NDK.

## What this overlay does
Turns the pure-Rust `featherkey-core` composition façade (Wave 4, verified) into
a `cdylib` exposing a UniFFI object `KeyboardCore`, per ADR-18. The FFI wrapper
owns the `RedbSecureStore` adapter opened from the device key the shell
provisions via the Android Keystore (BR-62).

## Steps

1. **Copy the module in:**
   ```
   cp android/ffi-bridge/rust-overlay/ffi.rs        crates/featherkey-core/src/ffi.rs
   cp android/ffi-bridge/rust-overlay/build.rs      crates/featherkey-core/build.rs
   cp android/ffi-bridge/rust-overlay/uniffi.toml   crates/featherkey-core/uniffi.toml
   ```

2. **`crates/featherkey-core/src/lib.rs`** — add, gated so the default (verified)
   build is unchanged:
   ```rust
   #[cfg(feature = "uniffi")]
   mod ffi;
   ```

3. **`crates/featherkey-core/Cargo.toml`** — add:
   ```toml
   [lib]
   crate-type = ["lib", "cdylib"]

   [features]
   uniffi = ["dep:uniffi", "dep:thiserror"]

   [dependencies]
   uniffi = { version = "0.28", optional = true }
   thiserror = { version = "2", optional = true }

   [build-dependencies]
   uniffi = { version = "0.28", features = ["build"] }

   [[bin]]
   name = "uniffi-bindgen"
   path = "uniffi-bindgen.rs"
   required-features = ["uniffi"]
   ```
   Create `crates/featherkey-core/uniffi-bindgen.rs`:
   ```rust
   fn main() { uniffi::uniffi_bindgen_main() }
   ```

4. **The `unsafe` carve-out.** UniFFI's generated scaffolding contains `unsafe`
   FFI shims, but the workspace sets `unsafe_code = "forbid"`. `forbid` cannot be
   locally relaxed, so change **`featherkey-core`'s** manifest from inheriting the
   workspace lints to its own that `allow` unsafe *only when the `uniffi` feature
   compiles it*. Simplest: drop `[lints] workspace = true` from
   `featherkey-core/Cargo.toml` and add:
   ```toml
   [lints.rust]
   unsafe_code = "deny"        # deny (overridable), not forbid
   missing_debug_implementations = "warn"
   [lints.clippy]
   unwrap_used = "warn"
   expect_used = "warn"
   panic = "warn"
   ```
   Then in `ffi.rs` the generated scaffolding module carries its own
   `#[allow(unsafe_code)]` where UniFFI emits it. This confines the unsafe to the
   FFI seam — architecturally the right place (the seam is where FFI unsafe and
   `crash-guard`'s `catch_unwind` live).

   > This is the ONE mandate-touching change in Wave 5. It weakens
   > `unsafe_code = forbid` → `deny` for the single composition/FFI crate. Record
   > it as ADR-19 when you ratify it (the seam legitimately needs FFI unsafe);
   > every other crate keeps `forbid`.

5. **Generate the bindings** (see `android/BUILD_AND_RUN.md` §3) with the
   `uniffi-bindgen` bin against the built `.so`, emitting `uniffi/featherkey/`
   Kotlin into `android/ffi-bridge/src/main/kotlin`.

6. **Re-run `tools/ci-local.sh`** with default features to confirm the verified
   build is still green (the `uniffi` feature is off by default, so it must be).
