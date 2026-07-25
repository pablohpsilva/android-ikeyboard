# FeatherKey Performance — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the surgical, low-risk, high-impact performance wins and the measurement foundation: a repeatable on-device jank harness + controlled baseline, an installable optimized (R8) build with trimmed ABIs, the suggestion-strip layout-shift fix, and `buildCells` memoization — with before/after numbers.

**Architecture:** All code changes are confined to the `keyboard-view` Android module (a custom `View`) and the `app` module's Gradle build; plus a shell script under `tools/perf/`. Hot-path *decisions* (keyboard height, cell-layout memo key) are extracted into pure, Context-free Kotlin and unit-tested in a new `keyboard-view` JVM test source set — the project's established "pure logic + JUnit" pattern (cf. `SessionPlan`, `DefaultImeStatus`). No Rust, no FFI surface, no other module changes.

**Tech Stack:** Kotlin, Android custom `View` (Canvas), Gradle Kotlin DSL (AGP), JUnit4, `adb`/`dumpsys gfxinfo`.

## Global Constraints

- **No feature removed, no existing test deleted or weakened.** Every change stays behind the current suites and the CI gate.
- **No runtime network** (unchanged invariant).
- **Keyboard height must not depend on suggestion contents.** After Task 3 the reported IME height is constant whether or not suggestions are shown, so the host app never shifts on suggestion open/close.
- **Memo key = exactly the inputs `buildCells` reads.** `buildCells` reads `width`, `height`, `page`, and `renderKeys` (for column count and the letter cells). It does **not** read `shifted` (shift is applied at draw time, `KeyboardView.kt:359`) and — after Task 3 — does not read `suggestions` (the strip band is always reserved). The memo key therefore includes width, height, page, and a `keys` version; it excludes `shifted` and `suggestions`. Getting this wrong causes stale layout.
- **Reference device for all measurements:** Galaxy A16 (`SM-A166B`), Exynos `s5e8535`, Android 14. Perf budget: **janky frames < 5%**, **99th-percentile frame time < 32ms** on the typing sequence (Phase-exit target; each task records its before/after).
- **The optimized build must be run on-device, not just built** — R8 can break JNA/UniFFI reflective FFI at runtime, not at compile.
- **ABI set = `arm64-v8a` + `armeabi-v7a`** (covers all real devices + the Apple-silicon arm64 emulator). Do not drop `arm64-v8a`.
- Current verified constants (`KeyboardView.kt`): `stripHeight=dp(42)`, `rowHeight=dp(52)`, `funcRowHeight=dp(54)`, `bottomBarHeight=dp(46)`; height formula `strip + rowHeight*3 + funcRowHeight + bottomBarHeight + bottomInset`; `private enum class Page { ALPHA, NUMBERS, SYMBOLS, EMOJI }`.

---

## File Structure

- `tools/perf/jank.sh` — **Create.** On-device jank capture + budget assertion.
- `android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardGeometry.kt` — **Create.** Pure height + memo-key logic (no Android imports).
- `android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/KeyboardGeometryTest.kt` — **Create.** JUnit tests for the pure logic.
- `android/keyboard-view/build.gradle.kts` — **Modify.** Add `testImplementation(junit)`.
- `android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt` — **Modify.** `onMeasure`, `buildCells` strip offset, `suggestions` setter, `keys` setter (version bump), `onDraw` memoization.
- `android/app/build.gradle.kts` — **Modify.** `benchmark` build type + `ndk.abiFilters`.
- `android/app/proguard-rules.pro` — **Create/Modify.** R8 keep rules for JNA/UniFFI/Compose.
- `docs/superpowers/specs/2026-07-25-performance-optimization-design.md` — **Modify.** Record measurement numbers in the appendix.

---

### Task 1: On-device jank harness + controlled debug baseline

**Files:**
- Create: `tools/perf/jank.sh`

**Interfaces:**
- Produces: `tools/perf/jank.sh <serial> [budget_pct]` — resets `gfxinfo`, drives a fixed input sequence against a focused text field, prints `total/janky%/95th/99th/slow-UI`, exits non-zero if janky% > budget.

**Context:** This must exist first so every later claim of improvement is measured. The keyboard renders in the `com.featherkey` process, so `dumpsys gfxinfo com.featherkey` reports its frames even though the foreground app is the host. The operator focuses a text field (the script opens Settings search as a convenience) before running.

- [ ] **Step 1: Write the harness script**

Create `tools/perf/jank.sh`:

```bash
#!/usr/bin/env bash
# On-device jank measurement for the FeatherKey keyboard (com.featherkey).
# Usage: tools/perf/jank.sh <serial> [budget_pct]
# Precondition: a text field is focused and the FeatherKey keyboard is visible.
# The keyboard occupies the bottom band; taps/swipes below drive
# keystroke -> decode -> suggestion -> redraw cycles. Coordinates target the
# reference device (SM-A166B, 1080x2340); adjust for other screens.
set -uo pipefail
PKG=com.featherkey
SERIAL="${1:?usage: jank.sh <serial> [budget_pct]}"
BUDGET="${2:-5}"
adb() { command adb -s "$SERIAL" "$@"; }

# Ensure a focused text field (Settings search) so the keyboard is up.
adb shell am start -a android.settings.SETTINGS >/dev/null 2>&1; sleep 2
adb shell input keyevent 84 >/dev/null 2>&1; sleep 1   # SEARCH -> focuses a field on most Samsung builds

adb shell dumpsys gfxinfo "$PKG" reset >/dev/null 2>&1

# Fixed input sequence: 40 letter taps across the key band + 4 swipes (gesture typing).
KEYS_Y1=1900; KEYS_Y2=1980; KEYS_Y3=2060
for rep in $(seq 1 4); do
  for x in 90 210 330 450 570 690 810 930; do
    adb shell input tap "$x" "$KEYS_Y1" >/dev/null 2>&1
  done
  for x in 150 390 630 870; do
    adb shell input tap "$x" "$KEYS_Y2" >/dev/null 2>&1
  done
  adb shell input swipe 90 "$KEYS_Y2" 930 "$KEYS_Y2" 300 >/dev/null 2>&1  # swipe-to-type
done

OUT="$(adb shell dumpsys gfxinfo "$PKG" 2>/dev/null)"
total=$(echo "$OUT" | sed -n 's/.*Total frames rendered: \([0-9]*\).*/\1/p' | head -1)
janky=$(echo "$OUT" | sed -n 's/.*Janky frames: [0-9]* (\([0-9.]*\)%).*/\1/p' | head -1)
p95=$(echo "$OUT" | sed -n 's/.*95th percentile: \([0-9]*\)ms.*/\1/p' | head -1)
p99=$(echo "$OUT" | sed -n 's/.*99th percentile: \([0-9]*\)ms.*/\1/p' | head -1)
slowui=$(echo "$OUT" | sed -n 's/.*Number Slow UI thread: \([0-9]*\).*/\1/p' | head -1)

echo "total_frames=$total janky_pct=$janky p95_ms=$p95 p99_ms=$p99 slow_ui=$slowui budget_pct=$BUDGET"
awk -v j="${janky:-100}" -v b="$BUDGET" 'BEGIN{ exit !(j+0 <= b+0) }'
rc=$?
[ "$rc" -eq 0 ] && echo "JANK OK (<= ${BUDGET}%)" || echo "JANK OVER BUDGET (> ${BUDGET}%)"
exit "$rc"
```

- [ ] **Step 2: Make it executable and capture the debug baseline**

Run:
```bash
chmod +x tools/perf/jank.sh
# With the debug build installed and a text field focused:
tools/perf/jank.sh RZCY51D0T1K 100   # budget 100 => never fails; we only want the numbers
```
Expected: a line `total_frames=… janky_pct=… p95_ms=… p99_ms=… slow_ui=…`. Record it.

- [ ] **Step 3: Record the baseline in the spec appendix**

Append the captured `janky_pct / p95 / p99 / slow_ui` (debug build) under "Appendix — measurement log" in `docs/superpowers/specs/2026-07-25-performance-optimization-design.md`, labeled "Phase 1 controlled debug baseline".

- [ ] **Step 4: Commit**

```bash
git add tools/perf/jank.sh docs/superpowers/specs/2026-07-25-performance-optimization-design.md
git commit -m "perf(tools): add on-device jank harness + record controlled debug baseline"
```

---

### Task 2: Installable optimized build + ABI trim

**Files:**
- Modify: `android/app/build.gradle.kts`
- Create/Modify: `android/app/proguard-rules.pro`

**Interfaces:**
- Produces: `./gradlew :app:assembleBenchmark` → an R8-minified, non-debuggable, debug-signed, arm64+armv7-only APK that installs and runs on the A16 with FFI intact.

**Context:** The reported "heaviness" is partly the debug build. This adds an installable optimized variant to measure/ship, and trims the 7 shipped ABIs to the two that matter. R8 must not strip JNA/UniFFI reflective entry points.

- [ ] **Step 1: Add the benchmark build type and ABI filter**

In `android/app/build.gradle.kts`, replace the `buildTypes { release { … } }` block and add an `ndk` filter in `defaultConfig`:

```kotlin
defaultConfig {
    applicationId = "com.featherkey"
    minSdk = 26
    targetSdk = 35
    versionCode = 1
    versionName = "0.1.0"
    ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a") }
}

buildTypes {
    release {
        isMinifyEnabled = true
        proguardFiles(
            getDefaultProguardFile("proguard-android-optimize.txt"),
            "proguard-rules.pro",
        )
    }
    // Installable, R8-optimized, non-debuggable build for on-device measurement
    // and shipping, signed with the debug key until a release key is provisioned.
    create("benchmark") {
        initWith(getByName("release"))
        isDebuggable = false
        signingConfig = signingConfigs.getByName("debug")
        matchingFallbacks += listOf("release")
    }
}
```

- [ ] **Step 2: Add R8 keep rules for JNA / UniFFI / Compose**

Create/append `android/app/proguard-rules.pro`:

```proguard
# JNA (UniFFI Kotlin bindings call the Rust core through JNA reflection).
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
# UniFFI-generated bindings + our FFI records/enums are reached reflectively via JNA.
-keep class com.featherkey.ffi.generated.** { *; }
-keep class uniffi.** { *; }
# Compose tooling keeps (belt-and-suspenders; AGP usually injects these).
-keep class androidx.compose.runtime.** { *; }
```

- [ ] **Step 3: Build, install, and RUN on-device (FFI smoke test)**

Run:
```bash
cd android && ./gradlew :app:assembleBenchmark
adb -s RZCY51D0T1K install -r app/build/outputs/apk/benchmark/app-benchmark.apk
```
Then on-device: enable/select FeatherKey, focus a field, type a few letters and one swipe. Expected: keyboard shows suggestions and swipe produces a word — proves R8 did not break the FFI path. Verify the APK ships only two ABIs:
```bash
unzip -l app/build/outputs/apk/benchmark/app-benchmark.apk | grep -oE 'lib/[^/]+/' | sort -u
```
Expected: only `lib/arm64-v8a/` and `lib/armeabi-v7a/`.

- [ ] **Step 4: Re-baseline on the benchmark build + record**

Run `tools/perf/jank.sh RZCY51D0T1K 100` against the benchmark build; append the numbers to the spec appendix labeled "Phase 1 benchmark-build baseline". Note APK size delta vs the 18.3 MB debug APK.

- [ ] **Step 5: Commit**

```bash
git add android/app/build.gradle.kts android/app/proguard-rules.pro docs/superpowers/specs/2026-07-25-performance-optimization-design.md
git commit -m "perf(build): add installable R8 benchmark build type; trim ABIs to arm64+armv7"
```

---

### Task 3: Reserve the suggestion-strip band (fix layout shift ①)

**Files:**
- Create: `android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardGeometry.kt`
- Create: `android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/KeyboardGeometryTest.kt`
- Modify: `android/keyboard-view/build.gradle.kts`
- Modify: `android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt`

**Interfaces:**
- Produces: `KeyboardGeometry.totalHeightPx(stripReserved, rowPx, funcPx, barPx, insetPx): Float` and `KeyboardGeometry.contentTopPx(stripReserved, stripPx): Float`. Both are pure; **neither takes a `suggestions` parameter** — that is the guarantee that height no longer depends on strip contents.

**Context:** `onMeasure` (`KeyboardView.kt:189-190`) currently adds `stripHeight` only when `suggestions` is non-empty, and the `suggestions` setter calls `requestLayout()` on empty↔non-empty flips (`:71`). Strip-bearing pages (ALPHA/NUMBERS/SYMBOLS) will now always reserve the band; the emoji page (which `buildCells` skips, `:199`) keeps its current height and is handled in Task 4.

- [ ] **Step 1: Add the JUnit test dependency**

In `android/keyboard-view/build.gradle.kts`, add to `dependencies`:
```kotlin
    testImplementation("junit:junit:4.13.2")
```

- [ ] **Step 2: Write the failing pure-geometry test**

Create `android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/KeyboardGeometryTest.kt`:
```kotlin
package com.featherkey.keyboard

import org.junit.Assert.assertEquals
import org.junit.Test

class KeyboardGeometryTest {
    // px stand-ins (dp-independent): strip=42, row=52, func=54, bar=46, inset=10
    @Test fun strip_bearing_height_includes_the_strip_band() {
        val h = KeyboardGeometry.totalHeightPx(
            stripReserved = true, rowPx = 52f, funcPx = 54f, barPx = 46f, insetPx = 10f, stripPx = 42f,
        )
        assertEquals(42f + 52f * 3 + 54f + 46f + 10f, h, 0.001f)
    }

    @Test fun emoji_height_excludes_the_strip_band() {
        val h = KeyboardGeometry.totalHeightPx(
            stripReserved = false, rowPx = 52f, funcPx = 54f, barPx = 46f, insetPx = 10f, stripPx = 42f,
        )
        assertEquals(52f * 3 + 54f + 46f + 10f, h, 0.001f)
    }

    @Test fun content_top_offset_matches_the_reserved_band() {
        assertEquals(42f, KeyboardGeometry.contentTopPx(stripReserved = true, stripPx = 42f), 0.001f)
        assertEquals(0f, KeyboardGeometry.contentTopPx(stripReserved = false, stripPx = 42f), 0.001f)
    }
    // Note: neither function takes `suggestions` — height cannot depend on strip contents by construction.
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd android && ./gradlew :keyboard-view:testDebugUnitTest`
Expected: FAIL — `KeyboardGeometry` unresolved.

- [ ] **Step 4: Write the pure geometry**

Create `android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardGeometry.kt`:
```kotlin
package com.featherkey.keyboard

/**
 * Pure keyboard geometry — no Android types, so it is unit-testable off-device.
 * The strip band is reserved on strip-bearing pages regardless of whether any
 * suggestions are currently shown, so the reported IME height never changes on
 * suggestion open/close (the host app stops shifting).
 */
object KeyboardGeometry {
    /** Total keyboard height in px: three letter rows + function row + bottom bar
     *  + system inset, plus a reserved suggestion band ([stripPx]) when
     *  [stripReserved]. Deliberately has no `suggestions` parameter — the height
     *  cannot depend on strip contents. */
    fun totalHeightPx(
        stripReserved: Boolean,
        rowPx: Float,
        funcPx: Float,
        barPx: Float,
        insetPx: Float,
        stripPx: Float,
    ): Float = (if (stripReserved) stripPx else 0f) + rowPx * 3 + funcPx + barPx + insetPx

    /** The y-offset where the key grid starts: below a reserved strip band, else 0. */
    fun contentTopPx(stripReserved: Boolean, stripPx: Float): Float =
        if (stripReserved) stripPx else 0f
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd android && ./gradlew :keyboard-view:testDebugUnitTest`
Expected: PASS (3 tests).

- [ ] **Step 6: Wire `onMeasure` to the pure height (constant across suggestion state)**

In `KeyboardView.kt`, replace `onMeasure` (lines 185-192):
```kotlin
override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
    val w = MeasureSpec.getSize(widthMeasureSpec)
    val stripReserved = page != Page.EMOJI
    val h = KeyboardGeometry.totalHeightPx(
        stripReserved = stripReserved,
        rowPx = rowHeight, funcPx = funcRowHeight, barPx = bottomBarHeight,
        insetPx = bottomInset.toFloat(), stripPx = stripHeight,
    )
    setMeasuredDimension(w, h.toInt())
}
```

- [ ] **Step 7: Reserve the strip band unconditionally in `buildCells`**

In `KeyboardView.kt`, replace the strip block (lines 217-222):
```kotlin
        // Strip band is always reserved on this (non-emoji) page, so the key grid
        // sits at a constant offset whether or not suggestions are shown. The three
        // Suggest cells always exist (they draw nothing while suggestions is empty).
        val cw = w / 3f
        for (i in 0..2) out += Cell.Suggest(RectF(i * cw, 0f, (i + 1) * cw, stripHeight), i)
        var top = KeyboardGeometry.contentTopPx(stripReserved = true, stripPx = stripHeight)
```

- [ ] **Step 8: Drop the toggle `requestLayout()` in the `suggestions` setter**

In `KeyboardView.kt`, replace the setter (lines 67-73):
```kotlin
    var suggestions: List<String> = emptyList()
        set(value) {
            field = value
            invalidate() // height is constant now; only the strip text repaints
        }
```

- [ ] **Step 9: Build the module and run its tests**

Run: `cd android && ./gradlew :keyboard-view:testDebugUnitTest :app:assembleDebug`
Expected: BUILD SUCCESSFUL; geometry tests pass.

- [ ] **Step 10: On-device verification — no shift**

Install the debug build; focus a field; type until suggestions appear and clear (backspace to empty). Expected: the keyboard/host boundary does **not** move when the strip populates/empties. (Capture a before screenshot from the pre-change build if useful.)

- [ ] **Step 11: Commit**

```bash
git add android/keyboard-view/
git commit -m "perf(keyboard): reserve suggestion-strip band so the IME height is constant (fixes host-app shift)"
```

---

### Task 4: Constant height across pages incl. emoji (finish ①)

**Files:**
- Modify: `android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt`

**Interfaces:**
- Consumes: `KeyboardGeometry.totalHeightPx` from Task 3.

**Context:** After Task 3 the emoji page is still shorter by `stripHeight` (Task 3 sets `stripReserved = page != Page.EMOJI`), so switching alpha↔emoji still resizes the window. The emoji page draws/hit-tests its own grid (`buildCells` returns empty for it, `:199`) sized to the view height, so making its height constant means its grid gets `stripHeight` more room. This eliminates the remaining (unreported) page-switch shift.

- [ ] **Step 1: Make the height constant for every page**

In `KeyboardView.kt` `onMeasure`, set `stripReserved = true` unconditionally (all pages reserve the band):
```kotlin
    val stripReserved = true // constant keyboard height across all pages (incl. emoji)
```

- [ ] **Step 2: Offset the emoji page's own layout by the reserved band**

Inspect `drawEmojiPage` and the emoji grid's top origin / hit-testing (the emoji draw path starting near `KeyboardView.kt:337` and its geometry helpers). Add `stripHeight` to the emoji content's top origin and to the emoji hit-test math so the grid/tabs render within the taller envelope with the reserved band above them (blank). Show the exact edited lines in the implementation (the emoji origin currently starts at 0; it must start at `stripHeight`). Keep the emoji scroll clamp using the emoji content area height (`height - stripHeight - controlBarHeight`).

- [ ] **Step 3: Build + on-device check**

Run: `cd android && ./gradlew :app:assembleDebug`; install; switch alpha↔emoji↔numbers. Expected: keyboard height is identical across all pages (no host-app shift on any page switch); emoji grid is fully visible and scrolls correctly.

- [ ] **Step 4: Commit**

```bash
git add android/keyboard-view/
git commit -m "perf(keyboard): constant height across all pages incl. emoji (no shift on page switch)"
```

---

### Task 5: Memoize `buildCells` (fix ② per-frame rebuild)

**Files:**
- Modify: `android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardGeometry.kt`
- Modify: `android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/KeyboardGeometryTest.kt`
- Modify: `android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt`

**Interfaces:**
- Produces: `data class CellLayoutKey(width: Int, height: Int, pageOrdinal: Int, keysVersion: Int)` (pure, value-equality). `buildCells` output is a pure function of exactly these inputs.

**Context:** `onDraw` calls `cells = buildCells(width, height)` on **every** frame (`KeyboardView.kt:339`) — every keystroke, suggestion change, press highlight, and gesturing MOVE — rebuilding ~80 objects + two `groupBy` maps + a `TreeMap` + per-row sorts. Memoize on the exact inputs `buildCells` reads: width, height, page, and a `keys` version (bumped when the alpha layout changes). `shifted` is excluded (draw-time uppercasing, `:359`); `suggestions` is excluded (Task 3 made the strip band constant).

- [ ] **Step 1: Write the failing memo-key test**

Add to `KeyboardGeometryTest.kt`:
```kotlin
    // requires: import org.junit.Assert.assertNotEquals
    @Test fun memo_key_is_equal_for_identical_inputs_and_differs_per_field() {
        val base = CellLayoutKey(width = 1080, height = 900, pageOrdinal = 0, keysVersion = 3)
        assertEquals(base, CellLayoutKey(1080, 900, 0, 3))
        // Any layout-affecting input change produces a different key:
        assertNotEquals(base, CellLayoutKey(1081, 900, 0, 3)) // width
        assertNotEquals(base, CellLayoutKey(1080, 901, 0, 3)) // height
        assertNotEquals(base, CellLayoutKey(1080, 900, 1, 3)) // page
        assertNotEquals(base, CellLayoutKey(1080, 900, 0, 4)) // keys version (language switch)
    }
```
(Add `import org.junit.Assert.assertNotEquals` to the test file.)

- [ ] **Step 2: Run to verify it fails**

Run: `cd android && ./gradlew :keyboard-view:testDebugUnitTest`
Expected: FAIL — `CellLayoutKey` unresolved.

- [ ] **Step 3: Add the memo key type**

Append to `KeyboardGeometry.kt`:
```kotlin
/**
 * Identity of a computed key-cell layout. `buildCells` output depends on exactly
 * these inputs; a repeated draw with an equal key can reuse the cached cells.
 * Excludes `shifted` (applied at draw time) and `suggestions` (strip band is
 * always reserved), which do not change cell geometry.
 */
data class CellLayoutKey(
    val width: Int,
    val height: Int,
    val pageOrdinal: Int,
    val keysVersion: Int,
)
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd android && ./gradlew :keyboard-view:testDebugUnitTest`
Expected: PASS.

- [ ] **Step 5: Bump a keys version when the alpha layout changes**

In `KeyboardView.kt`, change the `keys` setter (line 64) to bump a version, and add the field:
```kotlin
    private var keysVersion = 0

    var keys: List<RenderKey> = emptyList()
        set(value) { field = value; keysVersion++; requestLayout(); invalidate() }
```

- [ ] **Step 6: Cache cells in `onDraw`, rebuilding only on key change**

In `KeyboardView.kt`, add cache fields near `cells` and replace the non-emoji `cells = buildCells(width, height)` (line 339) with a memoized fetch:
```kotlin
    private var cachedCells: List<Cell>? = null
    private var cachedKey: CellLayoutKey? = null

    private fun layoutCells(): List<Cell> {
        val key = CellLayoutKey(width, height, page.ordinal, keysVersion)
        val hit = cachedCells
        if (hit != null && key == cachedKey) return hit
        val built = buildCells(width, height)
        cachedCells = built; cachedKey = key
        return built
    }
```
Then in `onDraw`, replace line 339 with:
```kotlin
        cells = layoutCells()
```
(The emoji early-return at line 337 is unchanged; it sets `cells = emptyList()`.)

- [ ] **Step 7: Build + module tests**

Run: `cd android && ./gradlew :keyboard-view:testDebugUnitTest :app:assembleDebug`
Expected: BUILD SUCCESSFUL; tests pass.

- [ ] **Step 8: On-device correctness sweep**

Install; verify no regressions: typing shows correct keys; language switch (globe→settings→change language→back) re-renders the alpha layout (proves `keysVersion` invalidation works); shift highlights + uppercases; number/symbol/emoji pages render; swipe still types. Expected: all correct.

- [ ] **Step 9: Commit**

```bash
git add android/keyboard-view/
git commit -m "perf(keyboard): memoize buildCells on (size,page,keysVersion) to stop per-frame layout rebuilds"
```

---

### Task 6: Phase-1 measurement + exit gate

**Files:**
- Modify: `docs/superpowers/specs/2026-07-25-performance-optimization-design.md`

**Context:** Prove the gains with the harness on the benchmark build and confirm the CI gate is green.

- [ ] **Step 1: Re-measure on the benchmark build (post-changes)**

Build + install `assembleBenchmark` (now including Tasks 3–5), focus a field, run:
```bash
tools/perf/jank.sh RZCY51D0T1K 5
```
Expected: `janky_pct` materially below the recorded baseline, trending to `< 5%`; `slow_ui` down. Record `total/janky%/p95/p99/slow_ui`.

- [ ] **Step 2: Record before/after in the spec appendix**

Append the debug baseline, benchmark baseline, and post-Phase-1 numbers to the appendix as a table; note APK size and ABI-count reduction.

- [ ] **Step 3: Full gate green**

Run: `cd android && ./gradlew test :app:assembleDebug` (all module unit tests + build) and confirm the Rust gate is untouched (no Rust changed this phase). Expected: all green.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-07-25-performance-optimization-design.md
git commit -m "docs(perf): record Phase 1 before/after measurements"
```

---

## Phase 1 Exit Criteria
- No host-app layout shift on suggestion open/close (Task 3) and on page switch (Task 4) — on-device confirmed.
- `buildCells` no longer rebuilt per frame (Task 5); measured janky% down vs baseline, trending to < 5% on the benchmark build.
- Installable R8 benchmark build runs with FFI intact; APK ships only arm64+armv7.
- All module unit tests green (incl. new `KeyboardGeometryTest`); no feature removed; Rust gate untouched.
- Before/after numbers recorded in the spec appendix.
