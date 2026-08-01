# Performance, Size & Safety Hardening — Design

**Status:** design (awaiting approval → plan → build, each `/r-u-sure`-gated)
**Date:** 2026-08-02
**Branch:** `perf-size-safety-hardening` (off `master` `e3cfee2`)

## 1. Problem

FeatherKey ships and is device-accepted, but a multi-agent audit
(`scratchpad/perf-audit-report.md`, 6 scouts → adversarial vetting → synthesis)
found the app is larger than it needs to be, carries one latent FFI-safety
foot-gun, and is one FFI method away from a mechanical CI break. Every
opportunity below was independently re-verified against the code; several
inflated scout claims were downgraded by the vetting pass and are excluded here.

The goal is **smaller, safer, and cheaper-to-build without touching runtime
behaviour, hot-path latency, or readability.** Nothing here changes what the
keyboard does; it changes what ships and how it is built.

## 2. Requirements this closes

These are non-functional/tech-debt goals (no new BR); the audit is the source of
truth, cross-checked against the repo contract (CLAUDE.md, ARCHITECTURE.md).

| # | Goal | Evidence it is real |
|---|---|---|
| G1 | Shrink the shipped native payload | Core `.so` is 2.67 MB arm64, **unstripped** (`file` → "not stripped"); no `[profile.release]` exists anywhere in `core/`. |
| G2 | Stop shipping dead weight | `libredb-*.so` (498 KB arm64) is emitted by cargo-ndk but nothing links/loads it (core's `NEEDED` = libc/libm/libdl only; redb is static in the core `.so`). |
| G3 | Prevent an FFI panic-safety regression | `panic="abort"` would silently no-op UniFFI's `catch_unwind` — the *only* live FFI panic containment (`featherkey-crash-guard` is dead code: zero dependents, zero call sites — verified). |
| G4 | Pre-empt an imminent CI break | `ffi.rs` is **exactly 500 lines** = the fitness ceiling; the next FFI method fails `check.py`. |
| G5 | Harden the "errors are values" invariant | `unwrap_used`/`expect_used`/`panic` are `warn`, not `deny`; code is already clean under the CI gate but `cargo clippy --all-targets` fails 22× in 3 crates lacking test allow-headers. |
| G6 | Cut feature-build / CI-iteration cost | `uniffi`'s `cli`/bindgen tool is bundled into the shipped crate's `uniffi` feature, dragging ~34 cli-only crates through every `--features uniffi` build (`ci-local.sh:42`, `build-jni.sh`, `bindings_check.py`). |

**Explicit non-goals (audit red lines — must NOT do):**
- **No `panic = "abort"`** (G3). A guardrail comment is added where the profile lives.
- **No `opt-level = "s"/"z"`** — trades typing latency for size the strip already delivers safely. Keep `opt-level = 3`.
- No lexicon/asset shrinking (vetting rated it high-risk, not a win).
- No hot-path behavioural change; no readability regression.

## 3. Increments, modules involved, and whether they already exist

Four independent increments. Each is separately verifiable and separately
revertable. None depends on another; ordering below is by value/risk.

### Increment 1 — Release profile + drop orphaned `.so` (G1, G2, G3 guardrail)
- **`core/Cargo.toml`** (exists) — add a new `[profile.release]` section:
  ```toml
  [profile.release]
  opt-level = 3        # explicit: keep speed — do NOT lower for size
  strip = true         # G1: ~815 KB (30.5%) off arm64 core .so
  lto = "thin"         # size + speed; only lengthens build time
  codegen-units = 1    # smaller/faster code; only lengthens build time
  # DO NOT add `panic = "abort"`: it no-ops UniFFI's catch_unwind FFI panic
  # containment (crash-guard is not wired). See design G3.
  ```
- **`apps/android/ffi-bridge/build-jni.sh`** (exists) — after the cargo-ndk
  build, delete the inert byproduct: `find "$out_dir" -name 'libredb*.so' -delete`.
  Belt-and-suspenders (optional): a Gradle `packaging { jniLibs { excludes += "**/libredb*.so" } }` in the app module.
- No new module. CODEMAP unaffected (no symbol/feature/crate change).

### Increment 2 — Extract `ffi.rs` test modules (G4)
- **`core/crates/featherkey-core/src/ffi.rs`** (exists, 500 lines) — move its two
  `#[cfg(test)]` modules (~139 lines) into **`core/crates/featherkey-core/src/ffi/tests.rs`**
  (new file) via `#[cfg(test)] mod tests;`, exactly as `learn.rs`/`rank.rs` already do.
- Pure mechanical move; zero behaviour change; restores ~140 lines of headroom.

### Increment 3 — Promote panic lints to `deny` (G5)
- **`core/Cargo.toml`** (exists) `[workspace.lints.clippy]` — `warn` → `deny` for
  `unwrap_used`, `expect_used`, `panic`.
- Add `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` to the
  test modules that lack it in **`featherkey-editing`**, **`featherkey-dictionary`**,
  **`featherkey-layout-engine`** (the 3 crates where `clippy --all-targets` currently fails).
- Compile-time only; zero runtime effect.

### Increment 4 — Split the `uniffi` bindgen tool into its own crate (G6)
- **New crate `core/tools/uniffi-bindgen-tool`** (does NOT exist) — a workspace
  member whose `src/main.rs` is `fn main() { uniffi::uniffi_bindgen_main() }` with
  `uniffi = { version = "0.28", features = ["cli"] }`.
  - **Placed under `core/tools/`, not `core/crates/`, by design.** The fitness
    checker layer-validates only crates under `core/crates/` (`CRATES = REPO/"crates"`,
    globs `*/Cargo.toml`) — a build tool has no place in the inward-pointing domain
    layering, so it must not live there (`layer = "tooling"` would fail fitness's
    known-layer check; a default `domain` label would be a lie). Living under
    `core/tools/` keeps it out of layer enforcement while still being a workspace
    member (so `cargo run -p uniffi-bindgen-tool` works and it shows honestly in the
    crate map, which enumerates *members*, not just `crates/`). Set
    `[package.metadata.featherkey] layer = "tooling"` for display only.
- **`core/crates/featherkey-core/Cargo.toml`** (exists) — drop `features = ["cli"]`
  from its `uniffi` dep (keep `uniffi = { version = "0.28", optional = true }` and the
  existing `uniffi = ["dep:uniffi", "dep:thiserror"]` feature — `#[uniffi::export]`
  needs only `uniffi_macros` (default), not `cli`); remove the `[[bin]] uniffi-bindgen`
  stanza (L20–22) and delete `uniffi-bindgen.rs`.
- **`core/tools/bindings_check.py`** (exists) — retarget: `CRATE_DIR` (L51) and the
  cargo invocation (L125, `--features uniffi --bin uniffi-bindgen`) become
  `cargo run -p uniffi-bindgen-tool -- generate …` (no `--features uniffi`).
- **`apps/android/BUILD_AND_RUN.md`** (exists, L35/L65/L90) — update the documented
  bindgen commands. (`test_bindings_check.py:4` mentions `uniffi-bindgen` only in a
  comment — no change. `build-jni.sh` does not invoke bindgen — no change there.)
- **`core/Cargo.toml`** `members` (exists) — add `"tools/uniffi-bindgen-tool"`. CODEMAP regenerates.
- **Confirm during build (audit-flagged):** the standalone tool must read
  featherkey-core's UniFFI metadata purely via `--library <path-to-.so>` with no Rust
  path-dependency back on featherkey-core; regenerated bindings must diff empty.

## 4. Invariants (must hold after every increment)

- **FFI bindings byte-identical** — `python3 core/tools/bindings_check.py --check`
  passes unchanged (Increment 4's direct regression gate; also guards 1–3).
- **Rust core imports no Android/JNI types** — fitness unchanged.
- **Coverage ≥ 98% workspace line** — no increment removes tested code (Increment 2
  moves tests verbatim; Increment 3 is lints; 1 & 4 are build config).
- **Fitness ≤ 500 lines/file, ≤ 60 lines/function** — Increment 2 *improves* this;
  the new bindgen `main.rs` is a one-liner.
- **No hot-path behavioural change** — increments touch build config, test-module
  location, lints, and a tooling crate only; no library logic changes.
- **`bash core/tools/ci-local.sh` exits 0** — the whole-gate check after each increment.

## 5. Verification plan (evidence the gate will demand)

- **Size (G1/G2):** release `.so` byte size before/after strip (`ls -l`, `file`
  "stripped"); confirm the **dynamic** symbol table is byte-identical (`llvm-objdump -T`
  diff) so FFI resolution is provably unaffected; APK size before/after; confirm
  `libredb*.so` absent from `jniLibs` and the built APK.
- **Safety (G3):** assert no `panic = "abort"` in the profile; guardrail comment present.
- **CI-break (G4):** `python3 core/tools/fitness/check.py` exit 0 with `ffi.rs` < 500.
- **Lints (G5):** `cargo clippy --workspace --all-targets -- -D warnings` exit 0.
- **Bindgen split (G6):** `bindings_check.py --check` byte-identical; regenerated
  bindings diff empty; `ci-local.sh` exit 0; CODEMAP regenerated + committed.
- **On-device smoke:** install the release build, verify the keyboard types,
  swipes, and shows suggestions (the accepted behaviour) — the user's step.

## 6. Alternatives rejected

- **`opt-level = "s"/"z"`** — rejected: the repo's own perf history flags typing
  latency as sensitive; strip delivers the size win without touching speed.
- **`panic = "abort"`** — rejected as an active hazard (G3), not merely declined.
- **Gradle-only `jniLibs` exclude** for the redb `.so` — kept only as optional
  belt-and-suspenders; the build-script delete is the reproducible primary.
- **Deleting `featherkey-crash-guard`** — out of scope; it is dead but harmless,
  and wiring it (the real fix for FFI panic containment) is a separate design.
- **Shrinking lexicon/proper-noun assets** — rejected by vetting as high-risk for
  a modest, quality-affecting gain.
- **Merging the uniffi bindgen tool's deps away entirely** — impossible; most of
  the tree is mandatory `uniffi_macros`, not `cli`. The split targets only the
  ~34 cli-exclusive crates (build-time/supply-chain win, negligible runtime).

## 7. Audit log

### Pass 1 — ⚠️ Done but unverified → gaps fixed
Gaps found (against the code, not the audit report):
- **Increment 4 named an invalid layer.** `layer = "tooling"` is not in
  `LAYER_RANK` (`foundation/port/domain/adapter/composition`); fitness fails
  unknown layers. Root cause: a build tool doesn't belong in the inward domain
  layering at all.
- **Increment 4 mislocated the crate.** Placing it under `core/crates/` forces
  fitness layer-validation on a non-domain tool. Fitness scopes to `core/crates/*`;
  codemap enumerates all workspace *members*.
- **Increment 4 under-listed the references to retarget.** Missed
  `bindings_check.py` `CRATE_DIR` (L51) and `BUILD_AND_RUN.md` L35/L65; `build-jni.sh`
  does not call bindgen (wrong target); `test_bindings_check.py:4` is comment-only.

Changed:
- Increment 4 now places the crate at **`core/tools/uniffi-bindgen-tool`** (member,
  not under `crates/`), `layer = "tooling"` for display only, with the fitness-scope
  rationale spelled out.
- Increment 4 now lists the exact edit sites (`bindings_check.py` L51 + L125,
  `BUILD_AND_RUN.md` L35/L65/L90) and the `#[uniffi::export]`-still-works reasoning.
- Added the "confirm `--library` decoupling / bindings diff empty" build-time check.

Verified this pass: `[profile.release]` absent (self-run grep); `ffi.rs` == 500
(`wc -l`); `crash-guard` dead (0 dependents, 0 call sites); the 3 lint crates lack
test allow-headers; `out_dir` is the real var in `build-jni.sh`.

Not yet verified (correctly deferred to build, not design): actual stripped `.so`
size delta, APK size delta, `--library` decoupling of the split tool, and that the
regenerated bindings diff empty. These need a real NDK/Gradle build.

### Verdict: ⚠️ Done but unverified
Design is complete and internally consistent; the four increments are sound and the
one non-trivial item (Increment 4) is now corrected against how fitness/codemap
actually work. Remaining unknowns are all build-time measurements, which belong to
the build phase, not the design. Ready to advance to the plan.
