# Numeric Dialpad Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** For number and phone fields, show a telephone-style dialpad (digits with E.161 letter subtitles, a `. , 0 ⌫` row, and the standard `[ABC][emoji][space][return]` function row below) instead of the current 123 symbols page. Date/time fields keep the 123 page.

**Architecture:** Pure Kotlin platform shell. A pure `Dialpad.ROWS` table + a public `InitialPage` enum are added first; then a new private `Page.PHONE` renders the pad, the field→page classifier becomes three-way (`FieldLayout.initialPage`), and the keyboard height grows by one content row on the dialpad — reusing the existing page-varying-height mechanism. No Rust `core/`, FFI, `.so`, or bindings change.

**Tech Stack:** Kotlin, Android `InputType`, custom `View` draw/measure, JUnit 4 (plain JVM — no Robolectric). Modules: `apps/android/keyboard-view`, `apps/android/ime-service`. Builds on the unmerged `context-aware-layout` branch.

## Global Constraints

- **Pure Kotlin, no core touch.** No `core/` change, no FFI/`.so`/bindings regen. `codemap.py --check` and `bindings_check.py --check` must stay green.
- **Additive / backwards-compatible.** Only NUMBER/PHONE fields change (123 page → dialpad). DATETIME, email, URL, and ordinary fields behave exactly as before. Every new parameter has a default that preserves existing callers.
- **`Page` enum stays private**; `Page.PHONE` is **appended last** so existing ordinals (used by the `CellLayoutKey` cache key) are unchanged. The cross-module seam is the public `InitialPage` enum.
- **No new touch/commit path.** Dialpad keys are `Cell.Char` → `onCharKey` → `handleChar` → `commitText`. Backspace is `Sp.BACKSPACE`. **Letter subtitles are decorative** — a tap types the digit, never letters.
- **E.161 keypad (exact):** 1→(none), 2→ABC, 3→DEF, 4→GHI, 5→JKL, 6→MNO, 7→PQRS, 8→TUV, 9→WXYZ, 0→(none). Row 4 char keys: `.`, `,`, `0` (backspace is a function key appended by the view).
- **Subtitle drawing uses `labelPaint`** (center-aligned) with color `c.hint`, restoring `labelPaint.color = c.label` after — NOT `hintPaint` (right-aligned; used by the space-bar hint; mutating its alignment mid-draw corrupts that hint).
- **Height:** the dialpad is 4 content rows vs the usual 3; `Page.PHONE` is taller via `contentRows`, mirroring how the emoji page already varies height.
- **CODEMAP:** the local PostToolUse hook regenerates `CODEMAP.md` after `.kt` edits; each task that adds/removes a public symbol must `git add CODEMAP.md` in its commit and confirm `python3 core/tools/codemap.py --check` passes. If the hook did not run, run `python3 core/tools/codemap.py` first.
- **No Gherkin/BDD scenario** (deliberate, precedented — `core/features/` is Rust-only, traced by `bdd_check.py`; no Kotlin `inputType` classifier has a `.feature`). Behavior is pinned by JVM unit tests + on-device smoke.
- **Branch:** all work on `context-aware-layout` (already checked out). No AI-attribution trailer in any commit. Do **not** merge to `master` without an explicit request.
- **Rollback:** each task is one commit; `git revert <sha>` fully restores prior behavior (no migrations/state/core change). Task 3 writes no code.
- **Sandbox gradle:** run from `apps/android`; if a normal invocation hangs or hits EPERM/daemon errors, use `./gradlew --no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false <tasks>`.

---

### Task 1: Pure foundations — dialpad table, `InitialPage` enum, height parameter

**Files:**
- Create: `apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/Dialpad.kt`
- Test: `apps/android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/DialpadTest.kt`
- Modify: `apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt` (add top-level `enum class InitialPage`, near `RenderKey`/`FunctionKey` at lines 38–41)
- Modify: `apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardGeometry.kt:14-21` (`totalHeightPx`)
- Test: `apps/android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/KeyboardGeometryTest.kt`

**Interfaces:**
- Produces: `DialKey(val label: String, val sub: String)`; `object Dialpad { val ROWS: List<List<DialKey>> }`; `enum class InitialPage { LETTERS, NUMBERS, DIALPAD }`; `KeyboardGeometry.totalHeightPx(..., contentRows: Int = 3)`.

- [ ] **Step 1: Write the failing `DialpadTest`**

Create `DialpadTest.kt`:
```kotlin
package com.featherkey.keyboard

import org.junit.Assert.assertEquals
import org.junit.Test

class DialpadTest {
    @Test fun rows_are_the_e161_telephone_keypad() {
        val rows = Dialpad.ROWS
        assertEquals(4, rows.size)
        assertEquals(listOf(3, 3, 3, 3), rows.map { it.size })
        // labels, row-major
        assertEquals(
            listOf("1","2","3","4","5","6","7","8","9",".",",","0"),
            rows.flatten().map { it.label },
        )
        // E.161 letter subtitles ("" where the key has none)
        assertEquals(
            listOf("","ABC","DEF","GHI","JKL","MNO","PQRS","TUV","WXYZ","","",""),
            rows.flatten().map { it.sub },
        )
    }
}
```

- [ ] **Step 2: Run it — verify it fails**

Run: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest --tests '*DialpadTest'`
Expected: FAIL — `Unresolved reference: Dialpad`.

- [ ] **Step 3: Create `Dialpad.kt`**

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
        listOf(DialKey("1", ""),     DialKey("2", "ABC"),  DialKey("3", "DEF")),
        listOf(DialKey("4", "GHI"),  DialKey("5", "JKL"),  DialKey("6", "MNO")),
        listOf(DialKey("7", "PQRS"), DialKey("8", "TUV"),  DialKey("9", "WXYZ")),
        listOf(DialKey(".", ""),     DialKey(",", ""),     DialKey("0", "")),
    )
}
```

- [ ] **Step 4: Run `DialpadTest` — verify it passes**

Run: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest --tests '*DialpadTest'`
Expected: PASS.

- [ ] **Step 5: Add the `InitialPage` enum**

In `KeyboardView.kt`, immediately after the `FunctionKey` enum (line 41), add a top-level declaration:
```kotlin
/** Which page a field opens on. The view maps these to its private Page. */
enum class InitialPage { LETTERS, NUMBERS, DIALPAD }
```

- [ ] **Step 6: Write the failing `KeyboardGeometryTest` case for `contentRows`**

In `KeyboardGeometryTest.kt`, add:
```kotlin
    @Test fun dialpad_reserves_a_fourth_content_row() {
        val three = KeyboardGeometry.totalHeightPx(
            stripReserved = true, rowPx = 52f, funcPx = 54f, barPx = 46f, insetPx = 10f, stripPx = 42f,
        )
        val four = KeyboardGeometry.totalHeightPx(
            stripReserved = true, rowPx = 52f, funcPx = 54f, barPx = 46f, insetPx = 10f, stripPx = 42f,
            contentRows = 4,
        )
        assertEquals(three + 52f, four, 0.001f) // exactly one extra row
    }
```

- [ ] **Step 7: Run it — verify it fails**

Run: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest --tests '*KeyboardGeometryTest'`
Expected: FAIL — `totalHeightPx` has no `contentRows` parameter (too many arguments).

- [ ] **Step 8: Add the `contentRows` parameter**

In `KeyboardGeometry.kt`, change `totalHeightPx` (keep `contentRows` last with a default so every existing caller is unaffected):
```kotlin
    fun totalHeightPx(
        stripReserved: Boolean,
        rowPx: Float,
        funcPx: Float,
        barPx: Float,
        insetPx: Float,
        stripPx: Float,
        contentRows: Int = 3,
    ): Float = (if (stripReserved) stripPx else 0f) + rowPx * contentRows + funcPx + barPx + insetPx
```

- [ ] **Step 9: Run the keyboard-view suite — verify green**

Run: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest`
Expected: PASS (DialpadTest, KeyboardGeometryTest, and all existing tests). Nothing consumes the new symbols yet, so behavior is unchanged.

- [ ] **Step 10: Commit**

Regenerate/confirm CODEMAP (the hook likely already did; else run `python3 core/tools/codemap.py`), then:
```bash
cd /Users/pablohpsilva/Documents/android-ikeyboard
python3 core/tools/codemap.py --check   # must pass; if it fails, `python3 core/tools/codemap.py` then re-add
git add apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/Dialpad.kt \
        apps/android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/DialpadTest.kt \
        apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt \
        apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardGeometry.kt \
        apps/android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/KeyboardGeometryTest.kt \
        CODEMAP.md
git commit -m "feat(keyboard): dialpad key table, InitialPage enum, height contentRows param"
```

---

### Task 2: Render `Page.PHONE` and route number/phone fields to it

**Files:**
- Modify: `apps/android/ime-service/src/main/kotlin/com/featherkey/ime/TypingRules.kt:66-76` (`FieldLayout`: replace `opensNumeric` with `initialPage`)
- Test: `apps/android/ime-service/src/test/kotlin/com/featherkey/ime/TypingRulesTest.kt:219-231` (replace the two `opensNumeric` tests)
- Modify: `apps/android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt:257` (the `resetPage` call in `applyFieldLayout`)
- Modify: `apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt` (Page enum L171; `Cell.Char` L328; `resetPage` L208-213; `onMeasure` L348-357; `buildCells` `when(page)` L436-457; onDraw `Cell.Char` branch L542; new `drawDialKey`)

**Interfaces:**
- Consumes: `Dialpad.ROWS`, `DialKey`, `InitialPage`, `totalHeightPx(contentRows)` (Task 1); existing `Cell.Char`/`Cell.Special(Sp.BACKSPACE)`, `charRow`/`lastRow` NOT used for PHONE (custom grid), `keyBg`, `drawTextKey`.
- Produces: `FieldLayout.initialPage(inputType): InitialPage`; `KeyboardView.resetPage(initial: InitialPage = InitialPage.LETTERS)`; `Page.PHONE`; `Cell.Char(rect, label, sub = "")`.

- [ ] **Step 1: Write the failing classifier tests**

In `TypingRulesTest.kt`, REPLACE the two methods `numeric_family_fields_open_on_the_numbers_page` and `text_family_fields_do_not_open_on_the_numbers_page` (lines 219-231) with:
```kotlin
    @Test fun number_and_phone_fields_open_on_the_dialpad() {
        assertEquals(InitialPage.DIALPAD, FieldLayout.initialPage(number))
        assertEquals(InitialPage.DIALPAD, FieldLayout.initialPage(phone))
        assertEquals(InitialPage.DIALPAD, FieldLayout.initialPage(numberPin)) // numeric PIN → dialpad
    }

    @Test fun datetime_fields_keep_the_123_numbers_page() {
        assertEquals(InitialPage.NUMBERS, FieldLayout.initialPage(datetime))
    }

    @Test fun text_family_fields_open_on_letters() {
        assertEquals(InitialPage.LETTERS, FieldLayout.initialPage(text))
        assertEquals(InitialPage.LETTERS, FieldLayout.initialPage(email))
        assertEquals(InitialPage.LETTERS, FieldLayout.initialPage(uri))
        assertEquals(InitialPage.LETTERS, FieldLayout.initialPage(0))
    }
```
Add the import if not present: `import com.featherkey.keyboard.InitialPage`. (The existing `private val number/phone/datetime/numberPin/uri/webEmail/text/email` constants at lines 212-217 remain.)

- [ ] **Step 2: Run — verify it fails**

Run: `cd apps/android && ./gradlew :ime-service:testDebugUnitTest --tests '*TypingRulesTest'`
Expected: FAIL — `Unresolved reference: initialPage` (and `opensNumeric` no longer referenced).

- [ ] **Step 3: Replace `opensNumeric` with `initialPage`**

In `TypingRules.kt`, replace the `opensNumeric` function (lines 70-76) with (add `import com.featherkey.keyboard.InitialPage` at the top):
```kotlin
    /** Which page a field should open on, from its inputType. Number and phone
     *  fields get the telephone dialpad; date/time keeps the 123 numbers page (it
     *  needs / : - separators the dialpad lacks); everything else opens on letters.
     *  Covers numeric-PIN password fields (TYPE_CLASS_NUMBER → dialpad). */
    fun initialPage(inputType: Int): InitialPage =
        when (inputType and InputType.TYPE_MASK_CLASS) {
            InputType.TYPE_CLASS_NUMBER,
            InputType.TYPE_CLASS_PHONE -> InitialPage.DIALPAD
            InputType.TYPE_CLASS_DATETIME -> InitialPage.NUMBERS
            else -> InitialPage.LETTERS
        }
```
(`affixKeys` is unchanged.)

- [ ] **Step 4: Update the service call**

In `FeatherKeyImeService.kt:257`, change:
```kotlin
        keyboard?.resetPage(FieldLayout.initialPage(editorInputType))
```
(`FieldLayout` and, via keyboard-view, `InitialPage` are already on the classpath; add `import com.featherkey.keyboard.InitialPage` only if the compiler asks — the value flows through `resetPage`, so it may not need a direct import.)

- [ ] **Step 5: Add `Page.PHONE`, extend `Cell.Char`, change `resetPage`**

In `KeyboardView.kt`:
- Line 171: `private enum class Page { ALPHA, NUMBERS, SYMBOLS, EMOJI, PHONE }` (PHONE appended last).
- Line 328: `class Char(rect: RectF, val label: String, val sub: String = "") : Cell(rect)`.
- Replace `resetPage` (lines 208-213):
```kotlin
    /** Return to the field-appropriate page (called by the IME when a new field
     *  starts): dialpad for number/phone, 123 page for date/time, letters
     *  otherwise. A new field always starts unshifted and unlocked. */
    fun resetPage(initial: InitialPage = InitialPage.LETTERS) {
        page = when (initial) {
            InitialPage.DIALPAD -> Page.PHONE
            InitialPage.NUMBERS -> Page.NUMBERS
            InitialPage.LETTERS -> Page.ALPHA
        }
        shiftMode = ShiftMode.OFF // a new field starts unshifted and unlocked
        lastShiftTapAt = 0L
        requestLayout(); invalidate()
    }
```

- [ ] **Step 6: Make the dialpad taller in `onMeasure`**

In `KeyboardView.kt` `onMeasure` (lines 348-357), pass `contentRows`:
```kotlin
    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val w = MeasureSpec.getSize(widthMeasureSpec)
        val stripReserved = page != Page.EMOJI
        val h = KeyboardGeometry.totalHeightPx(
            stripReserved = stripReserved,
            rowPx = rowHeight, funcPx = funcRowHeight, barPx = bottomBarHeight,
            insetPx = bottomInset.toFloat(), stripPx = stripBand,
            contentRows = if (page == Page.PHONE) 4 else 3,
        )
        setMeasuredDimension(w, h.toInt())
    }
```

- [ ] **Step 7: Lay the dialpad grid in `buildCells`**

In the `when (page)` block (lines 436-457), add a `Page.PHONE` branch after `Page.EMOJI`. It lays 4 full-`rowHeight` rows from `Dialpad.ROWS` (rows 0-2 = 3 equal columns; row 3 = 4 equal columns `. , 0 ⌫`) directly, advancing `top` (do not use `charRow`, which centers at `baseKeyW`; the dialpad fills the content width):
```kotlin
            Page.PHONE -> {
                // `contentW` is already defined above in buildCells; reuse it.
                fun dialRow(keys: List<DialKey>, cols: Int, backspace: Boolean) {
                    val kt = top + rowGap / 2f; val kb = top + rowHeight - rowGap / 2f
                    val kw = (contentW - keyGap * (cols - 1)) / cols
                    var x = sideMargin
                    for (k in keys) {
                        out += Cell.Char(RectF(x, kt, x + kw, kb), k.label, k.sub); x += kw + keyGap
                    }
                    if (backspace) { out += Cell.Special(RectF(x, kt, x + kw, kb), Sp.BACKSPACE) }
                    top += rowHeight
                }
                dialRow(Dialpad.ROWS[0], cols = 3, backspace = false)
                dialRow(Dialpad.ROWS[1], cols = 3, backspace = false)
                dialRow(Dialpad.ROWS[2], cols = 3, backspace = false)
                dialRow(Dialpad.ROWS[3], cols = 4, backspace = true) // ". , 0 ⌫"
            }
```
The function-row block and bottom bar that follow are unchanged: `leftKind` resolves to `Sp.TO_ALPHA` (ABC) because `page != Page.ALPHA`, and affix keys are skipped for the same reason — so the dialpad automatically gets `[ABC][emoji][space][return]`.

- [ ] **Step 8: Draw the digit + subtitle**

In `KeyboardView.kt`, change the `is Cell.Char ->` draw branch (line 542) to route subtitle keys to a new helper:
```kotlin
            is Cell.Char ->
                if (cell.sub.isEmpty()) drawTextKey(canvas, cell.rect, c, cell === pressed, cell.label, cell.rect.height() * 0.5f)
                else drawDialKey(canvas, cell.rect, c, cell === pressed, cell.label, cell.sub)
```
Add the helper next to `drawTextKey` (after line 610):
```kotlin
    /** A dialpad key: the digit in the upper half, its telephone letters small and
     *  dim below. Uses labelPaint (center-aligned) for both, swapping color/size —
     *  never hintPaint, which is right-aligned for the space-bar hint. */
    private fun drawDialKey(canvas: Canvas, r: RectF, c: Palette, isPressed: Boolean, digit: String, sub: String) {
        keyBg(canvas, r, c, isPressed)
        labelPaint.color = c.label
        labelPaint.textSize = r.height() * 0.32f
        canvas.drawText(digit, r.centerX(), r.centerY() - r.height() * 0.04f, labelPaint)
        labelPaint.color = c.hint
        labelPaint.textSize = r.height() * 0.16f
        canvas.drawText(sub, r.centerX(), r.centerY() + r.height() * 0.28f, labelPaint)
        labelPaint.color = c.label // restore for subsequent keys
    }
```

- [ ] **Step 9: Verify compile + all unit suites**

Run: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest :ime-service:testDebugUnitTest`
Expected: BUILD SUCCESSFUL; `TypingRulesTest` (new `initialPage` cases), `DialpadTest`, `KeyboardGeometryTest`, and all existing tests pass.

- [ ] **Step 10: Commit**

```bash
cd /Users/pablohpsilva/Documents/android-ikeyboard
python3 core/tools/codemap.py --check   # must pass; else `python3 core/tools/codemap.py` then re-add CODEMAP.md
git add apps/android/ime-service/src/main/kotlin/com/featherkey/ime/TypingRules.kt \
        apps/android/ime-service/src/test/kotlin/com/featherkey/ime/TypingRulesTest.kt \
        apps/android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt \
        apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt \
        CODEMAP.md
git commit -m "feat(keyboard): telephone dialpad for number and phone fields"
```

---

### Task 3: Build, install, and verify on-device

**Files:** none (verification only).

The unit-testable seams (dialpad table, height, classifier) are covered by Tasks 1–2. This confirms the grid geometry, subtitle rendering, and end-to-end behavior no unit harness can exercise.

- [ ] **Step 1: Build and install**

Run: `cd apps/android && ./gradlew --no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false :app:installDebug`
Expected: BUILD SUCCESSFUL, installs on SM-A166B. No `.so` rebuild (pure Kotlin).

- [ ] **Step 2: On-device — phone field → dialpad**

Ensure FeatherKey is the active IME (see the device notes: `monkey -p com.featherkey 1` to clear stopped-state if needed, `ime set com.featherkey/.ime.FeatherKeyImeService`, force-stop the host app, reopen a fresh field). Open Contacts new-contact (`am start -a android.intent.action.INSERT -t vnd.android.cursor.dir/contact`), focus the **Phone** field, `screencap`.
Expected: the **dialpad** — `1 2 3 / 4 5 6 / 7 8 9 / . , 0 ⌫`, with letters `ABC…WXYZ` under 2–9, and `[ABC][emoji][space][return]` below. The keyboard is one row taller than the letter keyboard.

- [ ] **Step 3: On-device — number field → dialpad**

Focus a plain number field (e.g. a numeric quantity/amount input, or a numeric-PIN field). `screencap`.
Expected: the same dialpad.

- [ ] **Step 4: On-device — date/time field → 123 page (not the dialpad)**

Focus a date or time field (`TYPE_CLASS_DATETIME`), e.g. an alarm/calendar time input. `screencap`.
Expected: the **existing 123 numbers page** (`1234567890` / `- / : ; ( ) $ & @ "` / …), NOT the dialpad.
**If no such IME-driven DATETIME field is readily reachable on the device** (most
apps use native date/time *pickers*, not editable text fields), this exclusion is
already pinned by the unit test `datetime_fields_keep_the_123_numbers_page`
(`initialPage(datetime) == InitialPage.NUMBERS`) — note that in the result rather
than blocking, and do not force the dialpad on DATETIME to make a device repro.

- [ ] **Step 5: On-device — taps and unchanged fields**

Tap a couple of dialpad digits → they commit (e.g. "5", "0"); tap ⌫ → deletes. Focus a plain text field and an email field → letters page (email still shows the `@`/`.` affixes); confirm those are unchanged.

- [ ] **Step 6: Record the result**

If all checks pass, done. If any fails, capture the screenshot and fix before proceeding. Do not claim completion without the observed result.

---

## Post-implementation gate (CLAUDE.md §1.1)

After Task 3, run `/r-u-sure` against this plan's DoD and the design, append the verdict + evidence (unit output, on-device result, `codemap`/`bindings` checks) to the design doc's `## Audit log`, and loop until clean. Hold any `master` merge for an explicit request.

## Self-review notes

- **Spec coverage:** Dialpad table → Task 1 (Dialpad.kt + DialpadTest). `InitialPage` + three-way decision → Task 1 (enum) + Task 2 (`initialPage` + tests). `Page.PHONE` render + `Cell.Char.sub` + `drawDialKey` → Task 2. Height (`contentRows`) → Task 1 (param + test) + Task 2 (onMeasure). Service wiring → Task 2 step 4. Function row/affix reuse → Task 2 step 7 (no change needed). On-device (grid, subtitles, datetime exclusion) → Task 3. All design sections mapped.
- **Type consistency:** `FieldLayout.initialPage(Int): InitialPage`, `resetPage(initial: InitialPage = InitialPage.LETTERS)`, `Cell.Char(rect, label, sub = "")`, `totalHeightPx(..., contentRows: Int = 3)`, `DialKey(label, sub)`, `Dialpad.ROWS` — used identically across tasks. `Page.PHONE` appended last (cache ordinals stable). `opensNumeric` fully removed (def + caller + 2 test methods) in Task 2.
- **No placeholders:** every code step carries the actual code; every run step the exact gradle command + expected result.
