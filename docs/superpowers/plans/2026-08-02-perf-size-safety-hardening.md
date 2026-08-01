# Performance, Size & Safety Hardening — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development — one fresh implementer subagent per task, task review between tasks, on branch `perf-size-safety-hardening`. Steps use `- [ ]` for tracking.

**Goal:** Ship a smaller, safer, cheaper-to-build FeatherKey with zero change to runtime behaviour, hot-path latency, or readability.

**Architecture:** Four independent increments — (1) release profile + drop an inert `.so`, (2) relocate `ffi.rs` tests to restore fitness headroom, (3) harden panic lints to `deny`, (4) split the uniffi bindgen tool into its own workspace-member crate. Each is separately verifiable and revertable; land in order 1→4 on one branch.

**Tech Stack:** Rust (Cargo workspace under `core/`), UniFFI 0.28 proc-macros, cargo-ndk + Android NDK 28.2, Python fitness/codemap/bindings tooling, Gradle/Kotlin shell.

**Design:** `docs/superpowers/specs/2026-08-02-perf-size-safety-hardening-design.md` (gated).

## Global Constraints

Every task's requirements implicitly include these — exact values, copied verbatim:

- **NEVER add `panic = "abort"`** to any Cargo profile (it no-ops UniFFI's `catch_unwind` FFI panic containment; `crash-guard` is not wired). A guardrail comment must sit where the profile lives.
- **Keep `opt-level = 3`** in `[profile.release]` — do NOT lower to `"s"`/`"z"` (typing latency).
- **FFI bindings must stay byte-identical:** `python3 core/tools/bindings_check.py --check` passes with no diff after every task.
- **Rust core imports no Android/JNI types** (fitness-enforced).
- **Errors are values:** no `unwrap`/`expect`/`panic` in non-test library code.
- **Fitness:** ≤ 500 lines/file, ≤ 60 lines/function (`python3 core/tools/fitness/check.py` exit 0).
- **Coverage ≥ 98% workspace line** (no task removes tested code).
- **CODEMAP is generated** — never hand-edit; regenerate with `python3 core/tools/codemap.py` and `git add CODEMAP.md` on any `.rs`/`.kt`/`.feature`/`Cargo.toml`/`settings.gradle.kts` change.
- **Per-task gate:** `bash core/tools/ci-local.sh` exits 0 (ALL GATES PASSED) before a task is done.
- **No AI attribution** anywhere (commits, PRs, comments).
- **Branch `perf-size-safety-hardening`; never commit to master.** No BR/Gherkin applies — these are non-functional/tech-debt changes closing no business requirement; the "tests" are the gates named per task.

---

### Task 1: Release profile + drop the orphaned `libredb-*.so`

**Files:**
- Modify: `core/Cargo.toml` (add a `[profile.release]` section — none exists today)
- Modify: `apps/android/ffi-bridge/build-jni.sh:33` (delete the inert byproduct after the cargo-ndk build)

**Interfaces:**
- Consumes: nothing.
- Produces: a stripped, LTO'd release profile; a `jniLibs` tree free of `libredb-*.so`. No source-symbol, feature, or crate change — CODEMAP unaffected.

> **Verification note (gate-corrected):** the shipped artifact is the **Android
> ELF** from `cargo ndk`, not a host build. On macOS a host `cargo build` of this
> `cdylib` yields a Mach-O `.dylib`, and `nm -D`/`file "stripped"` are ELF-only — so
> all size/strip evidence below builds the real arm64 ELF. The toolchain is present
> (cargo-ndk 4.1.2, `aarch64-linux-android` target, NDK 28.2 llvm tools). Baseline:
> the committed `jniLibs/arm64-v8a/libfeatherkey_core.so` is **2,674,400 bytes,
> "not stripped"**. `NM=~/Library/Android/sdk/ndk/28.2.13676358/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-nm`.

- [ ] **Step 1: Record the "before" evidence (this is the failing state).**

```bash
ls -l apps/android/ffi-bridge/src/main/jniLibs/arm64-v8a/libfeatherkey_core.so
file apps/android/ffi-bridge/src/main/jniLibs/arm64-v8a/libfeatherkey_core.so   # expect: "not stripped"
```
Expected: ~2.67 MB and **not stripped** — the waste this task removes. (If the committed `.so` is absent, build it first with the Step-3 command minus the profile change.)

- [ ] **Step 2: Add the release profile to `core/Cargo.toml`.**

Append after the `[workspace.lints.clippy]` block:
```toml
# Release build: strip symbols and LTO for a smaller, faster shipped .so.
# The default release profile ships the full symbol table (~815 KB on arm64)
# and no LTO. See docs/superpowers/specs/2026-08-02-perf-size-safety-hardening-design.md.
[profile.release]
opt-level = 3        # keep speed — typing latency is sensitive; do NOT lower to "s"/"z"
strip = true         # drop .symtab/.strtab; dynamic (FFI) symbols are unaffected
lto = "thin"         # cross-crate inlining; lengthens build time only
codegen-units = 1    # smaller/faster codegen; lengthens build time only
# DO NOT add `panic = "abort"`: it turns UniFFI's catch_unwind FFI panic
# containment into a no-op (crash-guard is not wired). See design §G3.
```

- [ ] **Step 3: Build the arm64 release ELF with the new profile and verify the strip win.**

```bash
cd core/crates/featherkey-core
ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/28.2.13676358 \
  cargo ndk -t arm64-v8a -o /tmp/fk-strip build --release --locked --features uniffi
ls -l /tmp/fk-strip/arm64-v8a/libfeatherkey_core.so
file /tmp/fk-strip/arm64-v8a/libfeatherkey_core.so   # expect: "stripped"
```
Expected: `file` reports **stripped** and size is materially below the 2,674,400-byte baseline (design predicts ~1.86 MB). Record both numbers in the report.

- [ ] **Step 4: Confirm the dynamic (FFI) symbol table survived the strip.**

```bash
NM=~/Library/Android/sdk/ndk/28.2.13676358/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-nm
$NM -D /tmp/fk-strip/arm64-v8a/libfeatherkey_core.so | grep -c UNIFFI_META
python3 core/tools/bindings_check.py --check   # byte-identical (bindings build debug — profile can't touch them)
```
Expected: `UNIFFI_META_*` exports still present (non-zero) — strip removed only `.symtab`/`.strtab`; `bindings_check` byte-identical.

- [ ] **Step 5: Drop the orphaned `libredb-*.so` in the Android build.**

In `apps/android/ffi-bridge/build-jni.sh`, immediately before the final `find … -exec ls` (line 33), insert:
```bash
# redb declares crate-type=["cdylib","rlib"] for its optional Python bindings, so
# cargo-ndk emits a standalone libredb-*.so that nothing links or loads (redb is
# compiled statically into libfeatherkey_core.so). Drop the dead weight (~498 KB arm64).
find "$out_dir" -name 'libredb*.so' -delete
```

- [ ] **Step 6: Verify the `.so` build (best-effort; needs the NDK).**

```bash
ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/28.2.13676358 bash apps/android/ffi-bridge/build-jni.sh
find apps/android/ffi-bridge/src/main/jniLibs -name 'libredb*.so'   # expect: no output
file apps/android/ffi-bridge/src/main/jniLibs/arm64-v8a/libfeatherkey_core.so  # expect: stripped
```
Expected: `libredb*.so` **absent**; core `.so` **stripped** and smaller. If the NDK is unavailable in this environment, report that Step 6 was not run and hand it to the controller — Steps 1–4 already prove `strip` works on the host `.so`.

- [ ] **Step 7: Full gate + commit.**

```bash
bash core/tools/ci-local.sh   # ALL GATES PASSED
git add core/Cargo.toml apps/android/ffi-bridge/build-jni.sh
git commit -m "build(core): strip+thin-LTO release profile; drop inert libredb .so"
```

**Definition of Done:** host `.so` reports `stripped` and is smaller (numbers recorded); `UNIFFI_META_*` exports intact; `bindings_check.py --check` byte-identical; `build-jni.sh` deletes `libredb*.so`; no `panic = "abort"` present; guardrail comment present; `ci-local.sh` exit 0.

**Rollback:** `git revert` the commit — removing the profile restores default release behaviour; removing the `find -delete` line restores the (harmless) byproduct.

---

### Task 2: Extract `ffi.rs` test modules to restore fitness headroom

**Files:**
- Modify: `core/crates/featherkey-core/src/ffi.rs` (currently **exactly 500 lines** — the fitness ceiling)
- Create: `core/crates/featherkey-core/src/ffi/tests.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `ffi.rs` back under the 500-line ceiling; the same tests, relocated, still running. No public-symbol change.

- [ ] **Step 1: Confirm the failing-soon state.**

```bash
wc -l core/crates/featherkey-core/src/ffi.rs   # expect: 500 (zero headroom)
python3 core/tools/fitness/check.py            # currently passes at exactly 500
```

- [ ] **Step 2: Move both test modules into `src/ffi/tests.rs`.**

`ffi.rs` lines **362–500** hold two `#[cfg(test)]` modules: `mod tests` (362–389) and `mod autocorrect_outcome_tests` (392–500). Follow the existing `learn.rs`/`rank.rs` pattern (`#[cfg(test)] mod tests;`). Replace lines 362–500 of `ffi.rs` with:
```rust
#[cfg(test)]
mod tests;
```
Create `core/crates/featherkey-core/src/ffi/tests.rs` containing the bodies of BOTH former modules — keep the first module's test fns at the file's top level (it *is* the `tests` module now) and nest the second as `mod autocorrect_outcome_tests { … }`. Fix `use` paths: inside `ffi::tests`, `super` resolves to `ffi`, so `use super::*;` continues to work for the outer tests; the nested module uses `use super::super::*;` (or an explicit `use crate::ffi::*;`). Preserve any `#[cfg(feature = "uniffi")]` gating the tests already carry.

- [ ] **Step 3: Verify tests still run and pass.**

```bash
cd core && cargo test -p featherkey-core --features uniffi 2>&1 | tail -5
```
Expected: the same test count as before, all passing (no tests dropped or skipped).

- [ ] **Step 4: Verify fitness headroom restored.**

```bash
wc -l core/crates/featherkey-core/src/ffi.rs           # expect: ~362 (was 500)
python3 core/tools/fitness/check.py                    # exit 0
```

- [ ] **Step 5: Regenerate CODEMAP, full gate, commit.**

```bash
python3 core/tools/codemap.py
bash core/tools/ci-local.sh   # ALL GATES PASSED
git add core/crates/featherkey-core/src/ffi.rs core/crates/featherkey-core/src/ffi/tests.rs CODEMAP.md
git commit -m "refactor(core): extract ffi.rs test modules to ffi/tests.rs (fitness headroom)"
```

**Definition of Done:** `ffi.rs` < 500 lines; identical test set passes under `--features uniffi`; fitness exit 0; CODEMAP regenerated + staged; `bindings_check.py --check` byte-identical (unchanged public surface); `ci-local.sh` exit 0.

**Rollback:** `git revert` — the two modules move back inline; behaviour never changed.

---

### Task 3: Promote panic lints from `warn` to `deny`

**Files:**
- Modify: `core/Cargo.toml` (`[workspace.lints.clippy]`)
- Modify: `core/crates/featherkey-core/Cargo.toml` (its OWN `[lints.clippy]` table — it overrides the workspace one, so it must change too)
- Modify: test modules in `core/crates/editing`, `core/crates/dictionary`, `core/crates/layout-engine` that lack `#[allow(clippy::unwrap_used, …)]`

**Interfaces:**
- Consumes: nothing.
- Produces: `unwrap_used`/`expect_used`/`panic` are hard `deny` workspace-wide; `cargo clippy --all-targets` is clean. Compile-time only — zero runtime effect.

- [ ] **Step 1: Confirm the failing state.**

```bash
cd core && cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep -c 'error:' 
```
Expected: non-zero (audit measured 22 errors in `editing`/`dictionary`/`layout-engine` — their test modules use `unwrap()` without the allow-header).

- [ ] **Step 2: Add the allow-header to the offending test modules.**

For each `#[cfg(test)]` module flagged by Step 1 in the three crates, add directly under the `#[cfg(test)]` line (matching the convention already used elsewhere, e.g. `codec.rs`):
```rust
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
```
Let Step 1's diagnostics enumerate the exact files/lines; fix every one.

- [ ] **Step 3: Flip the lints to `deny` in both tables.**

`core/Cargo.toml` `[workspace.lints.clippy]` and `core/crates/featherkey-core/Cargo.toml` `[lints.clippy]`:
```toml
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
```

- [ ] **Step 4: Verify green (both the CI gate and the plain all-targets run).**

```bash
cd core && cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
```
Expected: **zero** errors — clean.

- [ ] **Step 5: Full gate + commit.**

```bash
bash core/tools/ci-local.sh   # ALL GATES PASSED (it already -D warnings on lib/bins)
git add core/Cargo.toml core/crates/featherkey-core/Cargo.toml core/crates/editing core/crates/dictionary core/crates/layout-engine
git commit -m "chore(core): deny unwrap/expect/panic lints; add missing test allow-headers"
```

**Definition of Done:** `cargo clippy --workspace --all-targets -- -D warnings` exit 0; all three lints `deny` in both the workspace table and featherkey-core's local table; `ci-local.sh` exit 0; no non-test library code needed an `#[allow]` (if any did, that is a real defect — stop and report, do not paper over it).

**Rollback:** `git revert` — lints return to `warn`; the added `#[allow]` headers are harmless if left.

---

### Task 4: Split the uniffi bindgen tool into its own workspace crate

**Files:**
- Create: `core/tools/uniffi-bindgen-tool/Cargo.toml`, `core/tools/uniffi-bindgen-tool/src/main.rs`
- Modify: `core/Cargo.toml` (`members` — add `"tools/uniffi-bindgen-tool"`)
- Modify: `core/crates/featherkey-core/Cargo.toml` (drop `features=["cli"]`; remove `[[bin]] uniffi-bindgen`)
- Delete: `core/crates/featherkey-core/uniffi-bindgen.rs`
- Modify: `core/tools/bindings_check.py` (`CRATE_DIR` L51 context; cargo invocation L125)
- Modify: `apps/android/BUILD_AND_RUN.md` (L35, L65, L90)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: a standalone `uniffi-bindgen-tool` binary that generates the Kotlin bindings via `--library`; `featherkey-core` no longer drags the `cli` feature tree through `--features uniffi` builds. The generated bindings must be byte-identical.

- [ ] **Step 1: Confirm the current bindgen path works (baseline).**

```bash
python3 core/tools/bindings_check.py --check   # baseline: byte-identical, exit 0
```

- [ ] **Step 2: Create the tool crate.**

`core/tools/uniffi-bindgen-tool/Cargo.toml`:
```toml
[package]
name = "uniffi-bindgen-tool"
version = "0.0.0"
publish = false
edition.workspace = true
license.workspace = true
rust-version.workspace = true
description = "Standalone UniFFI bindgen CLI, split out of featherkey-core so the shipped crate does not carry the cli feature tree."

[package.metadata.featherkey]
layer = "tooling"   # display-only; fitness scopes layer checks to core/crates/*, not tools/

[[bin]]
name = "uniffi-bindgen"
path = "src/main.rs"

[dependencies]
uniffi = { version = "0.28", features = ["cli"] }
```
`core/tools/uniffi-bindgen-tool/src/main.rs`:
```rust
fn main() {
    uniffi::uniffi_bindgen_main()
}
```

- [ ] **Step 3: Register the member and strip `cli` from featherkey-core.**

In `core/Cargo.toml` `members`, add `"tools/uniffi-bindgen-tool"`. In `core/crates/featherkey-core/Cargo.toml`: change the dep to `uniffi = { version = "0.28", optional = true }` (drop `features=["cli"]`); delete the `[[bin]] name = "uniffi-bindgen"` stanza; delete the file `core/crates/featherkey-core/uniffi-bindgen.rs`. Leave the `uniffi = ["dep:uniffi", "dep:thiserror"]` feature intact — `#[uniffi::export]` needs `uniffi_macros` (default), not `cli`.

- [ ] **Step 4: Verify `#[uniffi::export]` still compiles without `cli`.**

```bash
cd core && cargo build -p featherkey-core --features uniffi 2>&1 | tail -3
```
Expected: builds clean — the FFI overlay compiles with the macro feature alone.

- [ ] **Step 5: Retarget the bindgen invocation (minimal — `--library` mode already in use).**

`bindings_check.py:regenerate_to()` already (a) builds the cdylib via `cargo build --features uniffi` in `CRATE_DIR` (L117) and (b) generates from it with `--library <lib>` (L127). The split needs ONLY the runner (L124–125) changed — keep every other arg (`--library lib`, `--language kotlin`, `--no-format`, `--out-dir`) and the L117 build and `CRATE_DIR` exactly as-is:
```python
    _run([
        "cargo", "run", "--quiet", "-p", "uniffi-bindgen-tool",
        "--", "generate",
        "--library", lib,
        "--language", "kotlin",
        "--no-format",
        "--out-dir", out_dir,
    ])
```
Update the L51 comment (featherkey-core no longer "carries the uniffi-bindgen bin"). Because generation is already `--library`-based, the tool has NO Rust path-dependency on featherkey-core — its input is the built `.so`.

- [ ] **Step 6: Prove the bindings are byte-identical (the load-bearing gate).**

```bash
python3 core/tools/bindings_check.py --check   # MUST be byte-identical, exit 0
```
Expected: empty diff. If the regenerated file differs by even one byte, STOP and report — the split changed the output, which is a regression.

- [ ] **Step 7: Update the docs.**

`apps/android/BUILD_AND_RUN.md` L35/L65/L90 — replace the `--features uniffi --bin uniffi-bindgen` commands with the new `cargo run -p uniffi-bindgen-tool` form.

- [ ] **Step 8: Regenerate CODEMAP, full gate, commit.**

```bash
python3 core/tools/codemap.py
bash core/tools/ci-local.sh   # ALL GATES PASSED (this runs bindings_check --check)
git add core/Cargo.toml core/crates/featherkey-core/Cargo.toml core/tools/uniffi-bindgen-tool core/tools/bindings_check.py apps/android/BUILD_AND_RUN.md CODEMAP.md
git rm core/crates/featherkey-core/uniffi-bindgen.rs
git commit -m "build(core): split uniffi bindgen into its own tool crate; drop cli from featherkey-core"
```

**Definition of Done:** `bindings_check.py --check` byte-identical; `featherkey-core --features uniffi` builds without `cli`; new crate is a workspace member under `core/tools/`; CODEMAP regenerated + staged; `ci-local.sh` exit 0; docs updated; `uniffi-bindgen.rs` removed.

**Coverage watch (Task 4 only):** the new `main.rs` is a one-line passthrough to `uniffi::uniffi_bindgen_main()` — untestable by nature. Workspace coverage has margin (~99.3% vs the 98% floor), so 1–2 uncovered lines should not breach it. If `ci-local.sh`'s coverage gate does dip below 98%, do NOT write a fake test — exclude the passthrough bin from coverage instrumentation (the established way this repo excludes untestable glue) and note it. If ci-local also invokes the old `--bin uniffi-bindgen` anywhere beyond `bindings_check.py`, that surfaces here as a gate failure — fix the reference, don't suppress it.

**Rollback:** `git revert` — restores the in-crate bin and `cli` feature; the standalone crate is dropped from members.

---

## Self-Review

- **Spec coverage:** G1/G2/G3 → Task 1; G4 → Task 2; G5 → Task 3; G6 → Task 4. All design increments mapped.
- **Placeholder scan:** none — every step has exact snippets/commands. Task 2's `use`-path mechanics and Task 3's exact offending lines are intentionally resolved by the implementer from the tool's own diagnostics (enumerated at run time), not guessed here.
- **Type/name consistency:** the new binary keeps the name `uniffi-bindgen` (so `--bin uniffi-bindgen` semantics are familiar) but lives in package `uniffi-bindgen-tool`; `bindings_check.py` targets it via `-p uniffi-bindgen-tool`.

## Audit log

### Pass 1 — 🚧 Incomplete → gaps fixed
Gaps found by verifying the plan against the actual code/toolchain:
- **Task 1 verified the wrong artifact.** Draft built a host `target/release/libfeatherkey_core.so` and used `nm -D`/`file "stripped"` — but this `cdylib` on macOS is a Mach-O `.dylib`, and those checks are ELF-only. The shipped artifact is the `cargo ndk` **Android ELF**.
  - Changed: Task 1 Steps 1–4 now build the arm64 ELF via `cargo ndk … --release` into `/tmp`, verify `file` = "stripped", size vs the 2,674,400-byte baseline, and `UNIFFI_META_*` via the NDK's `llvm-nm -D`. Added a verification note pinning the baseline and NDK tool paths.
- **Task 4 risked being far bigger than stated.** Whether the standalone tool works depends on whether bindgen runs in source-mode or library-mode. Verified `bindings_check.py:regenerate_to()` already builds the cdylib (L117) and generates with **`--library <lib>`** (L127).
  - Changed: Task 4 Step 5 reduced to swapping only the runner (`--features uniffi --bin uniffi-bindgen` → `-p uniffi-bindgen-tool`); `CRATE_DIR` and the L117 debug build stay. Confirmed the tool has no Rust path-dependency on featherkey-core (its input is the built `.so`). Also noted bindings build **debug**, so the release profile provably cannot alter them.
- **Added a coverage watch-item** for Task 4's untestable passthrough `main.rs` (exclude from instrumentation if the 98% floor dips; never a fake test).

Verified this pass: toolchain present (cargo-ndk 4.1.2, 3 android targets, NDK 28.2 llvm-nm/strip/objdump); `featherkey-core` has BOTH the workspace lint table AND its own `[lints.clippy]` (Task 3 flips both); `ffi.rs` test modules span 362–500 (two modules); `build-jni.sh` builds `--release --features uniffi` (so the profile applies) and its out var is `out_dir`.

### Verdict: ✅ Complete and verified
Every design increment (G1–G6) maps to a task; every step has exact, executable
commands checked against the real toolchain and code; the two load-bearing risks
(Task 1 artifact, Task 4 mode) are resolved, not assumed. Ready to execute via
subagent-driven-development.
