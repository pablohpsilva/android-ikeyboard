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

```
crates/            Rust core (host-testable, no Android types)
  kernel/            shared value objects + error types (no deps)
  layout-engine/     key layouts & geometry
  input-decoder/     touch -> intended key (the accuracy engine)
tools/fitness/     executable architectural rules (no-god-files, DAG, purity)
features/          BDD (Gherkin) specs, tagged to BR IDs
android/           Kotlin/Android shell — SCAFFOLD, not yet built (see its README)
.github/workflows/ CI: fmt, clippy, test, fitness, 98% coverage gate
```

The full module map (≈20 crates + shell modules) is in `SOFTWARE_ENGINEERING.md`
§5. Crates are added to the workspace as they are implemented, TDD-first; the
skeleton contains only the three crates the keystroke tracer bullet exercises.

## Develop

```bash
cargo test --workspace          # run the core test suite
python3 tools/fitness/check.py  # enforce the architectural rules locally
```

CI runs the same, plus `cargo fmt --check`, `cargo clippy -D warnings`, and a
line+branch coverage gate at **98%** (ARCH §7.3). The Android shell has its own
job, dormant until its Gradle build is wired up on a machine with the Android
toolchain — see [`android/README.md`](./android/README.md).

## The tracer bullet

The thinnest proven slice: a touch coordinate → `layout-engine` geometry →
`input-decoder` → the character an editor would receive. Rust side is real and
green (`crates/input-decoder/tests/tracer_bullet.rs`, BDD spec
`features/keystroke_decoding.feature`, tagged `@BR-5`/`@BR-6`). The Kotlin side
of the same path is scaffolded in `android/`.
