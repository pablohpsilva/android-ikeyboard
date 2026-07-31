# User-selectable keyboard layout — design

**Date:** 2026-07-31
**Status:** design
**Slug:** selectable-keyboard-layout

## 1. Problem

FeatherKey today derives the on-screen alphabetic layout **entirely from the
primary active language**. The single selector is
`Layout::alpha_for(primary_tag)` (`core/crates/layout-engine/src/scripts.rs:65`),
called from three places in the composition root
(`core/crates/featherkey-core/src/lib.rs:167,199,233`):

| Primary language | Layout forced |
|---|---|
| `fr` | AZERTY |
| `de`, `lb` | QWERTZ |
| `ru/uk/be/bg/sr/mk` | Cyrillic (ЙЦУКЕН) |
| `el` | Greek |
| everything else | QWERTY |

Consequence: **the user cannot choose their letter arrangement.** A French
speaker who prefers QWERTY is stuck on AZERTY; an English speaker who trained on
QWERTZ cannot get it. The layouts `qwertz()` and `azerty()` are fully implemented
and tested in `layout-engine` but are **unreachable except via the primary
language** — there is no "I want AZERTY while typing English."

Requirement (user, verbatim): *"allow the user, disregarding the language (or
languages) the user has selected, pick the best keyboard layout he wants and use
that layout always. By layout I mean: QWERTY, QWERTZ, ... . Make sure the layout
selection is easy to use."*

Additional user directives (this session):
- Ship a first set now (**QWERTY / QWERTZ / AZERTY**) but architect so **all
  combinations** can be added later.
- **Default** must **match the layout the system already has**; if the detected
  layout is unsupported, fall back to **QWERTY**.
- Layout choice replaces key **geometry only**; the selected **language(s) still
  drive accents, prediction, and autocorrect** (confirmed).
- The setting lives in the **FeatherKey settings screen**.

This closes a new business requirement, **BR-68** (added to the BRD in the same
change — see §9).

## 2. Current behaviour (what exists — per CODEMAP §2)

Confirmed by CODEMAP query + source read + a dedicated exploration pass:

1. **Layout is single-sourced in the Rust core.** The Kotlin view never authors
   alpha letters in production — `FeatherKeyImeService.renderKeys()` maps whatever
   `bridge.layoutKeys()` returns (`FeatherKeyImeService.kt:445`), and the
   tap-decoder resolves against that **same** key set (`keyCenters` derived at
   `:448`, inside the `.also` block opened at `:447`). So rendering and decoding
   are always consistent *because* they read
   one source. (Kotlin owns only the number/symbol pages and a `FALLBACK_QWERTY`
   used while the bridge is still opening — `KeyboardView.kt:1230`.)

2. **The only layout selector is the primary language.** `alpha_for` keys on the
   primary subtag; there is no override, and `Direction` (RTL/LTR) is a passive
   marker unrelated to selection.

3. **FFI layout surface takes no layout parameter.** `layout_keys()`,
   `use_alpha_layout()`, `use_numeric_layout()`, `use_symbols_layout()`,
   `set_active_languages()` — none selects an arrangement
   (`core/crates/featherkey-core/src/ffi.rs:394,403,408,413,423`). Rust
   `FeatherKeyCore::set_layout(Layout)` exists (`lib.rs:227`) but is **not**
   `#[uniffi::export]`ed, so it is unreachable from Kotlin.

4. **Settings pattern already exists.** `KeyboardAppearancePrefs`
   (`platform-services/.../KeyboardAppearancePrefs.kt`) is a SharedPreferences
   store using an `enum(tag)` + getter/setter, read by the IME per field. The
   settings screen's `TypingSection` (`SettingsActivity.kt:399`) renders the
   height picker as a row of `HeightOption(...)` calls (`:409`–`:415`), each a
   thin wrapper over `FilterChip` (`HeightOption` at `:444`, `FilterChip` at
   `:445`). `LanguagePrefs`
   (`platform-services/.../LanguagePrefs.kt`) is the precedent for a preference
   that flows **to the core** (via `applyLanguages`), not to the view.

5. **Android exposes no directly-readable "system layout" for a soft keyboard.**
   The real detection API, `InputDevice.getKeyCodeForKeyLocation()`, reports only
   the layout of an **attached physical keyboard** (which most phone users lack).
   ([Android InputDevice docs](https://developer.android.com/reference/android/view/InputDevice#getKeyCodeForKeyLocation(int)))
   For soft keyboards the only signal is the locale — which is exactly what
   `alpha_for` already infers.

6. **Test gaps.** `settings-ui` has **zero** tests. No test asserts that
   `alpha_for`'s output actually reaches `KeyboardView` (the FFI→view coupling is
   verified only on the Rust side and indirectly via a `keysVersion` cache test).

## 3. Decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | The layout override lives **in the Rust core** as `latin_override: Option<LatinLayout>`. | Layout is single-sourced there (§2.1); putting the choice anywhere else risks the render/decode set diverging. |
| D2 | The override is **Latin-only**. Non-Latin scripts (Cyrillic, Greek) **always win** and ignore the override. | Forcing Latin keys onto a Russian keyboard would make Cyrillic untypable — never strand the user. |
| D3 | The FFI/pref enum is **total**: `{ Auto, Qwerty, Qwertz, Azerty }`. `Auto` ⇒ `None` in the core ⇒ per-language locale default (today's behaviour). | One value ("Auto") expresses "match the system" without the core needing Android APIs. |
| D4 | **"Auto" resolution is split**: Kotlin probes an attached **physical** keyboard; the **core** does the **locale** inference. | Keeps the Android-only probe in Kotlin and the locale rule (already in `alpha_for`) unduplicated in Rust. |
| D5 | New dedicated **`KeyboardLayoutPrefs`** SharedPreferences store (not folded into `KeyboardAppearancePrefs`). | Layout flows to the **core** (like `LanguagePrefs`), not to the view like appearance; different destination ⇒ its own store. |
| D6 | Settings UI is a **segmented `FilterChip` row** `Auto · QWERTY · QWERTZ · AZERTY` in the Typing section. | Reuses the exact widget/pattern of the existing height picker — "easy to use", zero new UX vocabulary. |

Rejected alternative (for D1): *Kotlin passes a synthetic language tag so
`alpha_for` resolves it* (e.g. force AZERTY by pretending primary=`fr`). Rejected
— it overloads the language tag with two meanings, corrupts language-dependent
logic (prediction/momentum), and is fragile. D1 keeps layout and language
orthogonal, which is the whole point of the feature.

## 4. Architecture

```
Settings screen (FilterChip row)
   │  writes tag: "auto|qwerty|qwertz|azerty"
   ▼
KeyboardLayoutPrefs  (SharedPreferences, platform-services)
   │  read on onStartInput (read-on-next-field, like LanguagePrefs)
   ▼
FeatherKeyImeService.applyLayout()
   │  resolves setting → FfiLatinLayout:
   │    explicit → that kind
   │    "auto"   → probe physical kbd (Q→A ⇒ Azerty, Y→Z ⇒ Qwertz, else Qwerty)
   │               no physical kbd → Auto  (let core infer from locale)
   ▼  bridge.setLatinLayout(kind)   ── new FFI ──▶ KeyboardCore::set_latin_layout
                                                        │ stores latin_override
                                                        ▼
                              Layout::alpha_for(primary, override)
                                 script? → cyrillic()/greek()  (override ignored, D2)
                                 latin?  → override ⧸ locale-default
                                                        │
   keyboard?.keys = renderKeys() ◀── bridge.layoutKeys() ◀┘  (render + decode)
```

### 4.1 Rust `layout-engine`
- New public enum `LatinLayout { Qwerty, Qwertz, Azerty }` (extensible — Dvorak,
  Colemak, etc. added here later, one variant each).
- New `Script` classifier (private or crate-internal): `fn script_of(tag) ->
  Script` returning `Cyrillic | Greek | Latin`, factored out of `alpha_for`'s
  current match.
- `alpha_for(tag: &str, override: Option<LatinLayout>) -> Layout`:
  - `Cyrillic` → `cyrillic()`; `Greek` → `greek()` (override ignored).
  - `Latin` → `override.map(LatinLayout::build).unwrap_or_else(|| default_latin_for(tag))`
    where `default_latin_for` is today's rule (fr→azerty, de/lb→qwertz, else
    qwerty).
- `LatinLayout::build(self) -> Layout` maps `Qwerty→qwerty()`, `Qwertz→qwertz()`,
  `Azerty→azerty()`.

**Signature change** to `alpha_for` — one added param. All 3 core call sites
updated to pass `self.latin_override`.

### 4.2 Rust `featherkey-core`
- Struct field `latin_override: Option<LatinLayout>` (defaults `None` in `new`).
- `pub fn set_latin_layout(&mut self, layout: Option<LatinLayout>)` — sets the
  field and re-derives the current alpha page **if** the live page is alpha (so
  the change is visible without a language switch). Numeric/symbol page stays.
- `new`, `set_active_languages`, `use_alpha_layout` all pass
  `self.latin_override` into `alpha_for`, so **call order is irrelevant** — a
  later language switch never drops the user's layout choice.

### 4.3 FFI (`ffi.rs` + regenerated bindings)
- New UniFFI enum `FfiLatinLayout { Auto, Qwerty, Qwertz, Azerty }`.
- New method `pub fn set_latin_layout(&self, layout: FfiLatinLayout)`:
  maps `Auto → None`, others → `Some(LatinLayout::…)`, calls the core.
- Regenerate `generated/featherkey_core.kt`; the bindings-freshness gate
  (`tools/bindings_check.py --check`) must stay green. Rebuild the `.so`.

### 4.4 Kotlin `platform-services`
- `KeyboardLayoutPrefs(context)`: enum `KeyboardLayoutChoice(tag)` =
  `AUTO, QWERTY, QWERTZ, AZERTY`; `choice()` / `setChoice()`; default `AUTO`.
  SharedPreferences file `featherkey_layout`.
- `PhysicalKeyboardLayout` — a **pure** classifier `fun classify(probe: (Int) ->
  Int): KeyboardLayoutChoice?` where `probe` wraps
  `InputDevice.getKeyCodeForKeyLocation`. `KEYCODE_Q→A ⇒ AZERTY`,
  `KEYCODE_Y→Z ⇒ QWERTZ`, identity ⇒ QWERTY, unknown ⇒ null. The Android glue
  (finding an attached full-keyboard `InputDevice`) is a thin separate function;
  the classification rule is unit-tested via the injected `probe`.

### 4.5 Kotlin `ime-service`
- `applyLayout()` (mirrors `applyLanguages`): read `KeyboardLayoutPrefs.choice()`;
  if `AUTO`, try the physical probe → a concrete kind, else keep `AUTO`; map to
  `FfiLatinLayout`; `bridge?.setLatinLayout(kind)`; then re-pull
  `keyboard?.keys = renderKeys()` (same refresh `applyLanguages` does at `:298`).
  Called on `onStartInput` alongside the existing appearance/language reads.
- `FeatherKeyBridge.setLatinLayout(choice)` wrapper.

### 4.6 Kotlin `settings-ui`
- In `TypingSection`, a new labelled row "Keyboard layout" = `Row` of
  `FilterChip`s `Auto · QWERTY · QWERTZ · AZERTY`, each writing
  `KeyboardLayoutPrefs.setChoice(...)`. Reuse the existing "applies next time the
  keyboard opens" caption.

## 5. Data flow & edge cases

- **Change takes effect** on the next `onStartInput` (read-on-next-field), matching
  every other pref; the caption says so.
- **Language switch after choosing a layout**: choice persists — the core holds
  `latin_override` across `set_active_languages` (§4.2).
- **Primary = Cyrillic/Greek**: chosen layout stored but not shown; the native
  script block renders (D2). If the user later switches primary back to a Latin
  language, their Latin choice reappears automatically.
- **Auto, no physical keyboard** (typical phone): core infers from locale — de →
  QWERTZ, fr → AZERTY, else QWERTY (today's behaviour, now the documented Auto
  default).
- **Auto, physical keyboard attached but unsupported layout** (e.g. Dvorak): probe
  returns null → treat as `Auto` → locale default (QWERTY for most) — satisfies
  "unsupported → QWERTY".

## 6. Components / boundaries

| Unit | One job | Depends on |
|---|---|---|
| `LatinLayout` enum + `LatinLayout::build` | Name the Latin arrangements; build their `Layout` | `layout-engine` internals |
| `script_of` / `default_latin_for` | Classify a tag's script; today's per-locale Latin default | tag string only |
| `alpha_for(tag, override)` | Resolve the alpha page from language **and** override | above |
| `KeyboardCore::set_latin_layout` + FFI | Carry the choice across the boundary into core state | `layout-engine` |
| `KeyboardLayoutPrefs` | Persist the user's choice | Android SharedPreferences |
| `PhysicalKeyboardLayout.classify` | Fingerprint an attached physical layout (pure) | injected `probe` |
| `applyLayout()` | Resolve choice→kind per field and push to core | prefs + bridge |
| settings layout row | Let the user pick, easily | prefs |

Each is independently testable; the only Android-touching, hard-to-unit-test part
(finding the physical `InputDevice`) is isolated to a few lines behind the pure
`classify`.

## 7. Testing strategy

**BDD first** (`core/features/layout-engine.feature`, tagged `@BR-68`):
- Choosing QWERTZ while typing English yields the QWERTZ block.
- Choosing AZERTY does not change accents/prediction (language still drives them).
- A Cyrillic primary ignores the Latin choice and shows ЙЦУКЕН.
- Auto with no override reproduces the per-language default.

**Rust unit** (`layout-engine`):
- `alpha_for("en", Some(Azerty))` → AZERTY; `("de", None)` → QWERTZ (default);
  `("en", None)` → QWERTY.
- `alpha_for("ru", Some(Qwerty))` → Cyrillic (override ignored);
  `("el", Some(Azerty))` → Greek.
- `LatinLayout::build` maps each variant to the right block; enum is exhaustive.

**Rust core** (`featherkey-core/tests`):
- `set_latin_layout(Some(Azerty))` changes `layout_keys()` first row to `azerty…`.
- Choice survives a `set_active_languages` switch between two Latin languages.
- `use_alpha_layout()` after a numeric page returns to the **chosen** Latin block.

**Kotlin JVM**:
- `KeyboardLayoutPrefs`: default `AUTO`; set/get round-trips; unknown tag → `AUTO`.
- `PhysicalKeyboardLayout.classify`: Q→A ⇒ AZERTY; Y→Z ⇒ QWERTZ; identity ⇒
  QWERTY; unknown ⇒ null (injected `probe`, no device needed).
- (New `settings-ui` test module if a pure unit is extractable; otherwise the
  prefs test in `platform-services` is the floor — noted because `settings-ui`
  has none today.)

**Gates**: `bindings_check.py --check` green; `.so` rebuilt for arm64-v8a (device)
+ the other shipped ABIs before release; `ci-local.sh` clean; CODEMAP regenerated.

**On-device** (SM-A166B): pick AZERTY → reopen keyboard → top row `azertyuiop`,
typing still lands on the right keys (render+decode consistent); QWERTZ shows y/z
swapped; switch primary to French with choice=QWERTY → still QWERTY; Auto with no
physical keyboard → QWERTY (en locale).

## 8. Definition of Done (per IMPLEMENTATION_PLAN §3.2)

Tests green; coverage ≥ 98% line on new Rust; fitness functions exit 0 (no Android
types in core; no god-files — `layout-engine` stays small); public API matches
this design; a `@BR-68` scenario per closed behaviour; BR-68 added to BRD with a
traceability row; bindings gate green; no panics on the hot path.

## 9. BRD change (flagged — BRD is source of truth)

Add to `BUSINESS_REQUIREMENTS.md`:

> **BR-68** | The user must be able to choose their alphabetic key layout
> (QWERTY, QWERTZ, AZERTY, …) independently of the selected language(s), and that
> layout is used for all Latin-script typing. The default matches the system's
> layout where detectable, falling back to QWERTY. | S | OBJ-9 |

Priority **S** (Should) — a table-stakes convenience akin to BR-47 (number/symbol
layouts), traced to OBJ-9 (core typing experience). The exact wording/traces are
finalised when the row is added to the BRD in the build phase.

## 10. Build order (increments — detailed in the plan phase)

1. `layout-engine`: `LatinLayout`, `script_of`, `default_latin_for`,
   `alpha_for(tag, override)` — RED tests first, update 3 core call sites to
   compile.
2. `featherkey-core`: `latin_override` field + `set_latin_layout` + tests.
3. FFI: `FfiLatinLayout` + `set_latin_layout`; regen bindings (gate green);
   rebuild `.so`.
4. `platform-services`: `KeyboardLayoutPrefs` + `PhysicalKeyboardLayout.classify`
   + tests.
5. `settings-ui`: the FilterChip layout row.
6. `ime-service`: `applyLayout()` wiring + bridge wrapper.
7. BRD: add BR-68 + traceability; regenerate CODEMAP.
8. On-device verify; `/r-u-sure` gate.

Each increment: failing tests first, then minimal green, then refactor — per the
repo's TDD/BDD contract. Each phase (design/plan/build) exits on a clean
`/r-u-sure`.

## Audit log

### Pass 1 — ✅ Complete and verified (design phase)
Audited the design against BR-68 and against every existing-code reference it
makes (a dedicated verification pass read the cited files).

**Requirement side — verified:**
- BR-68 is unused (highest existing is BR-67) → the number is free.
- OBJ-9 ("dead-simple to use") exists (`BUSINESS_REQUIREMENTS.md:87`) → trace valid.
- BR-47 (number/symbol layouts, priority **M**, traced OBJ-9) is a real
  table-stakes-typing analog (`:291`). Note: the design proposes BR-68 as **S**
  while its stated analog BR-47 is **M** — a judgment call, to be finalised when
  the BRD row is written (build phase, §9); not a design defect.

**Code-reference side — 10/10 claims verified against source, 2 line-number
drifts (now corrected in §2):**
- alpha_for `scripts.rs:65` (single-arg `pub fn alpha_for(tag:&str)->Self`) ✓
- 3 call sites `lib.rs:167,199,233` ✓ (a 4th at `:154` is only a doc comment)
- `azerty()`@:42 / `qwertz()`@:52 implemented + test
  `azerty_and_qwertz_are_full_latin_variants`@:114 ✓
- FFI `layout_keys`@:394 / `use_alpha_layout`@:403 / `use_numeric_layout`@:408 /
  `use_symbols_layout`@:413 / `set_active_languages`@:423 — none takes a layout
  selector ✓
- `set_layout`@`lib.rs:227` exists, NOT `#[uniffi::export]`ed (lib.rs has no
  export attrs at all — all export scaffolding is in `ffi.rs`) ✓
- `KeyboardAppearancePrefs` + `LanguagePrefs` exist with the described pattern;
  Language flow to core confirmed (`activeTags()`→`applyLanguages`→
  `bridge.setActiveLanguages`) ✓
- `renderKeys()`→`layoutKeys()`@:445, `applyLanguages` refresh@:298 ✓; keyCenters
  actually at **:448** (was cited :447) — **fixed**.
- `FALLBACK_QWERTY`@`KeyboardView.kt:1230` ✓
- height picker chips: `TypingSection`@:399, chips via `HeightOption` wrapper over
  `FilterChip`@:409–:415/:445 (was cited flatly as ":407", which is the label) —
  **fixed**.
- `settings-ui` has zero test source dirs ✓ (matches §2.6 / §7 caveat).

Changed: §2 keyCenters ref `:447`→`:448`; §2 height-picker ref rewritten to name
the `HeightOption`/`FilterChip` wrapper and correct lines.

**Open item — RESOLVED (user, 2026-07-31):** "Auto, no physical keyboard"
resolves via the **selected primary language's** locale default (reuse
`alpha_for`), *not* the device's `Configuration.getLocales()`. The user chose this
reading of "match the system" — no second locale source. D4/§5 already specify
exactly this, so no design change is needed; the choice is now locked for the
plan/build phases.

Evidence basis: source verification pass (17 tool calls) + BRD grep. No code was
run — this is a design artifact; runtime evidence belongs to the build gate.

Verdict: ✅ **Complete and verified** for the design phase. Cleared to advance to
the plan phase.

### Pass 2 — ⚠️ Done but unverified on-device (build phase, 2026-07-31)
Design realised on branch `selectable-keyboard-layout` (commits `dbac6de`..`a40cfaa`),
subagent-driven with per-task reviews + a whole-branch review. Every DoD §8 item is
green with host evidence: `ci-local.sh` 906/0, coverage ≥98% (98.96/98.77/99.23),
fitness exit 0, bindings gate OK, `@BR-68` traceable, no panics; the public API
matches §4 (`LatinLayout`, `alpha_for(tag,override)`, `set_latin_layout` core+FFI,
`KeyboardLayoutPrefs`, `PhysicalKeyboardLayout`). Latin-only (D2), survives-switch,
and Auto=selected-language-default (the D4 open item, resolved above) are proven by
Rust tests. The whole-branch review caught + fixed one real crash (an API-33 probe
call unguarded vs `minSdk=26`). **Handed off (no NDK/device this session):** rebuild
the arm64 `.so` and run the on-device acceptance (§7). ⚠️ until the device pass
confirms render+decode on the phone. Full detail in the plan's Audit log Pass 2.
