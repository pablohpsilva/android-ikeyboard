# featherkey-locale-manager

**Its ONE job:** Track the ordered set of active languages and identify, per word, which active language it belongs to (lightweight statistical language-ID).

## Layer

`domain` (per `[package.metadata.featherkey] layer = "domain"`). Pure in-memory logic; no I/O.

## Ports & dependencies

Implements/offers no `contracts` port traits. It does not depend on `kernel` or `contracts`. Its only dependency is the sibling domain crate `featherkey-dictionary` (path dep), whose `Dictionary::contains` / `Dictionary::prefix` supply the detection signal (ADR-13: this crate reads `dictionary`).

Public API: `LocaleManager` (`new`, `set_active`, `active`, `detect`), the value object `LangId`, and the error `LocaleError` (`NoActiveLanguages`, `DuplicateLanguage`).

## Invariants

- **Non-empty active set** — construction and switching reject an empty set (`NoActiveLanguages`).
- **Ordered set, no duplicates** — a `LangId` appears at most once; repeats are rejected (`DuplicateLanguage`), never silently collapsed.
- **Atomic reconfigure (no half-apply)** — `set_active` validates the whole new set before storing anything; a rejected switch leaves the current set fully intact.
- **Parallel-vector correspondence** — `dicts[i]` is always the lexicon of `ids[i]`.
- **Detection never panics / never errors** — `detect` returns plain data; empty word → `None`, unrecognised word → `None`.
- **Containment dominates prefix breadth** — an exact lexicon hit (`CONTAINS_WEIGHT = 100`) always outscores prefix breadth (bounded by `MAX_COMPLETIONS = 16`), so a completed word pins its language.
- **First-wins hysteresis** — ties resolve to the earliest (most-recently-chosen) active language (index 0), so mixed input does not thrash between languages.
- **`LangId` equality is byte-equal tags** — the caller owns all normalisation (case, region); this type is a stable key, not a locale database.

## Serves (BRs)

BR-16, BR-17, BR-18, BR-19, BR-19a, BR-19b. Directly exercised by the code today: BR-16 (concurrent active languages, in order), BR-17 (instant switch, no reload), BR-18 (mixed-input hysteresis), BR-19b (per-word auto-detection). The broader BR-19 / BR-19a detection behaviour is served through the same `detect` scoring.

## Notes / deferred

The active-set count is ≥2 at MVP and architected toward 3 (SEDD §6.1); the domain type also permits a single active language. Detection is the lightweight statistical scheme of ADR-10 — no ML model, no trigram tables. `LangId` carries no plurals, collation, or region policy (deferred to layers above this crate).

## Tests

Inline `#[cfg(test)]` unit tests in `src/lib.rs` cover `LangId`, construction invariants, the full detection scoring (containment vs. prefix breadth, empty/unknown words), instant switching, and error `Display`. Integration tests in `tests/detection.rs` exercise the public API across the crate boundary, including 3-way concurrent active languages. No proptests.
