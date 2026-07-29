# Context-Aware Initial Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open FeatherKey on the layout the focused field needs — numeric-family fields open on the 123 page, and email/URL text fields keep letters but flank the space bar with the punctuation those fields always need (`@`/`.`/`/`).

**Architecture:** Pure Kotlin platform-shell change. A new pure classifier (`FieldLayout`) reads the field's Android `inputType` and answers two questions — "open on the numbers page?" and "which affix keys flank the space bar?". The IME service already reads `inputType` in `onStartInput`; it passes both answers to `KeyboardView`, which resets to the chosen page and renders the affix keys through the existing character-key path. No Rust core, FFI, or `.so`/bindings change.

**Tech Stack:** Kotlin, Android `InputType`/`EditorInfo`, JUnit 4 (plain JVM — the modules have no Robolectric). Modules: `apps/android/ime-service`, `apps/android/keyboard-view`.

## Global Constraints

- **Pure Kotlin, no core touch.** No change under `core/`, no FFI surface change, no UniFFI bindings regen, no native `.so` rebuild. The bindings-freshness and CODEMAP gates stay green untouched.
- **Additive and backwards-compatible.** Every existing call site behaves exactly as today unless the field is numeric/email/URL. `resetPage()`'s new parameter defaults to current behavior; `affixKeys` defaults to empty.
- **`Page` stays private to `KeyboardView`.** The service drives a single `Boolean` and a `List<String>`, never the internal page enum.
- **No new touch/commit path.** Affix keys reuse `Cell.Char` → `onCharKey` → `handleChar` → `InputConnection.commitText` verbatim.
- **Classifier keys (verbatim):** numeric page for `TYPE_CLASS_NUMBER`, `TYPE_CLASS_PHONE`, `TYPE_CLASS_DATETIME`. Affixes: email (`TYPE_TEXT_VARIATION_EMAIL_ADDRESS`, `TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS`) → `["@", "."]`; URL (`TYPE_TEXT_VARIATION_URI`) → `[".", "/"]`; index 0 sits left of the space bar, index 1 right.
- **Tested like its siblings.** JVM unit tests for the classifier (in `TypingRulesTest`) and the cache key (in `KeyboardGeometryTest`). The Android `KeyboardView`/`layoutCells()` is not unit-testable in this module (no Robolectric, `Cell` is private) — its pixel geometry is verified on-device.
- **Branch:** all work on `context-aware-layout` (already checked out). Do **not** merge to `master` without an explicit request.
- **No Gherkin/BDD scenario** (CLAUDE.md §1.2 asks for one; this is a deliberate, precedented deviation established in the design gate): `core/features/` holds only Rust-behavior features traced to BR IDs by `bdd_check.py`. No Kotlin `inputType` classifier (`AutoCaps`, `EnterKey`) has a `.feature`, because a platform-shell classifier changes no core behavior to tag. The observable behavior is instead pinned by the JVM unit tests (Tasks 1, 3) and the on-device smoke (Task 4).
- **Rollback:** each task is a single commit on the feature branch. To undo a task, `git revert <that task's commit>` (or, before its commit step, `git checkout -- <the task's files>`). No migrations, no persisted state, no core/`.so` change, so a revert fully restores prior behavior. Task 4 writes no code — nothing to roll back.

---

### Task 1: `FieldLayout` classifier (both decisions, pure)

**Files:**
- Modify: `apps/android/ime-service/src/main/kotlin/com/featherkey/ime/TypingRules.kt` (add `object FieldLayout` after `object AutoCaps`)
- Test: `apps/android/ime-service/src/test/kotlin/com/featherkey/ime/TypingRulesTest.kt`

**Interfaces:**
- Consumes: `android.text.InputType` constants (already imported in `TypingRules.kt`).
- Produces:
  - `FieldLayout.opensNumeric(inputType: Int): Boolean`
  - `FieldLayout.affixKeys(inputType: Int): List<String>` — returns `[]` or exactly two single-char strings, `[leftOfSpace, rightOfSpace]`.

- [ ] **Step 1: Write the failing tests**

Append to `TypingRulesTest.kt` (the class already defines `private val text`, `password`, `email` — reuse them; add the rest locally):

```kotlin
    // --- Context-aware initial layout (FieldLayout) ---------------------------

    private val phone = InputType.TYPE_CLASS_PHONE
    private val datetime = InputType.TYPE_CLASS_DATETIME
    private val number = InputType.TYPE_CLASS_NUMBER
    private val numberPin = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD
    private val uri = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
    private val webEmail = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS

    @Test fun numeric_family_fields_open_on_the_numbers_page() {
        assertTrue(FieldLayout.opensNumeric(number))
        assertTrue(FieldLayout.opensNumeric(phone))
        assertTrue(FieldLayout.opensNumeric(datetime))
        assertTrue(FieldLayout.opensNumeric(numberPin)) // numeric PIN is still numeric
    }

    @Test fun text_family_fields_do_not_open_on_the_numbers_page() {
        assertFalse(FieldLayout.opensNumeric(text))
        assertFalse(FieldLayout.opensNumeric(email))
        assertFalse(FieldLayout.opensNumeric(uri))
        assertFalse(FieldLayout.opensNumeric(0)) // unknown/unspecified field
    }

    @Test fun email_and_url_fields_get_affix_keys() {
        assertEquals(listOf("@", "."), FieldLayout.affixKeys(email))
        assertEquals(listOf("@", "."), FieldLayout.affixKeys(webEmail))
        assertEquals(listOf(".", "/"), FieldLayout.affixKeys(uri))
    }

    @Test fun ordinary_and_non_text_fields_get_no_affix_keys() {
        assertTrue(FieldLayout.affixKeys(text).isEmpty())
        assertTrue(FieldLayout.affixKeys(password).isEmpty())
        assertTrue(FieldLayout.affixKeys(number).isEmpty()) // numeric class, not text
        assertTrue(FieldLayout.affixKeys(0).isEmpty())
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd apps/android && ./gradlew :ime-service:testDebugUnitTest --tests '*TypingRulesTest'`
Expected: FAIL — `Unresolved reference: FieldLayout`.

- [ ] **Step 3: Write the minimal implementation**

In `TypingRules.kt`, add after the closing brace of `object AutoCaps`:

```kotlin
/** Which initial layout a field should present, from its inputType. Pure so it
 *  unit-tests off-device like its siblings above. */
object FieldLayout {
    /** True when a field is numeric in nature and should open on the 123 page:
     *  the number, phone, and date/time classes. Covers numeric-PIN password
     *  fields, which are TYPE_CLASS_NUMBER. */
    fun opensNumeric(inputType: Int): Boolean =
        when (inputType and InputType.TYPE_MASK_CLASS) {
            InputType.TYPE_CLASS_NUMBER,
            InputType.TYPE_CLASS_PHONE,
            InputType.TYPE_CLASS_DATETIME -> true
            else -> false
        }

    /** Punctuation keys to flank the space bar on the letter page for this field:
     *  [leftOfSpace, rightOfSpace], or empty for fields that need none. Email
     *  addresses always carry "@" and "."; URLs carry "." and "/". Only text-class
     *  fields qualify (number/symbol pages already carry these characters). */
    fun affixKeys(inputType: Int): List<String> {
        if (inputType and InputType.TYPE_MASK_CLASS != InputType.TYPE_CLASS_TEXT) {
            return emptyList()
        }
        return when (inputType and InputType.TYPE_MASK_VARIATION) {
            InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS,
            InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS -> listOf("@", ".")
            InputType.TYPE_TEXT_VARIATION_URI -> listOf(".", "/")
            else -> emptyList()
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd apps/android && ./gradlew :ime-service:testDebugUnitTest --tests '*TypingRulesTest'`
Expected: PASS (all `TypingRulesTest` cases, old and new).

- [ ] **Step 5: Commit**

```bash
git add apps/android/ime-service/src/main/kotlin/com/featherkey/ime/TypingRules.kt \
        apps/android/ime-service/src/test/kotlin/com/featherkey/ime/TypingRulesTest.kt
git commit -m "feat(ime): classify a field's initial layout (numeric page + email/URL affixes)"
```

---

### Task 2: Numeric-family fields open on the 123 page

**Files:**
- Modify: `apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt:201-206` (`resetPage`)
- Modify: `apps/android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt:235` (the `resetPage()` call)

**Interfaces:**
- Consumes: `FieldLayout.opensNumeric(inputType)` (Task 1); `editorInputType` (already assigned at `FeatherKeyImeService.kt:231`).
- Produces: `KeyboardView.resetPage(startNumeric: Boolean = false)` — resets to the numbers page when `true`, else the letter page.

*No unit test:* `resetPage` mutates the private `Page` on an Android `View`, and the module has no Robolectric — the decision it consumes (`opensNumeric`) is already covered by Task 1, and the page reset is verified by the on-device smoke in Task 4's Definition of Done. This task is a compile-and-wire step.

- [ ] **Step 1: Change `resetPage` to accept the initial page**

In `KeyboardView.kt`, replace the body of `resetPage` (currently lines 201-206):

```kotlin
    /** Return to the field-appropriate page (called by the IME when a new field
     *  starts). Numeric fields open on the numbers page; everything else on the
     *  letter page. A new field always starts unshifted and unlocked. */
    fun resetPage(startNumeric: Boolean = false) {
        page = if (startNumeric) Page.NUMBERS else Page.ALPHA
        shiftMode = ShiftMode.OFF // a new field starts unshifted and unlocked
        lastShiftTapAt = 0L
        requestLayout(); invalidate()
    }
```

- [ ] **Step 2: Wire the service to pass the field's page**

In `FeatherKeyImeService.kt`, replace line 235 `keyboard?.resetPage()` with:

```kotlin
        keyboard?.resetPage(FieldLayout.opensNumeric(editorInputType))
```

(`FieldLayout` is in the same package `com.featherkey.ime`, so no import is needed.)

- [ ] **Step 3: Verify it compiles and the existing suites pass**

Run: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest :ime-service:testDebugUnitTest`
Expected: BUILD SUCCESSFUL — no existing test regresses (the only `resetPage()` caller now passes the default-equivalent value for text fields).

- [ ] **Step 4: Commit**

```bash
git add apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt \
        apps/android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt
git commit -m "feat(keyboard): open numeric-family fields on the 123 page"
```

---

### Task 3: Email/URL fields render affix keys flanking the space bar

**Files:**
- Modify: `apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardGeometry.kt:34-39` (`CellLayoutKey`)
- Test: `apps/android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/KeyboardGeometryTest.kt`
- Modify: `apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt` (add `affixKeys` property; `layoutCells` cache key at line 333; the function-row block at lines 452-467)
- Modify: `apps/android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt` (set `affixKeys` in `onStartInput`, after the `resetPage` call)

**Interfaces:**
- Consumes: `FieldLayout.affixKeys(inputType)` (Task 1); the existing `Cell.Char(rect: RectF, label: String)` and its dispatch through `onCharKey` (`KeyboardView.kt:901`).
- Produces: `KeyboardView.affixKeys: List<String>` (settable property, default empty); `CellLayoutKey(..., affixKeys: List<String> = emptyList())`.

- [ ] **Step 1: Write the failing cache-key test**

In `KeyboardGeometryTest.kt`, inside `memo_key_is_equal_for_identical_inputs_and_differs_per_field`, add after the `keysVersion` assertion (line 36):

```kotlin
        assertNotEquals(base, CellLayoutKey(1080, 900, 0, 3, listOf("@", "."))) // affix keys
        assertEquals(
            CellLayoutKey(1080, 900, 0, 3, listOf("@", ".")),
            CellLayoutKey(1080, 900, 0, 3, listOf("@", ".")),
        ) // equal affixes still hit the cache
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest --tests '*KeyboardGeometryTest'`
Expected: FAIL — `CellLayoutKey` has no 5-argument constructor (too many arguments).

- [ ] **Step 3: Add the affix field to `CellLayoutKey`**

In `KeyboardGeometry.kt`, extend the data class (it must stay the *last* parameter with a default so the existing positional call sites keep compiling):

```kotlin
data class CellLayoutKey(
    val width: Int,
    val height: Int,
    val pageOrdinal: Int,
    val keysVersion: Int,
    val affixKeys: List<String> = emptyList(),
)
```

- [ ] **Step 4: Run the cache-key test to verify it passes**

Run: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest --tests '*KeyboardGeometryTest'`
Expected: PASS.

- [ ] **Step 5: Add the `affixKeys` property to `KeyboardView`**

In `KeyboardView.kt`, next to the other field-driven properties (e.g. just below the `keys` property near line 69–73), add:

```kotlin
    /** Punctuation keys to flank the space bar on the letter page (email/URL
     *  fields). Set per field by the IME; empty for ordinary fields. */
    var affixKeys: List<String> = emptyList()
        set(value) { if (field != value) { field = value; requestLayout(); invalidate() } }
```

- [ ] **Step 6: Include the affixes in the layout cache key**

In `KeyboardView.kt:333`, change the cache-key construction to:

```kotlin
        val key = CellLayoutKey(width, height, page.ordinal, keysVersion, affixKeys)
```

- [ ] **Step 7: Render the affix keys in the function row**

In `KeyboardView.kt`, replace the function-row `run { … }` block (lines 452-467) with the version below. The space-bar span is computed as before, then narrowed to make room for the affix keys only when on the letter page with a two-key affix set; with no affixes the geometry is byte-for-byte the current layout.

```kotlin
        // Function row: [123|ABC] [emoji] [affix?] [ space ] [affix?] [ return ].
        // The emoji key sits between the page-switch key and the space bar
        // (iOS-style). On the letter page, email/URL fields insert one affix key on
        // each side of the space bar (email → "@" | space | "." ; URL → "." | space
        // | "/"); the space bar shrinks to fit. Other pages carry these characters
        // already, so no affixes are added there.
        run {
            val kt = top + rowGap / 2f; val kb = top + funcRowHeight - rowGap / 2f
            val fSideW = baseKeyW * 2f
            val leftKind = if (page == Page.ALPHA) Sp.TO_NUMBERS else Sp.TO_ALPHA
            out += Cell.Special(RectF(sideMargin, kt, sideMargin + fSideW, kb), leftKind)
            val retLeft = w - sideMargin - fSideW
            out += Cell.Special(RectF(retLeft, kt, w - sideMargin, kb), Sp.ENTER)
            val emojiLeft = sideMargin + fSideW + keyGap
            val emojiW = baseKeyW * 1.2f
            out += Cell.Special(RectF(emojiLeft, kt, emojiLeft + emojiW, kb), Sp.TO_EMOJI)

            var spaceLeft = emojiLeft + emojiW + keyGap
            var spaceRight = retLeft - keyGap
            if (page == Page.ALPHA && affixKeys.size == 2) {
                val aw = baseKeyW
                out += Cell.Char(RectF(spaceLeft, kt, spaceLeft + aw, kb), affixKeys[0])
                spaceLeft += aw + keyGap
                out += Cell.Char(RectF(spaceRight - aw, kt, spaceRight, kb), affixKeys[1])
                spaceRight -= aw + keyGap
            }
            out += Cell.Special(RectF(spaceLeft, kt, spaceRight, kb), Sp.SPACE)
            top += funcRowHeight
        }
```

- [ ] **Step 8: Wire the service to set the affixes**

In `FeatherKeyImeService.kt`, in `onStartInput`, immediately after the `resetPage` line from Task 2, add:

```kotlin
        keyboard?.affixKeys = FieldLayout.affixKeys(editorInputType)
```

- [ ] **Step 9: Verify it compiles and all unit suites pass**

Run: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest :ime-service:testDebugUnitTest`
Expected: BUILD SUCCESSFUL, all tests pass.

- [ ] **Step 10: Commit**

```bash
git add apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardGeometry.kt \
        apps/android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/KeyboardGeometryTest.kt \
        apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt \
        apps/android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt
git commit -m "feat(keyboard): flank the space bar with @/./ affix keys in email and URL fields"
```

---

### Task 4: Build, install, and verify on-device

**Files:** none (verification only).

The unit-testable seams are covered by Tasks 1 and 3. This task confirms the pixel geometry and the end-to-end behavior that no unit harness in these modules can exercise.

- [ ] **Step 1: Build and install the debug app**

Run (per the sandbox build note): `cd apps/android && ./gradlew --no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false :app:installDebug`
Expected: BUILD SUCCESSFUL, installs on the connected device (SM-A166B). No `.so` rebuild is needed — this change is pure Kotlin.

- [ ] **Step 2: On-device smoke — numeric field**

Open a numeric field (e.g. the Contacts new-contact **Phone** field, or a dialer). Focus it.
Expected: FeatherKey opens on the **123** page, not letters.

- [ ] **Step 3: On-device smoke — email field**

Open an email field (e.g. a login/signup email input, or the "Email" field of a new contact). Focus it.
Expected: the letter page is shown with an **`@`** key immediately left of the space bar and a **`.`** key immediately right of it. Tapping `@` inserts `@`.

- [ ] **Step 4: On-device smoke — URL field**

Focus a browser address bar (a field with `TYPE_TEXT_VARIATION_URI`).
Expected: the letter page with **`.`** left of space and **`/`** right of space.

- [ ] **Step 5: On-device smoke — ordinary field (regression guard)**

Focus a plain text field (e.g. a notes body / message box).
Expected: the function row is unchanged — `[123] [emoji] [ space ] [return]`, no affix keys, space bar full width.

- [ ] **Step 6: Record the result**

If all five checks pass, the feature is done. If any fails, capture what appeared (screenshot) and fix before proceeding. Do not claim completion without the observed result.

---

## Post-implementation gate (CLAUDE.md §1.1)

After Task 4, run `/r-u-sure` against this plan's Definition of Done and the design spec, and append the verdict + evidence (unit output, on-device result) to the design doc's `## Audit log`. Loop until a clean verdict. Only then is the build phase complete. Hold any merge to `master` for an explicit request from the user.

## Self-review notes

- **Spec coverage:** Feature 1 (numeric page) → Tasks 1+2. Feature 2 (affixes) → Tasks 1+3. Cache correctness (`CellLayoutKey`) → Task 3 steps 1-4. Data-flow wiring (`onStartInput`) → Task 2 step 2 + Task 3 step 8. Testing strategy (classifier + cache key pure tests, geometry on-device) → Tasks 1, 3, 4. All spec sections mapped.
- **Type consistency:** `FieldLayout.opensNumeric(Int): Boolean` and `FieldLayout.affixKeys(Int): List<String>` used identically in Tasks 1-3. `resetPage(startNumeric: Boolean = false)` and `KeyboardView.affixKeys: List<String>` names match across tasks. `CellLayoutKey`'s new `affixKeys` field is the 5th positional param (defaulted) everywhere it is constructed.
- **No placeholders:** every code step shows the actual code; every run step shows the exact gradle command and expected result.

## Audit log

### Pass 1 — ✅ Complete and verified (plan phase, audited against the design)
Gaps found and fixed this pass:
- **No rollback per increment** (CLAUDE.md §1.2 requires it). Added a Global-
  Constraints rollback line: each task is one commit → `git revert` restores prior
  behavior; no migrations/state/core/`.so`, so revert is total.
- **No Gherkin scenario** (§1.2 asks for one). Added a Global-Constraints note
  making the omission explicit and precedented (matches the design gate's BDD
  finding: `core/features/` = Rust-only; no Kotlin classifier has a `.feature`).
- **Setter inconsistency:** the plan's guarded `affixKeys` setter differed from the
  design's illustrative snippet; synced the design to the guarded version.

Verified plan↔design consistency (evidence):
- Feature 1 → Tasks 1+2; Feature 2 → Tasks 1+3; testing (classifier + CellLayoutKey
  cache test + on-device) → Tasks 1,3,4. Every design section maps to a task.
- Types match across tasks: `FieldLayout.opensNumeric(Int):Boolean`,
  `affixKeys(Int):List<String>`, `resetPage(startNumeric:Boolean=false)`,
  `KeyboardView.affixKeys:List<String>`, `CellLayoutKey(...,affixKeys=emptyList())`
  as 5th positional param. Affix order `["@","."]`→ index0 left / index1 right in
  Task 3 step 7 matches Task 1's test and the design.
- Space-bar starvation check: `keyGap=dp(5)`, `sideMargin=dp(4)`
  (`KeyboardView.kt:235-236`), `baseKeyW≈30dp` on a 360dp screen → space bar stays
  ~4 key-widths wide after two affix keys are carved out. No guard needed (KISS).
- TDD-first honored where a unit seam exists (Task 1 classifier, Task 3
  `CellLayoutKey`); view-wiring steps (Task 2, Task 3 rendering) have no unit
  harness and are explicitly deferred to the on-device smoke (Task 4), stated in
  each task rather than silently skipped.

Not yet run: the gradle test/build commands (that is the **build** phase's gate,
Task 4 + the post-implementation `/r-u-sure`). This pass verifies the plan is
correct, complete, and faithful to the design — not that the code runs. Plan phase
is clean; ready for execution.
