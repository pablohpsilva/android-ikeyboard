# Multilingual Device Dictionary + Language Momentum — Design

**Date:** 2026-07-25
**Status:** Approved for planning (revised after completeness audit)
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

1. A user with `en, es, ru` active gets device corrections only for `en`.
   `ru`/`es` words the device could validate/correct are invisible.
2. Nothing models **which language the user is writing in right now**. Suggestions
   and autocorrect treat all active languages as a flat pool. A user typing mostly
   English who drops in a Spanish word gets no bias toward the language they are
   actually using, and a deliberate foreign word can be autocorrected away.

## Goals

- Query **all** active languages against the device dictionary concurrently and
  merge their results ("N locales → N sessions → merge").
- Bias **every** word-producing path — typed-prefix suggestions, noisy-channel tap
  decode, **swipe decode**, next-word prediction, and **autocorrect** — toward the
  language the user is currently writing in (**language momentum**), while never
  trapping the user: a deliberate foreign word survives and its alternative stays
  one tap away.
- Learn from the user's behaviour/typing style (recency-weighted), on-device only.

## Non-Goals

- No auto-switching of the visible layout / primary language (silent bias only —
  Option A). No on-screen "now in Spanish" cue this iteration.
- No new bundled word lists (ru/el remain device-sourced).
- No network. Ever.

## Standing Constraints (must hold)

- **No network at runtime.** Device dictionary reads the system spell-checker's own
  offline dictionaries; nothing is fetched.
- **Sensitive-field gating (E-2 / BR-26).** In a password/secure field: no device
  query is issued and momentum is frozen (no learning).
- **Consent gating (BR-22).** Learning — including momentum updates — is off until
  opted in.
- **SOLID, modular, TDD/BDD-first, high coverage.** Pure decision logic lives in the
  Rust core under the existing gate (rustfmt, clippy `-D warnings`, unit tests,
  fitness, BDD, coverage ≥ 98%, cargo-deny). **Any Kotlin that contains branching
  logic is extracted into a pure function and unit-tested** (a minimal JUnit harness
  is added to the Android modules that need it — see §Testing). Framework-only glue
  that cannot be tested off-device is kept as small as possible.

## Central Design Decision: one Ranker, every path

The single most important structural choice (and the fix for three audit gaps):
**every path that puts words on the strip, and the path that chooses a correction,
funnels through the same pure `Ranker`.** Nothing writes suggestions directly.

Paths that today bypass ranking and will be routed through it:
- `updateSuggestions` typed-prefix / tap-decode branches (already scored, re-ranked
  with momentum).
- `updateSuggestions` next-word branch (`bigrams.nextWords`) — tagged + momentum-biased.
- `handleGesture` **swipe** — currently writes `keyboard.suggestions = words.take(3)`
  directly (`ime-service:184`); will build candidates and rank them.
- `correctedWord` — the correction candidate is chosen by the Ranker, not picked ad-hoc.

This gives one place where momentum applies, one scoring contract, one set of tests.

## Architecture

```
 Android (thin shell)                       Rust core (verified, tested)
 ─────────────────────                      ────────────────────────────
 Vocabulary ──┐ per-lang scored candidates
 DeviceDict ──┤ per-lang scored candidates  Ranker (pure)
 Gesture    ──┤ per-lang scored candidates    • normalize source ranks → scores
 Bigrams    ──┘ per-lang scored candidates    • score = posScore
                                              •         + LM_LANG·ln(weight[lang])
                                              •         + source prior
 IME service ── candidates ────────────────▶  • dedupe by word, top-K
 (orchestrates,                             Momentum (pure, stateful)
  gates consent/sensitivity) ── observe ──▶   • decay-all + bump recognizers
                                              • floor + primary head-start
                            ◀── SessionPlan ─ session-planning (pure set math)
```

### Component 0 — Candidate model & score normalization (Rust core)

The audit's gap: bundled candidates carry a *frequency rank*, device candidates only
a *bucket position* — incompatible scales. Resolution: **sources never emit blended
raw scores.** Each source emits candidates as:

```
Candidate { word: String, lang: String, source: Source, source_rank: u32 }
```

where `source_rank` is 0-based position **within that source and language** (0 =
best). The Ranker converts rank → a shared, monotone `positional_score`
(e.g. `-ln(1 + source_rank)`), so bundled and device become commensurable regardless
of their internal maths. A small **per-source prior** (`SOURCE_PRIOR[source]`, tunable;
bundled ≥ device by default) prevents either source from flooding the strip. This is
the only place scores are combined, and it is pure and unit-tested.

### Component 1 — Multi-session `DeviceDictionary` (Kotlin, `platform-services`)

Responsibility: gather per-language spell-checker results. **No ranking logic.**

- State becomes `sessions: LinkedHashMap<String /*lang*/, SpellCheckerSession>`,
  keyed by `Locale.language`, primary first.
- `setLanguages(tags)`: replaces `setPrimary`. The **diff is a pure function**
  extracted for testing:
  `SessionPlan.of(openNow: Set<String>, desiredTags: List<String>) -> SessionPlan(open: List<String>, close: List<String>, order: List<String>)`
  — pure Locale/set math, no Android types, JUnit-tested (see §Testing). `setLanguages`
  just *executes* the plan (open/close sessions), so the Android-bound method holds no
  branching logic.
- Each session is created with **its own listener instance** carrying the language
  code (the framework callback does not identify the firing session); results land in
  per-language buckets.
- `refresh(word)`: fan the query out to **all** sessions concurrently.
- Query API:
  - `candidatesByLanguage(): Map<String, List<String>>` — ranked per language (the
    IME turns each into `Candidate`s with `source_rank` = position).
  - `knownLanguages(word): Set<String>` — languages that returned
    `RESULT_ATTR_IN_THE_DICTIONARY`.
- Callbacks on the main thread; buckets `@Volatile`; each callback re-runs
  `updateSuggestions`.
- Edge handling: a session that fails to open or has no data (ru/el on emulators)
  yields an empty bucket — never a crash, never blocks the bundled path.
- **Privacy unchanged:** the whole component is skipped by the caller in sensitive
  fields; no word reaches any spell-checker process there.

### Component 2 — `Momentum` (Rust core, pure + stateful)

Tracks how strongly the user is currently writing each active language.

- `weight: Map<lang, f64>` for the active set, behind the core's existing
  interior-mutability guard (a `Mutex`, like the other per-user models): `observe`
  takes the write lock, `rank` reads a snapshot — so async device callbacks
  re-running `rank` never race the writer.
- `observe(recognizers: &[Lang])` per committed word: **decay all** (`× DECAY`≈0.9),
  then **bump** each recognizing language (`+= 1.0`). Multi-language cognates bump all
  — neutral.
- `weight_of(lang)`: relative weight with a **floor** so no language is silenced.
- Cold start: **primary head-start** → first word behaves like today.
- `set_languages(tags)`: retain still-active weights, drop removed, add new at floor,
  re-apply head-start.
- Gating is the caller's job: the IME does not call `observe` when consent is off or
  the field is sensitive.

Persistence: momentum is recency state, in-memory in v1. A light persisted
per-language prior MAY seed weights at startup on the existing `schedulePersist`
cadence (same gating); included only if it fits cleanly, else a fast follow-up.

### Component 3 — `Ranker` (Rust core, pure)

Merges candidates from **all** sources into the final ranked list, for both the strip
and the correction:

- Input: `Vec<Candidate>` + a momentum snapshot.
- `score = positional_score(source_rank) + LM_WEIGHT_LANG·ln(weight_of(lang)) + SOURCE_PRIOR[source]`.
- Dedupe by word (keep best), return top-K.
- Pure function → unit- and property-testable (e.g. raising a language's momentum
  never demotes that language's candidates; a decisive `source_rank` still beats weak
  momentum).

### Correction flow (closing the "deliberate foreign word" gap)

`correctedWord(word)` becomes:

1. **Never rewrite a word recognized by *any* active language** — `vocab.rankOf(word) ≠ ∞`
   **or** `word ∈ deviceDict.knownLanguages(word)`. Returns `null`. This is what lets a
   deliberate Spanish word among English survive.
2. Otherwise assemble correction candidates, each tagged `{lang, source, source_rank}`:
   the core `correct` alternatives, `probableWords` (when per-tap data exists), and
   device suggestions per language.
3. Run them through the **same Ranker**. If the top ≠ typed word and clears a
   confidence margin, commit it; else `null`. Momentum thus decides *which* correction
   wins, and the runner-up stays in the strip for a one-tap fix.

(Existing consent + sensitivity guards and the "don't mangle Caps/ALLCAPS" rule stay.)

### FFI surface (additive, no breaking change)

Follows the existing `#[derive(uniffi::Record)]` + bridge-method pattern
(`crates/featherkey-core/src/ffi.rs`):

- New record `FfiCandidate { word, lang, source, source_rank }` (in) and a ranked
  result record (out; reuse `FfiSuggestion` where `lang` need not round-trip, else a
  thin new record).
- New bridge methods: `rank(candidates) -> Vec<...>`, `observe_language(recognizers)`,
  `session_plan(desired_tags) -> {open, close}` (optional — the Kotlin `SessionPlan`
  may stay in Kotlin-with-JUnit instead; decided in the plan, whichever keeps the
  untested surface smaller), and momentum `set_languages`.
- Momentum **state lives in the core**; Kotlin never marshals weights.
- **No change** to `correct`, `suggest`, `decode`, `set_active_languages`, or layout
  method signatures.

### IME orchestration (Kotlin, `ime-service`)

- `Vocabulary` gains `candidatesByLanguage(prefix|taps, learned, context, k)` returning
  **per-language, `source_rank`-ordered** candidates (today it returns a merged
  untagged `List<String>`; this is the explicit API change the audit flagged). Existing
  `suggestions`/`probableWords` are refactored to feed it, preserving their scoring.
- `applyLanguages(tags)` → `deviceDict.setLanguages(tags)` + core momentum
  `set_languages`.
- `updateSuggestions`: gather candidates from Vocabulary (per-language) + device
  buckets + (empty-prefix) bigram next-words, call core `rank`, show top 3. Async
  device callbacks re-run it.
- `handleGesture`: build candidates from the decoder's words (tagged by recognizing
  languages, `source_rank` = position), `rank`, show top 3 — no more direct write.
- `correctedWord`: as the Correction flow above.
- Word boundary / commit / swipe-commit: after the existing consent + sensitivity
  gate, call core `observe_language(recognizers)` where `recognizers` = bundled
  `rankOf ≠ ∞` languages ∪ `deviceDict.knownLanguages(word)`.

## Data Flow

1. **Keystroke** → `updateSuggestions` → gather per-language candidates (bundled +
   device + next-word) → core `rank` → top 3. Device `refresh` fires async; its
   callback re-runs gather→rank→show.
2. **Swipe** → `handleGesture` → decode → tag + `rank` → top 3 → commit best.
3. **Word boundary / suggestion commit / swipe commit** → `correctedWord` (respects
   known-in-any-language, Ranker-chosen correction) → commit → if consent on and not
   sensitive: `observe_language(recognizers)` → `learnWord` → `schedulePersist`.

## Error Handling & Edge Cases

- Per-language session open fails / no data → empty bucket; bundled path unaffected.
- Sensitive field → device queries skipped, momentum frozen.
- Consent off → momentum frozen (bias falls back to primary head-start / positional).
- Cold start / single language → primary head-start ⇒ behaviour identical to today.
- Empty candidate set → empty strip.
- Momentum read/write race → guarded by the core mutex; `rank` uses a snapshot.

## Testing (TDD/BDD-first, ≥ 98% coverage on the gate)

**Rust (under the existing gate):**
- Unit — `Momentum` (decay lowers all; bump raises exactly the recognizers; floor;
  primary head-start; `set_languages` add/drop/retain); `Ranker` (momentum never
  demotes the boosted language; decisive `source_rank` beats weak momentum; dedupe
  keeps best; top-K); score normalization (bundled vs device commensurable; source
  prior bounds flooding).
- Property tests — momentum-monotonicity of rank.
- **BDD `.feature`:**
  - "Mostly-English with one deliberate Spanish word → not autocorrected; its
    suggestion is offered."
  - "Sustained typing in language A → A's completions rank first."
  - "Switch A→B for several words → bias follows to B."
  - "Swipe in a momentum-B context → B word ranked first."
- Fitness / clippy / rustfmt / cargo-deny unchanged; real coverage measured with
  `--ignore-filename-regex '(^|/)workspace/'`.

**Kotlin (new minimal JUnit harness where logic exists):**
- Add JUnit/`kotlin-test` to `platform-services` (and `ime-service` if a pure helper
  lands there). Test:
  - `SessionPlan.of(...)` — add/drop/retain/reorder, dedupe by `Locale.language`,
    empty and single-language inputs.
  - the Vocabulary→`Candidate` tagging helper if it is pure Kotlin.
- `DeviceDictionary` session wiring and per-language listener routing remain
  framework-bound; verified on-device (English returns corrections; a multilingual
  sentence biases correctly). Kept logic-free so this untested surface is minimal.

## SOLID / Modularity Notes

- **SRP:** `DeviceDictionary` gathers; `Momentum` tracks; `Ranker` ranks; `SessionPlan`
  plans; the IME orchestrates + gates. One Ranker = one place momentum is applied.
- **OCP:** adding a new candidate source = emit `Candidate`s; Ranker unchanged.
- **DIP/ISP:** Kotlin depends on small FFI records/methods, not the algorithm; momentum
  weights never cross the boundary.
- Each unit is understandable/testable in isolation; `DeviceDictionary` stays small
  precisely because ranking and session-planning logic live elsewhere (core / pure fn).

## Open Questions / Risks

- Constant tuning (`DECAY`, `FLOOR`, head-start, `LM_WEIGHT_LANG`, `SOURCE_PRIOR`,
  correction margin) — sensible defaults, refined against the BDD scenarios; all pure
  and cheap to tune.
- `SessionPlan` home — pure Kotlin+JUnit vs Rust FFI; decided in the plan by whichever
  minimizes untested surface.
- Persisted long-run language prior — include only if clean; else v1 in-memory,
  follow-up.
- Per-keystroke FFI marshaling of small candidate lists — expected negligible; confirm
  no device jank.
