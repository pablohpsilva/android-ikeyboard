# Multilingual Device Dictionary + Language Momentum — Design

**Date:** 2026-07-25
**Status:** Approved for planning
**Author:** FeatherKey (pair: Oakblu)

## Problem

FeatherKey supports multiple active languages, but its two vocabulary sources
serve them unequally:

- **Bundled word lists (`Vocabulary`, Kotlin):** already multilingual — every
  active language's list is searched and results are round-robined.
- **Device dictionary (`DeviceDictionary`, Kotlin over Android TextServices):**
  covers **only the primary** active language. It holds a single
  `SpellCheckerSession` bound to the first active tag. So device-sourced
  corrections and "is this a real word?" checks ignore the user's other active
  languages entirely.

Two consequences:

1. A user with, say, `en, es, ru` active gets device corrections only for `en`.
   `ru`/`es` words the device could validate/correct are invisible.
2. Nothing models **which language the user is writing in right now**. Suggestions
   and autocorrect treat all active languages as a flat pool. A user typing mostly
   English who drops in a Spanish word gets no bias toward the language they are
   actually using, and — worse — a deliberate foreign word can be autocorrected
   away if only the "wrong" language is considered.

## Goals

- Query **all** active languages against the device dictionary concurrently and
  merge their results ("N locales → N sessions → merge").
- Bias suggestions and autocorrect toward the language the user is currently
  writing in (**language momentum**), while never trapping the user: a deliberate
  foreign word survives and its alternative stays one tap away.
- Learn from the user's behaviour/typing style (recency-weighted), on-device only.

## Non-Goals

- No auto-switching of the visible layout / primary language (silent bias only —
  chosen Option A). No on-screen "now in Spanish" cue in this iteration.
- No new bundled word lists (ru/el remain device-sourced; the device-only trade-off
  stands).
- No network. Ever. All learning and lookup stay on-device.

## Standing Constraints (must hold)

- **No network at runtime.** Device dictionary reads the system spell-checker's own
  offline dictionaries; nothing is fetched.
- **Sensitive-field gating (E-2 / BR-26).** In a password/secure field: no device
  query is issued (the word would reach another process) and momentum is frozen
  (no learning).
- **Consent gating (BR-22).** Learning — including momentum updates — is off until
  the user opts in.
- **SOLID, modular, TDD/BDD-first, high coverage.** The pure decision logic lives in
  the Rust core under the existing gate (rustfmt, clippy `-D warnings`, unit tests,
  fitness, BDD, coverage ≥ 98%, cargo-deny). Kotlin adapters stay thin.

## Architecture

Three components; the pure algorithm lives in Rust (Option R, approved), the
Android-framework adapter stays in Kotlin.

```
 Android (thin shell)                      Rust core (verified, tested)
 ─────────────────────                     ────────────────────────────
 Vocabulary ──┐  per-language candidates
              ├─────────────────────────▶  Ranker (pure)
 DeviceDict ──┘  per-language candidates      • merge + dedupe
   (N sessions,                                • score = lexical + log(weight[lang])
    fan-out, buckets)                          • top-K
                                            Momentum (pure, stateful)
 IME service  ───── observe(word, langs) ─▶    • decay-all + bump recognizers
 (orchestrates,                                • floor + primary head-start
  gates by consent/sensitivity)               • frozen when gating says so
```

### Component 1 — Multi-session `DeviceDictionary` (Kotlin, `platform-services`)

Responsibility: gather per-language spell-checker results. **No ranking logic.**

- State becomes `sessions: LinkedHashMap<String /*lang*/, SpellCheckerSession>`,
  keyed by `Locale.language`, deduped, primary first.
- `setLanguages(tags: List<String>)`: diff against the open set — open sessions for
  added languages, close removed ones. Replaces `setPrimary`.
- Each session is created with **its own listener instance** that carries the
  language code, because `SpellCheckerSessionListener` callbacks do not identify the
  firing session. Results land in per-language buckets.
- `refresh(word)`: fan the query out to **all** sessions concurrently.
- Query API (consumed by the IME to build candidates):
  - `suggestionsByLanguage(): Map<String, List<String>>` — ranked per language.
  - `knownLanguages(word): Set<String>` — which languages confirmed it real
    (`RESULT_ATTR_IN_THE_DICTIONARY`).
- Callbacks arrive on the main thread; buckets are `@Volatile`. Each callback
  re-runs `updateSuggestions()` (unchanged pattern).
- Failure/edge handling: a session that fails to open or has no data for its locale
  (ru/el on emulators) simply yields an empty bucket — never a crash, never blocks
  the bundled path.
- **Privacy unchanged:** the whole component is skipped in sensitive fields by the
  caller; no word reaches any spell-checker process there.

This component is a thin Android adapter (framework-bound, not unit-testable without
a device). It is kept deliberately logic-free so the untested surface is minimal.

### Component 2 — `Momentum` (Rust core, pure + stateful)

Responsibility: track how strongly the user is currently writing each active
language.

- Holds `weight: Map<lang, f64>` for the active set.
- `observe(recognizers: &[Lang])` per committed word: **decay all weights**
  (`× DECAY`, e.g. 0.9), then **bump** each language that recognized the word
  (`+= 1.0`). Cognates recognized by several languages bump all — neutral.
- `weight_of(lang)`: relative weight with a **floor** (`FLOOR`) so no language is
  ever fully silenced — a decisive exact match from a dormant language still
  surfaces.
- Cold start: the **primary language gets a head-start** so the first word behaves
  exactly like today.
- `set_languages(tags)`: keep weights for languages still active, drop removed, add
  new at floor, re-apply primary head-start.
- Deterministic and side-effect-free apart from its own state → fully unit-testable.
- Gating is the caller's job: the IME simply does not call `observe` when consent is
  off or the field is sensitive (so no persisted or in-memory learning leaks there).

Persistence (minimal): momentum is **recency state**, primarily in-memory. A light
per-language long-run prior MAY seed weights at startup, persisted on the existing
`schedulePersist` cadence and gated identically. Kept minimal to avoid scope creep;
if it complicates the plan, v1 ships in-memory only and the prior is a follow-up.

### Component 3 — `Ranker` (Rust core, pure)

Responsibility: merge candidates from both sources into the final ranked strip /
correction, applying momentum.

- Input: a list of candidates, each `{ word, lang, lexical_score, source }`, plus a
  reference to current momentum weights.
- `score = lexical_score + LM_WEIGHT_LANG * ln(weight_of(lang))`. Lexical strength
  (frequency / exact-match / learned usage) still dominates when decisive; momentum
  breaks ties and reorders near-ties toward the current language.
- Dedupe by word (keep best score), return top-K.
- Pure function of its inputs → fully unit-testable, property-testable (e.g. adding
  momentum to a language never demotes that language's candidates).

### FFI surface (additive, no breaking change)

Follows the existing `#[derive(uniffi::Record)]` + bridge-method pattern
(`crates/featherkey-core/src/ffi.rs`):

- New record `FfiCandidate*` for `{ word, lang, lexical_score, source }` in.
- New record for a ranked result out (reuse `FfiSuggestion { word, score }` where it
  fits, or a thin new record if `lang` must round-trip).
- New bridge methods, e.g. `rank(candidates, ...) -> Vec<...>` and
  `observe_language(recognizers)` / momentum accessors. Momentum **state lives in the
  core** (alongside the existing per-user models), so Kotlin never marshals weights.
- No change to existing method signatures (`correct`, `suggest`, `decode`,
  `set_active_languages`, layout calls).

### IME orchestration (Kotlin, `ime-service`)

- `applyLanguages(tags)` calls `deviceDict.setLanguages(tags)` (was `setPrimary`) and
  the core's `set_languages` for momentum.
- `updateSuggestions()`: build candidates from `Vocabulary` (per-language) +
  `DeviceDictionary.suggestionsByLanguage()`, call the core `rank(...)`, show top 3.
  Async device callbacks re-run it (unchanged).
- `correctedWord(word)`: unchanged privacy/consent guards; additionally **never
  rewrite a word in `knownLanguages(word)`** (any active language) — so a deliberate
  foreign word survives; its alternative remains in the strip for a one-tap fix.
- At a word boundary (`boundary`/`commitSuggestion`): after the existing consent +
  sensitivity gate, call the core `observe_language(recognizers)` where `recognizers`
  = bundled `rankOf ≠ ∞` languages ∪ `deviceDict.knownLanguages(word)`.

## Data Flow

1. **Keystroke** → `updateSuggestions` → gather per-language candidates (bundled +
   device buckets) → core `rank` → show top 3. Device `refresh(word)` fires async;
   its callback re-runs the gather → rank → show.
2. **Word boundary** → `correctedWord` (respects known-in-any-language) → commit → if
   consent on and field not sensitive: core `observe_language(recognizers)` →
   `learnWord` → `schedulePersist`.

## Error Handling & Edge Cases

- Per-language session open fails / no data → empty bucket; bundled path unaffected.
- Sensitive field → device queries skipped, momentum frozen.
- Consent off → momentum frozen (bias falls back to primary head-start / lexical
  only).
- Cold start / single language → primary head-start makes behaviour identical to
  today.
- Empty candidate set → empty strip (as today).

## Testing (TDD/BDD-first, ≥ 98% coverage on the gate)

Pure logic is in Rust specifically so it lands under the existing gate.

- **Unit (Rust):**
  - `Momentum`: decay reduces all weights; a recognizer bump raises exactly the
    recognizing languages; floor is respected; primary head-start; `set_languages`
    add/drop/retain.
  - `Ranker`: momentum never demotes the boosted language; decisive lexical score
    beats weak momentum; dedupe keeps the best; top-K bound.
- **BDD (Rust, `.feature`):**
  - "Mostly-English with one deliberate Spanish word → the Spanish word is not
    autocorrected and its suggestion is offered."
  - "Sustained typing in language A → A's completions rank first."
  - "Switch from A to B for several words → bias follows to B."
- **Property tests** where natural (monotonicity of momentum on rank).
- **Fitness / clippy / rustfmt / cargo-deny** unchanged; real coverage measured with
  `--ignore-filename-regex '(^|/)workspace/'`.
- **Kotlin:** `DeviceDictionary` is a thin framework adapter kept logic-light;
  verified on-device manually (English returns corrections; a multilingual sentence
  biases correctly). No new Kotlin test harness is required because the decision
  logic is in Rust.

## SOLID / Modularity Notes

- **SRP:** `DeviceDictionary` only gathers; `Momentum` only tracks language weight;
  `Ranker` only ranks; the IME only orchestrates + gates.
- **OCP/DIP:** the core exposes `rank` / `observe` behind the existing FFI trait
  boundary; Kotlin depends on that interface, not on the algorithm. Momentum weights
  never cross the boundary.
- **ISP:** small, purpose-specific FFI records/methods rather than one fat call.
- Each unit is understandable and testable in isolation; `DeviceDictionary` stays
  small precisely because it holds no ranking logic.

## Open Questions / Risks

- Constant tuning (`DECAY`, `FLOOR`, head-start, `LM_WEIGHT_LANG`) — start with
  sensible defaults, refine against BDD scenarios; they are pure and cheap to tune.
- Persisted long-run language prior: include only if it fits cleanly; otherwise v1 is
  in-memory momentum and the prior is a fast follow-up.
- Per-keystroke FFI marshaling of small candidate lists — expected negligible;
  confirm no jank on device.
