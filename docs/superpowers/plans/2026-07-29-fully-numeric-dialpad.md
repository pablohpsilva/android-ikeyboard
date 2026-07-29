# Fully-numeric dialpad — implementation plan

> **For agentic workers:** implement task-by-task, Red → Green → Refactor. Steps use `- [ ]`.

**Goal:** Make the numeric-only dialpad fully numeric by removing the shared
`[ABC][emoji][space][return]` function row (Enter included) for `Page.PHONE`;
the globe/mic bottom bar stays.

**Architecture:** One private `showsFunctionRow` predicate in `KeyboardView`,
consumed by `buildCells` (skip the function-row block) and `onMeasure` (zero its
height). Pure-Kotlin platform-shell; no Rust/FFI/.so/binding change.

**Tech Stack:** Kotlin, Android custom `View`, JUnit (host JVM).

## Global Constraints (from the design)

- Only `Page.PHONE` loses the function row. ALPHA/NUMBERS/SYMBOLS/EMOJI unchanged.
- `Dialpad.ROWS` / digit grid / `drawDialKey` untouched.
- Bottom bar (globe + mic) still renders on the dialpad.
- No dead gap: `onMeasure` height and `buildCells` layout stay in lock-step.
- No Rust/FFI/.so/UniFFI-binding change.
- Design: `docs/superpowers/specs/2026-07-29-fully-numeric-dialpad-design.md`.

---

### Task 1: Dialpad shows no function row

**Files:**
- Modify: `apps/android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt`
  (add `showsFunctionRow` after `private var page` at line 175; guard the
  function-row `run{}` block ~490–512; conditional `funcPx` in `onMeasure` ~360)
- Test: `apps/android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/KeyboardGeometryTest.kt`

**Interfaces:**
- Consumes: `KeyboardGeometry.totalHeightPx(stripReserved, rowPx, funcPx, barPx, insetPx, stripPx, contentRows)` (unchanged signature).
- Produces: no new public API. New private `KeyboardView.showsFunctionRow: Boolean`.

- [ ] **Step 1: Write the failing test** — add to `KeyboardGeometryTest.kt`, the
  dialpad-height contract (4 content rows, no function-row term):

```kotlin
    @Test fun dialpad_has_no_function_row() {
        // Fully-numeric dialpad: 4 content rows, NO shared function row (funcPx = 0).
        val dialpad = KeyboardGeometry.totalHeightPx(
            stripReserved = true, rowPx = 52f, funcPx = 0f, barPx = 46f, insetPx = 10f, stripPx = 42f,
            contentRows = 4,
        )
        assertEquals(42f + 52f * 4 + 46f + 10f, dialpad, 0.001f) // strip + 4 rows + bar + inset, no func row
        // And it is exactly one function-row shorter than a dialpad that still had one:
        val withFuncRow = KeyboardGeometry.totalHeightPx(
            stripReserved = true, rowPx = 52f, funcPx = 54f, barPx = 46f, insetPx = 10f, stripPx = 42f,
            contentRows = 4,
        )
        assertEquals(withFuncRow - 54f, dialpad, 0.001f)
    }
```

- [ ] **Step 2: Run it — expect PASS already for the arithmetic** (the seam is
  pure and the function accepts `funcPx`). Run:
  `./gradlew --no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false :keyboard-view:testDebugUnitTest --tests '*KeyboardGeometryTest'`
  This test documents/locks the dialpad height contract that the wiring must
  produce. (It is a contract-lock, not a red-first behavioural test — the
  behavioural proof for the private view is the on-device acceptance in Step 7,
  as the design's §4 states.)

- [ ] **Step 3: Add the single-source predicate.** After `private var page = Page.ALPHA`
  (line 175), insert:

```kotlin

    // The dialpad (numeric-only fields) is fully numeric: no shared
    // [ABC][emoji][space][return] row. Every other page keeps it.
    private val showsFunctionRow: Boolean get() = page != Page.PHONE
```

- [ ] **Step 4: Zero the function-row height for the dialpad in `onMeasure`.**
  Change the `funcPx` argument (currently `funcPx = funcRowHeight,` at ~line 360):

```kotlin
            funcPx = if (showsFunctionRow) funcRowHeight else 0f,
```

- [ ] **Step 5: Skip the function-row block in `buildCells`.** The unconditional
  `run { … }` block that builds `[ABC][emoji][affix?][space][affix?][return]`
  (opens at ~line 490 with `run {` and the comment
  `// Function row: [123|ABC] [emoji] [affix?] [ space ] [affix?] [ return ].`,
  closes at `top += funcRowHeight` `}` ~line 512). Wrap only that block:

```kotlin
        if (showsFunctionRow) {
            run {
                ... // existing block body, unchanged
                top += funcRowHeight
            }
        }
```

  Do NOT touch the bottom-bar `run { … }` (globe + mic) that follows — it must
  still run for every page including PHONE.

- [ ] **Step 6: Run the full keyboard-view + ime-service suites — expect PASS.**
  `./gradlew --no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false :keyboard-view:testDebugUnitTest :ime-service:testDebugUnitTest`
  Expected: all green, including the new test and existing `DialpadTest`,
  `KeyboardGeometryTest`, `AccentsTest`, `TypingRulesTest`.

- [ ] **Step 7: On-device acceptance (SM-A166B).** Build+install:
  `./gradlew --no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false :app:installDebug`.
  Focus a **phone/number** field and screenshot-verify:
  1. four dial rows render (`1–9`, `. , 0 ⌫`);
  2. **no** `[ABC][emoji][space][return]` row beneath them;
  3. globe + mic sit **directly under** dial row 4 — no dead gap;
  4. focus a normal **text** field → function row still present (regression).

- [ ] **Step 8: Regenerate CODEMAP + freshness gates.**
  `python3 core/tools/codemap.py` then
  `python3 core/tools/codemap.py --check` (expect exit 0). Confirm
  `git status` shows only Kotlin + docs (+ CODEMAP) — no `.so`/binding churn.

- [ ] **Step 9: Commit** (only after the design/plan/build gates are clean and
  the user has approved a commit — per CLAUDE.md §8, commit only when asked).

**Definition of Done:** new test green; `showsFunctionRow` added and both
consumers read it; `:keyboard-view` + `:ime-service` suites green; on-device §7
confirmed; CODEMAP fresh; no Rust/FFI/.so/binding change.

**Rollback:** revert the three edits in `KeyboardView.kt` and the added test —
self-contained, no cross-file coupling.

## Audit log

### Pass 1 — ✅ Complete and verified (plan gate)
Plan implements every design decision: §3.1 (`showsFunctionRow` — Step 3), §3.2
(skip block — Step 5), §3.3 (zero `funcPx` — Step 4). Anchors verified live by grep
this session: `funcPx = funcRowHeight` = exactly one occurrence (line 360, unique
edit target); `private var page = Page.ALPHA` = line 175; two `run{}` blocks (490
function row / 515 bottom bar) — the function-row block is uniquely identified by its
`// Function row:` comment (484) and tail `top += funcRowHeight` (511), which the
bottom-bar block lacks, so the wrap cannot capture the bottom bar. `Page.PHONE` real
(enum line 174); `showsFunctionRow` no collision; `page != Page.PHONE` → Boolean type
matches. Test uses real `totalHeightPx(contentRows=4, funcPx=0f)` seam, arithmetic
(`42+52·4+46+10` and `withFuncRow-54f`) correct against the formula, style matches file.
Honest limitation disclosed in Step 2 (contract-lock not red-first behavioural test;
wiring proof = on-device Step 7). No placeholders, no type/name mismatch, DoD complete.
Advance to build.
