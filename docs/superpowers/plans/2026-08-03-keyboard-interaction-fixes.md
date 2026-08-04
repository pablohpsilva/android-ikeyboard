# Keyboard Interaction Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop two-finger taps from firing bogus swipes (BR-41), and move the settings/voice icons into the suggestion strip while dropping the 46 dp bottom bar (BR-42/BR-43 presentation).

**Architecture:** Both changes are confined to `apps/android/keyboard-view`. Fix 2 extracts pure strip geometry into `KeyboardGeometry` (host-tested first), then rewires `buildCells`. Fix 1 makes `onTouchEvent` pointer-aware (device-verified — no host harness for `MotionEvent` in this repo). The Rust core, FFI, and UniFFI bindings are untouched.

**Tech Stack:** Kotlin, Android custom `View`, JUnit (host unit tests for pure geometry).

## Global Constraints

- No Rust/FFI/bindings changes; no `.so`/keystore/`local.properties` commits.
- No AI attribution in commits/PRs/comments.
- Fitness (`≤ 500 lines/file, ≤ 60 lines/function`) is **Rust-only** (`fitness/check.py` scans `core/crates/**/*.rs`); Kotlin is not line/function gated (`KeyboardView.kt` is already 1237 lines and passes). Still, keep the new Kotlin helpers small and prefer extraction over inflating `onTouchEvent` — good practice, not a gate.
- `CODEMAP.md` is generated — regenerate with `python3 core/tools/codemap.py`, never hand-edit; `git add` it with any `.kt` change.
- Kotlin: prefer smart-cast (`val x = field; if (x != null)`) over `!!`.
- **Two separate verification lanes.** Kotlin unit tests run via gradle: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest` (sandbox flags: `--no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false`). `bash core/tools/ci-local.sh` is the **Rust** gate — it does *not* run gradle; here it verifies codemap freshness (the `.kt` changes flow into CODEMAP) and that bindings stay byte-identical (unchanged — no FFI touched). Device behavior (Fix 1, Fix 2 layout) is verified on the connected device, per repo convention that swipe/touch lifecycle is not host-testable.
- Commit only when the user asks.

## File Structure

- `apps/android/keyboard-view/.../KeyboardGeometry.kt` — add `Rect4`, `StripRects`, `stripSubRects`; remove `barPx` from `totalHeightPx`.
- `apps/android/keyboard-view/src/test/.../KeyboardGeometryTest.kt` — new `stripSubRects` tests; update existing tests for the `barPx`-less signature.
- `apps/android/keyboard-view/.../KeyboardView.kt` — Fix 2: rewire strip in `buildCells`, delete bottom-bar block + `bottomBarHeight`, fix the `totalHeightPx` call. Fix 1: pointer-aware `onTouchEvent` + `finalizeGestureOrTap` helper + `gesturePointerId` state.

---

### Task 1: `KeyboardGeometry.stripSubRects` pure helper (host TDD)

**Files:**
- Modify: `apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardGeometry.kt`
- Test: `apps/android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/KeyboardGeometryTest.kt`

**Interfaces:**
- Produces: `data class Rect4(left, top, right, bottom: Float)`; `data class StripRects(settings: Rect4, suggestions: List<Rect4>, voice: Rect4)`; `KeyboardGeometry.stripSubRects(width: Float, band: Float, iconW: Float): StripRects`.

- [ ] **Step 1: Write the failing test** (append to `KeyboardGeometryTest.kt`)

```kotlin
@Test fun strip_places_square_icons_left_and_right_with_three_middle_cells() {
    val s = KeyboardGeometry.stripSubRects(width = 300f, band = 42f, iconW = 42f)
    assertEquals(Rect4(0f, 0f, 42f, 42f), s.settings)      // left square
    assertEquals(Rect4(258f, 0f, 300f, 42f), s.voice)      // right square
    assertEquals(3, s.suggestions.size)
    // middle [42,258] split into three equal 72-wide cells, contiguous:
    assertEquals(Rect4(42f, 0f, 114f, 42f), s.suggestions[0])
    assertEquals(Rect4(114f, 0f, 186f, 42f), s.suggestions[1])
    assertEquals(Rect4(186f, 0f, 258f, 42f), s.suggestions[2])
    // no gaps/overlap at the icon boundaries:
    assertEquals(s.settings.right, s.suggestions.first().left, 0.001f)
    assertEquals(s.voice.left, s.suggestions.last().right, 0.001f)
}

@Test fun strip_clamps_icon_width_so_the_middle_never_collapses() {
    // iconW larger than a third of the width is clamped to width/3.
    val s = KeyboardGeometry.stripSubRects(width = 90f, band = 42f, iconW = 42f)
    assertEquals(30f, s.settings.right, 0.001f)            // clamped to 90/3
    assertEquals(60f, s.voice.left, 0.001f)
    assertEquals(3, s.suggestions.size)
    assertEquals(30f, s.suggestions.first().left, 0.001f)
    assertEquals(60f, s.suggestions.last().right, 0.001f)
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest --tests '*KeyboardGeometryTest*'` (sandbox flags per the gradle memory: `--no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false`).
Expected: FAIL — `stripSubRects` / `Rect4` / `StripRects` unresolved.

- [ ] **Step 3: Write the minimal implementation** (in `KeyboardGeometry.kt`)

```kotlin
/** A rectangle as plain floats — Android-type-free, so it unit-tests off-device. */
data class Rect4(val left: Float, val top: Float, val right: Float, val bottom: Float)

/** Layout of the suggestion strip band: a square settings icon pinned left, a
 *  square voice icon pinned right, and three equal suggestion cells between. */
data class StripRects(val settings: Rect4, val suggestions: List<Rect4>, val voice: Rect4)
```

Add to `object KeyboardGeometry`:

```kotlin
/** Sub-rects of the strip band `[0,width] x [0,band]`: a square settings icon
 *  (left), a square voice icon (right), and three equal suggestion cells filling
 *  the middle. [iconW] is clamped to `[0, width/3]` so the middle never collapses
 *  on very narrow screens. */
fun stripSubRects(width: Float, band: Float, iconW: Float): StripRects {
    val ic = iconW.coerceIn(0f, width / 3f)
    val settings = Rect4(0f, 0f, ic, band)
    val voice = Rect4(width - ic, 0f, width, band)
    val cw = (width - 2f * ic) / 3f
    val suggestions = (0..2).map { i -> Rect4(ic + i * cw, 0f, ic + (i + 1) * cw, band) }
    return StripRects(settings, suggestions, voice)
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: same as Step 2. Expected: PASS (2 new tests).

---

### Task 2: Drop `barPx` from `totalHeightPx` (host TDD)

The bottom bar is being removed, so its height term no longer exists. Remove the parameter (not just pass 0) — a dead parameter is a KISS violation — and update the existing tests to the new geometry.

**Files:**
- Modify: `KeyboardGeometry.kt` (signature), `KeyboardGeometryTest.kt` (5 methods).

**Interfaces:**
- Produces: `KeyboardGeometry.totalHeightPx(stripReserved, rowPx, funcPx, insetPx, stripPx, contentRows=3): Float` (no `barPx`).

- [ ] **Step 1: Update the existing tests to the new signature (the failing step)**

Edit every `totalHeightPx(` call in `KeyboardGeometryTest.kt` to drop `barPx = 46f,` and remove the `+ 46f` / `+ bar` from each expected value. Result:

```kotlin
@Test fun strip_bearing_height_includes_the_strip_band() {
    val h = KeyboardGeometry.totalHeightPx(
        stripReserved = true, rowPx = 52f, funcPx = 54f, insetPx = 10f, stripPx = 42f,
    )
    assertEquals(42f + 52f * 3 + 54f + 10f, h, 0.001f)
}

@Test fun emoji_height_excludes_the_strip_band() {
    val h = KeyboardGeometry.totalHeightPx(
        stripReserved = false, rowPx = 52f, funcPx = 54f, insetPx = 10f, stripPx = 42f,
    )
    assertEquals(52f * 3 + 54f + 10f, h, 0.001f)
}

@Test fun dialpad_reserves_a_fourth_content_row() {
    val three = KeyboardGeometry.totalHeightPx(
        stripReserved = true, rowPx = 52f, funcPx = 54f, insetPx = 10f, stripPx = 42f,
    )
    val four = KeyboardGeometry.totalHeightPx(
        stripReserved = true, rowPx = 52f, funcPx = 54f, insetPx = 10f, stripPx = 42f, contentRows = 4,
    )
    assertEquals(three + 52f, four, 0.001f)
}

@Test fun dialpad_has_no_function_row() {
    val dialpad = KeyboardGeometry.totalHeightPx(
        stripReserved = true, rowPx = 52f, funcPx = 0f, insetPx = 10f, stripPx = 42f, contentRows = 4,
    )
    assertEquals(42f + 52f * 4 + 10f, dialpad, 0.001f)
    val withFuncRow = KeyboardGeometry.totalHeightPx(
        stripReserved = true, rowPx = 52f, funcPx = 54f, insetPx = 10f, stripPx = 42f, contentRows = 4,
    )
    assertEquals(withFuncRow - 54f, dialpad, 0.001f)
}
```

(The `content_top_offset...` and `memo_key...` tests are unchanged.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `./gradlew :keyboard-view:testDebugUnitTest --tests '*KeyboardGeometryTest*'` (sandbox flags).
Expected: FAIL — too many arguments / signature mismatch (the impl still has `barPx`).

- [ ] **Step 3: Remove `barPx` from the implementation**

```kotlin
fun totalHeightPx(
    stripReserved: Boolean,
    rowPx: Float,
    funcPx: Float,
    insetPx: Float,
    stripPx: Float,
    contentRows: Int = 3,
): Float = (if (stripReserved) stripPx else 0f) + rowPx * contentRows + funcPx + insetPx
```

Update the doc comment: `content rows + function row + system inset, plus a reserved suggestion band`.

- [ ] **Step 4: Fix the one `totalHeightPx` call site so the module still compiles**

`./gradlew :keyboard-view:testDebugUnitTest` compiles `main` + `test`, so the `KeyboardView.kt` caller must match the new signature now (the bottom-bar *block* and `bottomBarHeight` property stay until Task 3 — they still compile). At the height call (~364) drop just the `barPx = bottomBarHeight,` argument:

```kotlin
KeyboardGeometry.totalHeightPx(
    stripReserved = stripReserved,
    rowPx = rowHeight, funcPx = if (showsFunctionRow) funcRowHeight else 0f,
    insetPx = bottomInset.toFloat(), stripPx = stripBand,
    // ...contentRows as before...
)
```
(`bottomBarHeight` is still referenced by the not-yet-removed bottom-bar block, so it stays defined — module compiles.)

- [ ] **Step 5: Run tests to verify they pass**

Run: same as Step 2. Expected: PASS — the module compiles and all `KeyboardGeometryTest` cases are green.

---

### Task 3: Rewire the strip; delete the bottom bar (device-verified)

**Files:**
- Modify: `KeyboardView.kt` — `buildCells` strip section (~397-399), the `totalHeightPx` call (~364), delete the bottom-bar block (~519-524), delete `bottomBarHeight` (~249). Update the class doc (~5-7) to drop "globe/mic bar".

**Interfaces:**
- Consumes: `KeyboardGeometry.stripSubRects` (Task 1), `totalHeightPx` without `barPx` (Task 2).

- [ ] **Step 1: Replace the strip cell construction**

Replace:
```kotlin
val cw = w / 3f
for (i in 0..2) out += Cell.Suggest(RectF(i * cw, 0f, (i + 1) * cw, band), i)
```
with:
```kotlin
// Strip band: [settings] [sugg0] [sugg1] [sugg2] [voice]. iconW = band → square
// icons. The globe/mic reuse Sp.GLOBE/Sp.MIC, so their draw + onFunctionKey
// wiring is unchanged (globe → Settings, mic → voice).
val strip = KeyboardGeometry.stripSubRects(w.toFloat(), band, band)
fun rf(r: Rect4) = RectF(r.left, r.top, r.right, r.bottom)
out += Cell.Special(rf(strip.settings), Sp.GLOBE)
strip.suggestions.forEachIndexed { i, r -> out += Cell.Suggest(rf(r), i) }
out += Cell.Special(rf(strip.voice), Sp.MIC)
```

- [ ] **Step 2: Delete the bottom-bar block**

Remove:
```kotlin
// Bottom bar: globe (left) + mic (right), icon-only.
run {
    val sz = bottomBarHeight
    out += Cell.Special(RectF(sideMargin, top, sideMargin + sz, top + sz), Sp.GLOBE)
    out += Cell.Special(RectF(w - sideMargin - sz, top, w - sideMargin, top + sz), Sp.MIC)
}
```

- [ ] **Step 3: Remove `bottomBarHeight`** (the `totalHeightPx` call was already fixed in Task 2)

Delete the property:
```kotlin
private val bottomBarHeight get() = dp(46f) * animatedHeightScale
```
Confirm no other references remain: `grep -n bottomBarHeight KeyboardView.kt` → none (the block from Step 2 was its last user). Update the class KDoc line that says "and a globe/mic bar" to describe the strip-hosted icons.

- [ ] **Step 4: Regenerate CODEMAP and build**

Run: `python3 core/tools/codemap.py`, then build the module:
`cd apps/android && ./gradlew :keyboard-view:assembleDebug --no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 5: Build + install the app and device-verify**

Build the `.so` + APK per the gradle/`.so` memory, install on the connected device. Verify on a standard page: strip shows `[globe] suggestions [mic]`; tapping globe opens Settings; tapping mic starts voice; the bottom globe/mic bar is gone and the keyboard is visibly shorter; the space/return row is not clipped by the system nav buttons; the emoji page still returns to letters via its ABC control.

- [ ] **Step 6: Commit** (only if the user has asked to commit)

```bash
git add apps/android/keyboard-view CODEMAP.md docs/superpowers
git commit -m "feat(keyboard): host settings/voice icons in the suggestion strip; drop bottom bar"
```

---

### Task 4: Pointer-locked gesture tracking (device-verified) — BR-41

Makes the swipe single-touch: a second finger never joins the trail; if it arrives before a glide starts, the first key commits as a tap and the second finger takes over. `MotionEvent` logic has no host harness here → device-verified against the acceptance scenario below.

**Acceptance scenario (BR-41):**
```gherkin
@BR-41
Scenario: Two near-simultaneous key taps type two letters, not a swipe
  Given the letters page is shown
  When I tap "a" and "s" almost at the same time with two fingers
  Then the field contains "as"
  And no swipe trail is drawn
  And no swipe-decoded word is committed
```

**Files:**
- Modify: `KeyboardView.kt` — swipe state (~281-285), `onTouchEvent` (~777-859), `resetGesture` (~884-889); add `finalizeGestureOrTap`.

- [ ] **Step 1: Add the owning-pointer state**

In the `--- Swipe/glide typing state ---` block:
```kotlin
private var gesturePointerId = MotionEvent.INVALID_POINTER_ID
```

- [ ] **Step 2: Record the owner on `ACTION_DOWN`**

In the letter-press branch (`if (page == Page.ALPHA && hit is Cell.Letter)`), after `gestureCell = hit`, add:
```kotlin
gesturePointerId = event.getPointerId(0)
```

- [ ] **Step 3: Read only the owning pointer on `ACTION_MOVE`**

Replace the `ACTION_MOVE` body's coordinate read:
```kotlin
MotionEvent.ACTION_MOVE -> {
    if (accentActive()) { updateAccentSelection(event.x); return true }
    val g = gestureCell ?: return true
    val idx = event.findPointerIndex(gesturePointerId)
    if (idx < 0) return true                 // the owning finger isn't in this event
    val p = PointF(event.getX(idx), event.getY(idx))
    val last = trail.lastOrNull()
    if (last != null) trailLen += kotlin.math.hypot(p.x - last.x, p.y - last.y)
    trail.add(p)
    if (!gesturing && trailLen > gestureStartThreshold()) {
        gesturing = true; pressed = null
        removeCallbacks(longPressRunnable)
    }
    if (gesturing) invalidate()
    return true
}
```

- [ ] **Step 4: Handle a second finger (`ACTION_POINTER_DOWN`)**

Add a branch:
```kotlin
MotionEvent.ACTION_POINTER_DOWN -> {
    // A swipe is single-touch. If the first finger is a pending letter that
    // hasn't become a glide, commit it as a tap and hand the press to the new
    // finger (two-finger fast typing). If a glide is already under way, or the
    // accent popup is open, ignore the extra finger.
    val g = gestureCell
    if (page == Page.ALPHA && g != null && !gesturing && !accentActive()) {
        val down = trail.firstOrNull()
        removeCallbacks(longPressRunnable)
        fire(g, down?.x ?: g.rect.centerX(), down?.y ?: g.rect.centerY()) // commit finger 1
        val ai = event.actionIndex
        val nx = event.getX(ai); val ny = event.getY(ai)
        val hit = cells.firstOrNull { it.rect.contains(nx, ny) } ?: nearestCell(nx, ny)
        if (hit is Cell.Letter) {                                        // finger 2 → new press
            keyPressFeedback()
            gesturePointerId = event.getPointerId(ai)
            gestureCell = hit; gesturing = false; trailLen = 0f
            trail.clear(); trail.add(PointF(nx, ny))
            pressed = hit; invalidate()
            if (Accents.hasVariants(hit.label.firstOrNull() ?: ' ')) {
                postDelayed(longPressRunnable, longPressTimeoutMs())
            }
        } else {
            resetGesture()
        }
    }
    return true
}
```

- [ ] **Step 5: Finalize when the owner lifts (`ACTION_POINTER_UP`) + extract helper**

Add the helper (keep it under 60 lines):
```kotlin
/** Commit the current press: a glide of ≥3 points fires a swipe; anything
 *  shorter is a tap reported at its down point. Always resets gesture state. */
private fun finalizeGestureOrTap() {
    val g = gestureCell
    if (g != null) {
        if (gesturing && trail.size >= 3) {
            lastShiftTapAt = 0L
            onGesture?.invoke(ArrayList(trail), letterCenters())
        } else {
            val down = trail.firstOrNull()
            fire(g, down?.x ?: g.rect.centerX(), down?.y ?: g.rect.centerY())
        }
    }
    resetGesture()
}
```
Add the branch:
```kotlin
MotionEvent.ACTION_POINTER_UP -> {
    if (event.getPointerId(event.actionIndex) == gesturePointerId) {
        removeCallbacks(longPressRunnable)
        finalizeGestureOrTap()
    }
    return true
}
```
Rewrite `ACTION_UP` to reuse the helper (accent path unchanged):
```kotlin
MotionEvent.ACTION_UP -> {
    removeCallbacks(longPressRunnable)
    stopBackspaceRepeat()
    if (accentActive()) {
        val chosen = accentSession.release()
        if (chosen != null) { lastShiftTapAt = 0L; onAccentKey?.invoke(chosen) }
        resetAccent(); resetGesture()
        return true
    }
    finalizeGestureOrTap()
    return true
}
```

- [ ] **Step 6: Clear the owner in `resetGesture`**

```kotlin
private fun resetGesture() {
    gestureCell = null; gesturing = false; trailLen = 0f
    gesturePointerId = MotionEvent.INVALID_POINTER_ID
    trail.clear()
    if (pressed != null) pressed = null
    invalidate()
}
```

- [ ] **Step 7: Build**

Run: `python3 core/tools/codemap.py` (no new public symbols expected, but keep it fresh), then
`cd apps/android && ./gradlew :keyboard-view:assembleDebug --no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false`.
Expected: BUILD SUCCESSFUL.

- [ ] **Step 8: Build + install and device-verify the acceptance scenario**

Install on the connected device. Verify:
1. Two-finger near-simultaneous taps on separated keys → both letters typed, **no** trail line, **no** swipe word (the BR-41 scenario).
2. A normal one-finger swipe still decodes a word (regression check).
3. Long-press accents still open on a single held key (regression check).

- [ ] **Step 9: Commit** (only if the user has asked to commit)

```bash
git add apps/android/keyboard-view CODEMAP.md docs/superpowers
git commit -m "fix(keyboard): lock swipe to one pointer so two-finger taps don't glide (BR-41)"
```

---

### Task 5: Full gate

- [ ] **Step 1: Run the Kotlin unit tests (gradle)**

Run: `cd apps/android && ./gradlew :keyboard-view:testDebugUnitTest --no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false`.
Expected: PASS, including the new `stripSubRects` cases and the updated `totalHeightPx` cases.

- [ ] **Step 2: Run the Rust/tooling gate (ci-local)**

Run: `bash core/tools/ci-local.sh`.
Expected: all gates pass. Rust gates are unchanged (core untouched); the load-bearing checks here are **codemap fresh** (the `.kt` edits are reflected — regenerate + `git add CODEMAP.md` if it flags stale) and **bindings byte-identical** (no FFI change).

- [ ] **Step 3: Build-phase `/r-u-sure`**

Audit the build against this plan; append the result to the design's `## Audit log`. Require: host tests green with counts, both device scenarios confirmed, fitness within limits, CODEMAP regenerated and staged.

## Self-Review

- **Spec coverage:** Fix 1 → Tasks 1-nothing/Task 4 (BR-41); Fix 2 layout → Tasks 1-3 (BR-42/43 presentation); inset preservation → Task 2 (barPx removed, insetPx kept) + Task 3 Step 5 device check. All design sections mapped.
- **Placeholder scan:** none — every code step carries real code.
- **Type consistency:** `Rect4`/`StripRects`/`stripSubRects` defined in Task 1 and consumed in Task 3; `finalizeGestureOrTap`/`gesturePointerId` defined and used within Task 4; `totalHeightPx` new signature defined **and** its call site fixed in Task 2, so every increment compiles.

## Audit log

### Pass 1 — 🚧 Incomplete → resolved
Plan gate (plan vs. design). Gaps found, all verified against the real repo:
1. **False claim — ci-local runs the Kotlin tests.** Read `core/tools/ci-local.sh`: it runs cargo + python tooling only, no `./gradlew`. Fixed: Global Constraints now names two lanes (gradle for Kotlin units, ci-local for the Rust/codemap/bindings gates); Task 5 split into Step 1 (gradle) + Step 2 (ci-local).
2. **False constraint — fitness ≤500/≤60 binds the Kotlin work.** Read `fitness/check.py`: it scans `core/crates/**/*.rs` only (that is why the 1237-line `KeyboardView.kt` passes). Fixed: constraint reworded as Rust-only; small Kotlin helpers kept as good practice.
3. **Broken increment — removing `barPx` (Task 2) left `KeyboardView.kt` uncompilable until Task 3.** Since `testDebugUnitTest` compiles `main`+`test`, Task 2 now also fixes the single `totalHeightPx` call site (bottom-bar block/`bottomBarHeight` stay until Task 3, still compiling). Each task is now independently buildable.

### Pass 2 — ✅ Complete and verified (plan phase)
Evidence:
- **Design coverage:** BR-41 → Task 4 (pointer lock, with the `@BR-41` acceptance scenario the design mandates); Fix-2 layout → Tasks 1-3; inset-preservation design finding → Task 2 (`barPx` dropped, `insetPx` kept) + Task 3 Step 5 device check. Emoji-page edge case → Task 3 Step 5.
- **TDD ordering honoured** where a host harness exists: Tasks 1-2 write/adjust failing tests before impl; Task 4 is device-verified (design established no `MotionEvent` host harness exists — not a skipped test).
- **Real code in every code step**; no placeholders. Verification commands carry the sandbox gradle flags from the repo's gradle memory.
- **Increment independence** confirmed after fixing gap 3.
Not verified (correctly deferred to build): actual test runs and device acceptance — those are Task 5 / the build gate.

**Verdict: ✅ Complete and verified (plan).**
