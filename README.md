# FeatherKey

A fast, private, modular, secure Android keyboard.

This repository currently holds the **product/engineering docs** and the
**walking skeleton**: the project structure, the architectural guardrails that
enforce the design, and one thin end-to-end tracer bullet. Feature work builds
on top of these.

## Documents (planning — source of truth first)

| Doc | Role |
|---|---|
| [`BUSINESS_REQUIREMENTS.md`](./BUSINESS_REQUIREMENTS.md) | **Source of truth.** What & why: BR-1…67, objectives, problems. |
| [`SOFTWARE_ENGINEERING.md`](./SOFTWARE_ENGINEERING.md) | How: stack, module decomposition, ADRs, traceability. |
| [`ARCHITECTURE.md`](./ARCHITECTURE.md) | Modular/SOLID/TDD/BDD rules; Ports & Adapters; fitness functions. |

## Repository layout

This is a **monorepo**. Deployable apps live under `apps/`; the shared Rust engine
they build on lives under `core/`.

```
apps/
  android/         Kotlin/Android keyboard app (see apps/android/README.md)
  web/             website app — placeholder, design pending
core/              Rust engine — Cargo workspace (host-testable, no Android types)
  crates/            ~22 crates (kernel, layout-engine, input-decoder, …)
  tools/fitness/     executable architectural rules (no-god-files, DAG, purity)
  features/          BDD (Gherkin) specs, tagged to BR IDs
  Cargo.toml, deny.toml, rust-toolchain.toml
docs/              design specs & plans
.github/workflows/ CI: fmt, clippy, test, fitness, 98% coverage gate
```

The Android app consumes the Rust core as a native library via UniFFI/JNI; the
`.so` is built from `core/` by `apps/android/ffi-bridge/build-jni.sh`. The full
module map is in `SOFTWARE_ENGINEERING.md` §5.

## Develop

```bash
cd core
cargo test --workspace          # run the core test suite
python3 tools/fitness/check.py  # enforce the architectural rules locally
```

CI runs the same (from `core/`), plus `cargo fmt --check`, `cargo clippy -D warnings`,
and a line+branch coverage gate at **98%** (ARCH §7.3). The Android app has its own
CI job (`apps/android`) — see [`apps/android/README.md`](./apps/android/README.md).

## The tracer bullet

The thinnest proven slice: a touch coordinate → `layout-engine` geometry →
`input-decoder` → the character an editor would receive. Rust side is real and
green (`core/crates/input-decoder/tests/tracer_bullet.rs`, BDD spec
`core/features/keystroke_decoding.feature`, tagged `@BR-5`/`@BR-6`). The Kotlin
side of the same path lives in `apps/android/`.
