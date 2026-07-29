# Fully-numeric dialpad — design

**Date:** 2026-07-29
**Status:** design
**Slug:** fully-numeric-dialpad

## 1. Problem

The telephone dialpad shown for numeric-only fields (`TYPE_CLASS_NUMBER` /
`TYPE_CLASS_PHONE`) currently carries the same shared **function row** every other
page has: `[ABC] [emoji] [ space ] [return]`. On a numeric-only field none of
those four keys belong there — the user asked to remove the row entirely and make
the dialpad *fully numeric*.

Requirement (user, verbatim): *"on the numeric keyboard we can remove the last
line `ABC, emoji, spacebar, enter/line break`. Let's make it fully numeric."*

Decision on the Enter/action key (user, this session): **remove it too** — drop
the whole row. The globe (keyboard/IME switch) and mic stay in the bottom bar as
the escape hatch out of the dialpad.

This closes no new BR; it refines the numeric-dialpad behaviour shipped in
`a256382` (context-aware layout / numeric dialpad).

## 2. Current behaviour (what exists — per CODEMAP §2)

`apps/android/keyboard-view/.../KeyboardView.kt`:

- `buildCells(w, h)` builds, for `Page.PHONE`, four dial rows (`Dialpad.ROWS`:
  `1–9`, then `. , 0 ⌫`) via the local `dialRow(...)` helper (lines ~465–481).
- **After** the `when (page)` block, an *unconditional* `run { … }` block (lines
  ~490–512) appends the shared function row: `Sp.TO_ALPHA` (ABC), `Sp.TO_EMOJI`,
  `Sp.SPACE`, `Sp.ENTER`. This is what renders under the dialpad today.
- A second `run { … }` block (lines ~515–518) appends the bottom bar: `Sp.GLOBE`
  (left) + `Sp.MIC` (right).
- `onMeasure(...)` computes height via `KeyboardGeometry.totalHeightPx(...)` with
  `contentRows = if (page == Page.PHONE) 4 else 3` and `funcPx = funcRowHeight`
  (always).

`KeyboardGeometry.totalHeightPx(stripReserved, rowPx, funcPx, barPx, insetPx,
stripPx, contentRows=3)` = `strip + rowPx·contentRows + funcPx + barPx + insetPx`.
It already accepts `funcPx` as a parameter — no signature change needed; the
dialpad simply passes `0f`.

`Dialpad.ROWS` (the E.161 table) is **unchanged** — the digit grid itself is
correct. This change only removes the row *below* the grid.

## 3. Design

One coherent change, expressed once and consumed by the two places that care
(cell building and height measurement).

### 3.1 Single source of truth — `showsFunctionRow`

Add a private computed property to `KeyboardView`:

```kotlin
// The dialpad (numeric-only fields) is fully numeric: no shared
// [ABC][emoji][space][return] row. Every other page keeps it.
private val showsFunctionRow: Boolean get() = page != Page.PHONE
```

Both consumers below read this one property — DRY: the "PHONE has no function
row" rule lives in exactly one place.

### 3.2 `buildCells` — skip the function-row block for the dialpad

Wrap the existing unconditional function-row `run { … }` block so it only runs
when `showsFunctionRow`:

```kotlin
if (showsFunctionRow) {
    run { … existing [ABC][emoji][affix?][space][affix?][return] block … }
}
```

The block's internals are **unchanged**. When skipped, `top` is not advanced by
`funcRowHeight`, so the subsequent bottom-bar block (globe + mic) lands directly
under dial row 4 — no dead gap.

### 3.3 `onMeasure` — zero the function-row height for the dialpad

```kotlin
funcPx = if (showsFunctionRow) funcRowHeight else 0f,
```

This keeps `onMeasure`'s height in exact lock-step with what `buildCells` lays
out: dialpad height = `strip + rowHeight·4 + 0 + bottomBar + inset`.

### 3.4 What does NOT change

- `Dialpad.ROWS` / the digit grid / `drawDialKey` — untouched.
- The `TO_ALPHA` / `TO_EMOJI` / `SPACE` / `ENTER` keys still render on every
  other page (ALPHA/NUMBERS/SYMBOLS) exactly as before.
- Bottom bar (globe + mic) still renders on the dialpad — the user keeps a way to
  switch keyboards / dictate.
- No Rust `core/`, FFI, `.so`, or UniFFI-binding change. Pure Kotlin
  platform-shell.
- `FieldLayout.initialPage` / `affixKeys` classifiers — untouched.

## 4. Testing strategy

`KeyboardView` is not Robolectric-tested and its `Page`/`Cell` types are private
(established repo constraint; the prior dialpad feature verified rendering
on-device). So:

- **TDD (pure seam):** add `KeyboardGeometryTest.dialpad_has_no_function_row`
  asserting the dialpad height contract — `totalHeightPx(contentRows = 4,
  funcPx = 0f, …)` equals `strip + rowPx·4 + barPx + insetPx` (no function-row
  term). This locks the arithmetic the wiring must produce and fails before the
  `onMeasure` edit passes `0f`. Sibling to the existing
  `dialpad_reserves_a_fourth_content_row` test.
- **On-device acceptance (SM-A166B):** focus a phone/number field →
  1. dialpad shows the four dial rows;
  2. **no** `[ABC][emoji][space][return]` row beneath them;
  3. globe + mic sit directly under dial row 4 with **no dead gap**;
  4. focus a normal text field → the function row is still present (regression).

No BDD/Gherkin: `core/features/` is Rust-only (traced by `bdd_check.py`); Kotlin
platform-shell classifiers/rendering have no feature files — a precedented
deviation carried from the context-aware-layout and numeric-dialpad features.

## 5. Alternatives rejected

- **Keep only Enter** — offered to the user; they chose full removal. Rejected
  per explicit decision.
- **Delete the whole bottom bar too** — would strand the user with no keyboard
  switch on a numeric field. Rejected; globe/mic stay as the escape hatch.
- **A dialpad-specific replacement row (e.g. `+ * #`)** — out of scope; the user
  asked for *fully numeric*, and phone-field special chars weren't requested.
  Recorded as a possible future refinement, not built (KISS/YAGNI).

## 6. Definition of Done

- `dialpad_has_no_function_row` written first, seen to fail, then green.
- `showsFunctionRow` added; `buildCells` skips the function row and `onMeasure`
  zeroes `funcPx` for `Page.PHONE`; both read the one property.
- Full Kotlin unit suites green (`:keyboard-view`, `:ime-service`).
- On-device acceptance (§4) confirmed on SM-A166B with screenshots.
- `CODEMAP.md` regenerated; `codemap --check` + `bindings_check --check` pass.
- No Rust/FFI/.so/binding change (verified: `git status` shows only Kotlin +
  docs).

## Audit log

### Pass 1 — ✅ Complete and verified (design gate)
Requirements R1–R10 each map to a concrete mechanism in §3, checked against the
actual code read this session (`KeyboardView.kt:340–519`, `KeyboardGeometry.kt:14–22`):
the function-row `run{}` block (490–512) is genuinely unconditional and contains
`Sp.TO_ALPHA/TO_EMOJI/SPACE/ENTER`; the bottom bar (515–518) is separate; `onMeasure`
passes `funcPx=funcRowHeight` always. Height math verified by hand — dialpad after
change = `strip + row·4 + bar + inset` (funcPx→0), `buildCells` `top` accumulation
in lock-step, no gap/overlap. CODEMAP §2 consulted (grep): `Dialpad`,
`KeyboardGeometry.totalHeightPx`, `CellLayoutKey` pre-exist → modification, not new
code, no duplication. DRY: single `showsFunctionRow` property feeds both consumers.
Named weakness (not hidden): the pure `totalHeightPx` test locks the arithmetic
contract but cannot reach the private, non-Robolectric `KeyboardView` — the wiring's
load-bearing proof is the on-device acceptance (§4), same seam strategy as the prior
numeric-dialpad feature. No design changes required; advance to plan.

### Pass 2 — ✅ Complete and verified (build gate)
Three edits made in `KeyboardView.kt`: `showsFunctionRow` property (after `page`
var); `funcPx = if (showsFunctionRow) funcRowHeight else 0f` in `onMeasure`;
`if (showsFunctionRow) run { … }` guard around the function-row block in `buildCells`.
Test `dialpad_has_no_function_row` added to `KeyboardGeometryTest`. Tests RUN and
parsed from result XML: keyboard-view **40/0**, ime-service **76/0**, KeyboardGeometryTest
**6/0** (new test present). On-device SM-A166B: **fk1** (text field) → function row
present (regression safe); **fk2** (phone field) → dialpad fully numeric — rows `1–9`,
`. , 0 ⌫`, NO `[ABC][emoji][space][return]` row, globe+mic directly under row 4, no
dead gap. The absence of both a gap and an overlap proves both edits took effect
(skip-block AND zero-height agree) — the load-bearing proof for the private view.
`codemap --check` exit 0; `git status` = only 2 Kotlin + 2 docs, no `.so`/FFI/binding.
Not run (deliberate, disclosed): full `ci-local.sh` (zero Rust/BDD touched);
landscape/tablet visual (resolution-independent, page-conditional — no new risk).
Not committed (CLAUDE.md §8 — commit only when asked). Build gate clean.
