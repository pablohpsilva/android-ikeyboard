# Numeric Dialpad Page (Design Spec)

**Goal:** For numeric-only fields, show a telephone-style dialpad instead of the
current 123 symbols page — the layout people already know from placing a call.
Digits 2–9 carry their E.161 letters as a small subtitle; the last row is
`. , 0 ⌫`; the standard `[ABC][emoji][space][return]` function row stays below so
the user can still reach letters or fire the field's action.

```
 1        2 ABC    3 DEF
 4 GHI    5 JKL    6 MNO
 7 PQRS   8 TUV    9 WXYZ
 .        ,        0      ⌫
[ ABC ]  [ 🙂 ]  [   space   ]  [ ↵ ]
```

**Scope (user-decided):** `TYPE_CLASS_NUMBER` **and** `TYPE_CLASS_PHONE` → dialpad.
`TYPE_CLASS_DATETIME` → the **existing 123 numbers page** (dates/times need the
`/ : ; -` separators the dialpad lacks). Email/URL affixes and everything else are
unchanged. This refines the numeric behavior shipped earlier on this branch
(`context-aware-layout`, currently unmerged) — the dialpad supersedes the
"numeric → 123 page" rule for NUMBER/PHONE only.

**Requirements served:** BR-33 (polished, familiar interactions), BR-34 (keys
appropriately placed for the task), BR-35 (dead-simple — the expected keypad just
appears). No dedicated BR; usability, like the rest of the context-aware work.

**Architecture:** Pure Kotlin platform shell. A new private `Page.PHONE` renders a
4-row dialpad; the telephone-keypad key table is a pure, unit-tested constant; the
field→page decision moves from a boolean to a small `InitialPage` enum. **No Rust
`core/`, FFI, `.so`, or bindings change** — the numeric/symbol pages are already
hardcoded in Kotlin (`layout-engine`'s `Layout::numeric` geometry is not used by
this page), so the dialpad is likewise Kotlin-only.

## Global constraints

- **Pure Kotlin, no core touch.** No `core/` change, no FFI/`.so`/bindings regen.
  CODEMAP + bindings freshness gates stay green.
- **Additive / backwards-compatible.** Only NUMBER/PHONE fields change (123 page →
  dialpad). DATETIME, email, URL, and ordinary fields behave exactly as today.
- **`Page` stays private.** The cross-module seam is the public `InitialPage` enum
  (LETTERS/NUMBERS/DIALPAD) that `resetPage` accepts.
- **No new touch/commit path.** Dialpad keys are `Cell.Char` → `onCharKey` →
  `handleChar` → `commitText`, verbatim. Backspace is the existing
  `Sp.BACKSPACE`. Letter subtitles are **decorative** (tapping types the digit,
  never letters — matches the system dialer; no vanity-letter input).
- **Total keyboard height stays coherent with the existing page-varying model.**
  The dialpad is 4 content rows (vs 3), so it is taller — reusing the same
  mechanism by which the emoji page already has a different height.
- **Telephone keypad is E.161** (2=ABC, 3=DEF, 4=GHI, 5=JKL, 6=MNO, 7=PQRS,
  8=TUV, 9=WXYZ; 1/0/./, carry no letters).

---

## CODEMAP consultation (CLAUDE.md §2)

- **`dialpad`/`keypad`/`phone layout`/`subtitle`** — no match. New capability.
- **`featherkey-layout-engine` (Rust, domain)** has `Layout::numeric` (geometry
  only) and a `LayoutKind` enum, but the Kotlin `KeyboardView` does **not** source
  its numeric/symbol pages from it — those are hardcoded Kotlin constants
  (`NUMBERS_R1`, `NUMBERS_R2`, `SYMBOLS_R1`, `PUNCT_R3`). A dialpad is one more
  Kotlin page; it neither uses nor duplicates `layout-engine` (which knows nothing
  of Android field classes, telephone letters, or the `Page` state machine). No
  core change.
- **`FieldLayout` (`:ime-service`, `TypingRules.kt`)** already classifies the field
  → this design **extends it**: `opensNumeric(inputType): Boolean` becomes
  `initialPage(inputType): InitialPage` (three-way). `affixKeys` is unchanged.
- **`KeyboardView.resetPage(startNumeric: Boolean)`** already exists → its
  parameter changes from `Boolean` to `InitialPage` (the branch is unmerged, so
  this is an in-branch evolution, not a public break).
- **`Cell.Char(rect, label)`** and **`drawTextKey`** exist → `Cell.Char` gains an
  optional `sub: String = ""`; a small draw branch renders the subtitle with the
  existing `hintPaint`. `CellLayoutKey` needs no change — `page.ordinal` already
  keys the cache and gains the new `PHONE` value.

Decision (§2 table): capability does not exist; `FieldLayout`/`KeyboardView` own the
same responsibilities → extend them. New pure constant `Dialpad.ROWS` is one
coherent new unit (the keypad table).

## Verified current state (confirmed by reading the code)

1. **`Page` enum is private** (`KeyboardView.kt:171`): `ALPHA, NUMBERS, SYMBOLS,
   EMOJI`. `buildCells` switches on it (`KeyboardView.kt:446-455`); NUMBERS lays
   `charRow(NUMBERS_R1)`, `charRow(NUMBERS_R2)`, `lastRow(TO_SYMBOLS, PUNCT_R3,
   fill=true)`.
2. **Height already varies by page.** `onMeasure` (`KeyboardView.kt:341-350`) sets
   `stripReserved = page != Page.EMOJI` and calls
   `KeyboardGeometry.totalHeightPx(stripReserved, rowPx=rowHeight,
   funcPx=funcRowHeight, barPx=bottomBarHeight, insetPx, stripPx=stripBand)`, which
   is `(strip?) + rowPx*3 + funcPx + barPx + inset`. The `*3` is the only
   assumption of a 3-row content area.
3. **`drawTextKey`** (`KeyboardView.kt:605`) draws one centered label at a given
   size. **`hintPaint`** (`KeyboardView.kt:255`) is an existing small right-aligned
   paint already used for the space-bar language hint — reusable for subtitles.
4. **The function row** (`KeyboardView.kt:461-476` region) computes
   `leftKind = if (page == Page.ALPHA) Sp.TO_NUMBERS else Sp.TO_ALPHA`. For a new
   non-ALPHA page it already yields **ABC** (`Sp.TO_ALPHA`), and affix keys render
   only when `page == Page.ALPHA` — so the dialpad automatically gets
   `[ABC][emoji][space][return]` and no affixes, with no change to that block.
5. **`Cell.Char` → `onCharKey`** dispatch (`KeyboardView.kt:901` region;
   `handleChar` at `FeatherKeyImeService.kt:547`) commits the label via
   `commitText`. `Sp.BACKSPACE` → `onFunctionKey(BACKSPACE)` is the existing
   backspace path.
6. **`FieldLayout`** (`TypingRules.kt`) currently exposes `opensNumeric` +
   `affixKeys`; the service calls them in `applyFieldLayout()`
   (`FeatherKeyImeService.kt`), invoked from both `onStartInput` and
   `onStartInputView`.

---

## Feature 1 — The dialpad key table (pure, testable)

New in `:keyboard-view`, its own tiny file `Dialpad.kt`:

```kotlin
package com.featherkey.keyboard

/** One dialpad key: the digit/char it types, plus its E.161 telephone letters
 *  ("" when the key has none). The letters are decorative — a tap types [label]. */
data class DialKey(val label: String, val sub: String)

/** Telephone keypad (E.161) for numeric-only fields. Row-major. Row 4 lists the
 *  three character keys ". , 0"; the trailing backspace is a function key added by
 *  the view, not a DialKey. */
object Dialpad {
    val ROWS: List<List<DialKey>> = listOf(
        listOf(DialKey("1", ""),    DialKey("2", "ABC"),  DialKey("3", "DEF")),
        listOf(DialKey("4", "GHI"), DialKey("5", "JKL"),  DialKey("6", "MNO")),
        listOf(DialKey("7", "PQRS"),DialKey("8", "TUV"),  DialKey("9", "WXYZ")),
        listOf(DialKey(".", ""),    DialKey(",", ""),     DialKey("0", "")),
    )
}
```

This is the single source of truth for what the pad contains, and the one thing a
unit test can pin exactly (2→ABC … 9→WXYZ, 1/0/./, → "").

## Feature 2 — The field→page decision (three-way)

`FieldLayout` in `TypingRules.kt` replaces the boolean with an enum-returning
classifier. The enum lives in `:keyboard-view` (so `resetPage` can take it;
`:ime-service` already depends on `:keyboard-view`):

```kotlin
// :keyboard-view — KeyboardView.kt (top-level, next to RenderKey/FunctionKey)
/** Which page a field opens on. The view maps these to its private Page. */
enum class InitialPage { LETTERS, NUMBERS, DIALPAD }
```

```kotlin
// :ime-service — TypingRules.kt, object FieldLayout (replaces opensNumeric)
/** Which page a field should open on, from its inputType. Number and phone
 *  fields get the telephone dialpad; date/time keeps the 123 numbers page (it
 *  needs / : - separators); everything else opens on letters. */
fun initialPage(inputType: Int): InitialPage =
    when (inputType and InputType.TYPE_MASK_CLASS) {
        InputType.TYPE_CLASS_NUMBER,
        InputType.TYPE_CLASS_PHONE -> InitialPage.DIALPAD
        InputType.TYPE_CLASS_DATETIME -> InitialPage.NUMBERS
        else -> InitialPage.LETTERS
    }
```

`affixKeys(inputType)` is unchanged. `opensNumeric` is removed (superseded).

## Feature 3 — Rendering the dialpad

### 3a — `Page.PHONE` and `resetPage`
- `Page` enum gains `PHONE`: `{ ALPHA, NUMBERS, SYMBOLS, EMOJI, PHONE }` (still
  private). *(Appended last so existing ordinals — used by the cache key — are
  unchanged.)*
- `resetPage` takes the enum:
```kotlin
fun resetPage(initial: InitialPage = InitialPage.LETTERS) {
    page = when (initial) {
        InitialPage.DIALPAD -> Page.PHONE
        InitialPage.NUMBERS -> Page.NUMBERS
        InitialPage.LETTERS -> Page.ALPHA
    }
    shiftMode = ShiftMode.OFF
    lastShiftTapAt = 0L
    requestLayout(); invalidate()
}
```
- Service `applyFieldLayout()` calls `keyboard?.resetPage(FieldLayout.initialPage(editorInputType))`.

### 3b — `Cell.Char` gains a subtitle
```kotlin
class Char(rect: RectF, val label: String, val sub: String = "") : Cell(rect)
```
Existing `Cell.Char(rect, label)` construction is unaffected (default `sub=""`).
Dispatch is unchanged (`onCharKey?.invoke(cell.label)`).

### 3c — Height: 4 content rows
`KeyboardGeometry.totalHeightPx` gains a `contentRows: Int = 3` parameter and uses
`rowPx * contentRows`. `onMeasure` passes `contentRows = if (page == Page.PHONE) 4
else 3`. Every existing caller keeps 3 rows via the default. This mirrors how the
emoji page already changes height via `stripReserved`.

### 3d — Laying the grid (in `buildCells`)
A new `Page.PHONE` branch lays 4 rows into the content area, each a full
`rowHeight` tall (so the pad has big, dialer-like keys):
- **Rows 0–2:** three equal columns across the content width; each cell is a
  `Cell.Char(rect, digit, sub)` from `Dialpad.ROWS`.
- **Row 3:** four equal columns — `Cell.Char(".")`, `Cell.Char(",")`,
  `Cell.Char("0")` (all `sub=""`), then `Cell.Special(rect, Sp.BACKSPACE)`.
- Then the existing function-row block and bottom bar run unchanged (they start at
  `top` after the 4 rows; `leftKind` resolves to `Sp.TO_ALPHA` = ABC, affixes
  suppressed because `page != Page.ALPHA`).

The suggestion strip band is reserved as on the NUMBERS page (kept for height
simplicity; it is simply empty on a numeric field — no predictions). *(Deferred: a
future polish could drop the empty band on the dialpad; out of scope here.)*

### 3e — Drawing a key with a subtitle
In the `is Cell.Char ->` draw branch: when `sub` is empty, call the existing
`drawTextKey` (unchanged). When `sub` is non-empty, a small dedicated `drawDialKey`
helper draws the digit in the upper portion of the key and the letters beneath it,
smaller and in the theme's **hint colour** (`c.hint`). It uses **`labelPaint`**
(which is center-aligned) with a color/size swap — restoring `labelPaint.color =
c.label` after — **not** `hintPaint`: `hintPaint` is right-aligned (it draws the
space-bar language hint), so mutating its alignment mid-draw would corrupt that
hint. No other draw path changes.

---

## Alternatives rejected

- **Squeeze 4 rows into the current 3-row height** (keep total height fixed, rows
  ~0.75× tall). Rejected: the point is the familiar *big-key* dialer; short keys
  undercut it. Taller is also the lower-risk change — it reuses the existing
  page-varying-height mechanism (emoji is already a different height) rather than
  changing per-row height.
- **A new `Cell.Dial(digit, sub)` cell type.** Rejected (DRY/KISS): the only
  difference from `Cell.Char` is a decorative subtitle and a draw branch; a new
  type would also need a new `fire()` dispatch arm. Extending `Cell.Char` with an
  optional `sub = ""` reuses the existing tap→commit path untouched.
- **Tappable letters (vanity numbers, "1-800-FLOWERS").** Rejected: the system
  dialer treats keypad letters as decorative; a `TYPE_CLASS_PHONE`/`NUMBER` field
  typically rejects letters anyway, and the user asked for the dial *look*, not
  letter entry.
- **Two booleans / duplicate enums for the page decision.** Rejected: three
  initial pages (letters/numbers/dialpad) is exactly one `InitialPage` enum,
  defined once in `:keyboard-view` and consumed by both `resetPage` and
  `FieldLayout` (which `:ime-service` already depends on).
- **Drive the numeric page from Rust `layout-engine::Layout::numeric`.** Rejected:
  out of scope and not how the Kotlin numeric page works today (hardcoded); would
  pull an FFI change into a pure-UI feature for no benefit.

## Data flow

```
Field focused → onStartInput / onStartInputView
    editorInputType = info.inputType
    keyboard.resetPage(FieldLayout.initialPage(editorInputType))   // NUMBER/PHONE → DIALPAD
    keyboard.affixKeys = FieldLayout.affixKeys(editorInputType)    // unchanged; empty for numeric
  → onMeasure: contentRows = 4 for Page.PHONE → taller keyboard
  → buildCells: Page.PHONE → 4-row dialpad from Dialpad.ROWS + backspace + function row
  → tap a key: Cell.Char → onCharKey → handleChar → commitText   (digit only; letters are decorative)
     backspace: Cell.Special(BACKSPACE) → onFunctionKey(BACKSPACE)  (existing)
```

## Error handling / edge cases

- **Numeric PIN** (`TYPE_CLASS_NUMBER | VARIATION_PASSWORD`): dialpad (a PIN pad —
  appropriate). Sensitivity gating is orthogonal and unchanged.
- **Manual page switch preserved:** from the dialpad, ABC → letters, and the user
  can still reach numbers/symbols from there. We only choose the initial page.
- **DATETIME** stays on the 123 page (has `/ : ; -`); explicitly excluded from the
  dialpad.
- **Unknown/zero inputType:** `initialPage` → LETTERS (default), as today.
- **Row 4 is 4 keys vs 3 in rows 1–3** (`. , 0 ⌫`): intentional per the requested
  layout; row 4 uses 4 equal columns, rows 1–3 use 3. No decode/tap-model
  involvement (all `Cell.Char`/`Special`), so free-form geometry is fine.

## Testing

**`TypingRulesTest.kt` (JVM, extends existing):** `initialPage` →
`DIALPAD` for NUMBER, PHONE, numeric-PIN; `NUMBERS` for DATETIME (date & time
variations); `LETTERS` for plain text, email, URI, and `0`. (`affixKeys` tests
unchanged.)

**`DialpadTest.kt` (JVM, new, `:keyboard-view`):** pins the keypad table exactly —
`Dialpad.ROWS` has 4 rows; rows 0–2 have 3 keys, row 3 has 3; labels are
`1..9` then `. , 0`; subtitles are `"", ABC, DEF, GHI, JKL, MNO, PQRS, TUV, WXYZ`
and `""` for 1/0/./,. This is the regression guard on the layout content.

**`KeyboardGeometryTest.kt` (JVM):** `totalHeightPx(contentRows = 4, …)` equals the
3-row height plus one `rowPx` (proves the dialpad reserves the extra row);
`contentRows` defaults to 3 (existing callers unaffected).

**Not unit-testable (no Robolectric; `Cell`/`Page` are private):** the actual
`buildCells` grid geometry and the subtitle drawing — verified **on-device**
(below), as with the rest of `KeyboardView`.

**Definition of Done:** unit suites green (`:ime-service`, `:keyboard-view`);
`:app:installDebug` builds; on-device — a phone field and a number field open on
the dialpad (digits + letter subtitles, `. , 0 ⌫` row, `[ABC][emoji][space][↵]`
below); a date/time field still opens on the 123 page; email/URL/plain fields
unchanged; tapping a dialpad digit types that digit; backspace deletes. CODEMAP +
bindings gates green (no core change).

---

## Audit log
<!-- appended on every /r-u-sure run per CLAUDE.md §1.1 -->

### Pass 1 — ✅ Complete and verified (design phase, audited vs BRD + CLAUDE.md §1.2/§2/§4)

Gap found and fixed this pass:
- **§1.2 "alternatives rejected" was missing.** Added the section (fixed height vs
  taller rows; `Cell.Dial` type vs extending `Cell.Char`; tappable vanity letters;
  two-booleans vs one `InitialPage` enum; Rust `layout-engine` vs Kotlin).

Design-level facts verified (evidence, not adjectives):
- `:ime-service` depends on `:keyboard-view` (`ime-service/build.gradle.kts:20`), so
  `FieldLayout.initialPage` may return a `:keyboard-view` `InitialPage` enum.
- The only page switch in `KeyboardView` is a **statement** `when (page)`
  (`KeyboardView.kt:436`) — appending `Page.PHONE` compiles without an else; the
  design adds the `PHONE` branch. No exhaustive-`when` breaks.
- `opensNumeric` has exactly one production caller (`FeatherKeyImeService.kt:257`)
  plus 8 test assertions, all in-branch — the design's replacement with
  `initialPage` accounts for every one; no hidden caller.
- Height model: `KeyboardGeometry.totalHeightPx` = `(strip?) + rowPx*3 + funcPx +
  barPx + inset` — the `*3` is the sole 3-row assumption; the `contentRows`
  parameter change is sufficient and `onMeasure` already varies height by page
  (emoji). `buildCells` reserves the strip + adds Suggest cells for every non-emoji
  page, so `Page.PHONE`'s height (strip + 4·row + func + bar + inset) is internally
  consistent.
- E.161 letters verified correct (2=ABC…9=WXYZ; 1/0/./, none). Row-4 = `. , 0 ⌫`
  and the kept `[ABC][emoji][space][return]` row match the user's locked decisions;
  scope NUMBER+PHONE→dialpad / DATETIME→123 matches.
- BR mapping BR-33/34/35 (usability); pure Kotlin, no core/FFI/.so/bindings change.

No code written this phase; this verifies design correctness (symbol existence, API
validity, page/height consistency), not a running build — that is the build gate.
Design clean; advancing to the plan gate.

### Pass 2 — ✅ Complete and verified (PLAN phase, audited vs the design + CLAUDE.md §1.2)

Gaps found and fixed this pass:
- **`buildCells` `Page.PHONE` code shadowed the outer `contentW` and had a redundant
  `sideMargin.toFloat()`** (`sideMargin` is already `Float`). Fixed in the plan
  (reuse the in-scope `contentW`; `var x = sideMargin`) — before writing the gate,
  in self-review.
- **Design 3e named `hintPaint` for the subtitle**, but `hintPaint` is right-aligned
  (space-bar hint); mutating its alignment mid-draw would corrupt that hint.
  Corrected design + plan to draw the subtitle with `labelPaint` (center-aligned) +
  `c.hint` color, restoring `labelPaint.color = c.label`.
- **Task 3's DATETIME on-device step could block** (date/time fields are usually
  native pickers, not IME text fields). Added a fallback: the exclusion is pinned by
  the `datetime_fields_keep_the_123_numbers_page` unit test; note it rather than
  forcing a repro.

Plan facts verified against the code (evidence):
- `resetPage` has exactly two references — its def (`KeyboardView.kt:208`) and the
  one service caller (`FeatherKeyImeService.kt:257`); no hidden/test caller, so
  changing `Boolean` → `InitialPage` is safe.
- Scope: `out` (358), `contentW` (359), `top` (388) are all `buildCells` locals
  declared before the `when(page)` at 436 — the `Page.PHONE` branch's references
  resolve.
- `Palette` has both `label` and `hint` fields (`KeyboardView.kt:1109-1110`), so
  `drawDialKey`'s `c.label`/`c.hint` are valid.
- Height math: `onMeasure` PHONE = strip + rowPx·4 + func + bar + inset;
  `buildCells` lays 4 rows (top += rowHeight ×4) then the unchanged function
  row/bar — the row-3 four-column fit resolves to right edge `w - sideMargin`
  exactly. Consistent.
- `opensNumeric` fully removed: def (step 3), caller (step 4), and both test methods
  (step 1) — all three references accounted for.
- TDD-first where a unit seam exists (Task 1: DialpadTest, KeyboardGeometryTest;
  Task 2: TypingRulesTest `initialPage`), each red→green; rendering (Page.PHONE,
  drawDialKey grid) has no harness → on-device (Task 3), stated explicitly.
- CODEMAP handling (git add + `--check`) in Tasks 1 & 2; rollback + Gherkin-omission
  note in Global Constraints. Every design section maps to a task (self-review).

Not run: the gradle suites/build (that is the **build** gate). This pass verifies the
plan is correct, complete, and faithful to the design. Plan clean; ready for
subagent-driven execution.
