# featherkey-core

**Its ONE job:** Be the composition root — wire the domain crates behind the `contracts` ports and present one narrow, UniFFI-ready use-case API to the shell.

## Layer

`composition` (`[package.metadata.featherkey] layer = "composition"`). The outermost layer (rank 4): the only crate allowed to name concrete adapters. It depends on the domain crates, the `secure-store` **adapter**, and the ports in `contracts`; nothing depends on it (ARCH §9.3, fitness E-1).

## Ports

Consumes, never defines. It composes the driven ports (`SecureStore` via the `secure-store` adapter; `SensitiveContextSource` supplied per call by the shell) and the driving ports (`Predictor`, `AutoCorrect`) into one `FeatherKeyCore` handle. The six §9.1 driving-port use-cases are surfaced as methods:

| Use-case (ARCH §9.1) | Method(s) |
|---|---|
| `DecodeKeystroke` | `decode(x, y) -> DecodeResult` |
| `Suggest` | `suggest(preceding, prefix) -> Suggestions` |
| `Correct` | `correct(text, preceding, prefix) -> Correction` |
| `SwitchLanguage` / `ActiveLanguages` | `set_active_languages(..)`, `active_languages()` |
| `LearnFromInput` | `learn_word(..)`, `observe_tap(..)` |
| `ManageUserDictionary` | `add_to_dictionary(..)`, `knows_word(..)`, `word_frequency(..)` |

Plus `persist`/`restore` through the `SecureStore` port and `set_layout` for page switching.

Dependencies: `kernel`, `contracts`, and every MVP domain crate (`layout-engine`, `input-decoder`, `touch-model`, `dictionary`, `locale-manager`, `prediction`, `autocorrect`, `personalization`, `sensitive-context`) plus the `secure-store` adapter. Dev-deps: `proptest`, `tempfile`.

## Invariants

- **Single source of truth.** The façade owns the authoritative learned state (`Personalization`, `TouchModel`) and the active `(LangId, Dictionary)` packs. The derived read engines (`StatisticalPredictor`, `NoClobberCorrector`, `LocaleManager`) are rebuilt on demand from it, so there is no cache to fall stale. (A materialized read-model to avoid per-call rebuilds is a v1.x optimization; decoding — the hot path — rebuilds nothing.)
- **E-2 — sensitive-context ordering (BR-26).** `learn_word` and `observe_tap` consult `SensitivityPolicy` *before* any `observe`, so a keystroke typed into a password/OTP field is dropped before it can reach `Personalization` or `TouchModel`. Explicit `add_to_dictionary` edits are deliberate user actions and are intentionally not gated.
- **Atomic language switch.** `set_active_languages` fully validates the new set before committing; a rejected switch leaves the current set intact.
- **One flat error.** Every internal crate error folds into `FeatherKeyError` (with a `Display` message), so the FFI surface exposes one stable error shape. Errors are values; no panics cross the boundary.
- **Host-testable.** Names no Android/JNI types; runs fully offline.

## UniFFI surface

The public methods are authored **UniFFI-ready** — owned plain types (`String`, `f32`, `bool`, flat structs/enums) cross the boundary and every fallible call returns `FeatherKeyError`. The actual `#[uniffi::export]` scaffolding and Kotlin-binding generation are applied in **Wave 5 (ADR-18)**: the workspace forbids `unsafe` (which UniFFI's generated scaffolding requires) and binding generation needs the Android NDK. Keeping the surface FFI-shaped now means Wave 5 annotates rather than redesigns.

## Serves (BRs)

Closes no new product BR directly — it composes the crates that own each BR and enforces the BR-26 gate ordering (E-2) at the one place the system is wired together. Exercised across BR-5, BR-7, BR-8, BR-10, BR-12, BR-16, BR-26.

## Tests

Inline coverage is exercised through two cross-boundary suites: `tests/composition.rs` (every use-case, the real `secure-store` adapter round-trip via `tempfile`, layout switching, and the full construction/error surface) and `tests/e2_sensitive_ordering.rs` (the E-2 property — a `proptest` that no input under a sensitive field is ever learned, a deterministic tap-geometry gating scenario, and a per-call gate check). Bound to the `features/featherkey-core.feature` scenarios.
