# Keyboard Interaction Fixes — Design

**Date:** 2026-08-03
**Slug:** keyboard-interaction-fixes
**Status:** Design (awaiting gate + user review)

## Problem

Two independent defects/UX gaps in the Android keyboard shell, both confined to
`apps/android/keyboard-view` (the custom `KeyboardView`). The Rust core is not
involved: no FFI surface changes, no bindings regeneration.

1. **Two-finger taps are misread as swipes (BR-41 defect).** Pressing two keys
   almost simultaneously makes the keyboard fire a bogus swipe: a swipe-decoded
   word is committed (and its trail line is visibly drawn), corrupting the
   message. BR-41 requires swipe typing "without it conflicting with quick
   single-key taps" — this is a direct violation.

2. **Settings + voice icons live in a dedicated bottom bar (BR-42/BR-43
   presentation).** The globe (opens Settings) and mic (voice) occupy a 46 dp
   bottom bar of their own. The user wants them relocated into the suggestion
   strip — `[globe] [sugg1] [sugg2] [sugg3] [mic]` — and the bottom bar removed,
   making the keyboard ~46 dp shorter.

## Requirements closed

| BR | How this design serves it |
|---|---|
| **BR-41** | Swipe typing must not conflict with quick single-key taps. Fix 1 makes two near-simultaneous taps register as two letters, never a swipe. |
| **BR-42** | Inline predictive strip. Fix 2 keeps the three suggestion slots; only their horizontal extent changes (they share the strip band with the two icons). Behavior unchanged. |
| **BR-43** | Privacy-preserving voice. Fix 2 relocates the mic entry point into the strip; the voice action (`startVoiceInput`) is unchanged. |

## Modules involved (all pre-existing — see CODEMAP)

| Module | Exists? | Role in this change |
|---|---|---|
| `apps/android/keyboard-view` → `KeyboardView.kt` | yes | Both fixes live here: `onTouchEvent` (Fix 1), `buildCells`/draw (Fix 2). |
| `apps/android/keyboard-view` → `KeyboardGeometry.kt` | yes | Total-height / content-top math; drop the bottom-bar band (Fix 2). |
| `apps/android/keyboard-view` → `GestureGeometry.kt` | yes | Pattern to mirror for a new **pure** strip-layout helper. |
| `apps/android/ime-service` → `FeatherKeyImeService.kt` | yes | `onFunctionKey(GLOBE/MIC)` and `onSuggestion` wiring — **unchanged**; the relocated cells reuse the existing `Sp.GLOBE`/`Sp.MIC` dispatch. |

No new crate, no new module. The one net-new unit is a pure geometry helper
function for the strip sub-rects, added alongside the existing `KeyboardView`
geometry (host-testable like `GestureGeometry`).

## Fix 1 — Pointer-locked gesture tracking

### Root cause (confirmed by reading `KeyboardView.onTouchEvent`)

The touch handler tracks only the primary pointer. `when (event.actionMasked)`
handles `ACTION_DOWN`, `ACTION_MOVE`, `ACTION_UP`, `ACTION_CANCEL` only;
`ACTION_POINTER_DOWN` (5) and `ACTION_POINTER_UP` (6) fall through to
`super.onTouchEvent`. `event.x`/`event.y` read pointer index 0. Sequence:

1. Finger 1 → `ACTION_DOWN`: `gestureCell = key A`, `trail = [A]`.
2. Finger 2 → `ACTION_POINTER_DOWN`: ignored.
3. Finger 1 lifts → `ACTION_POINTER_UP`: ignored; finger 2 re-indexes to 0.
4. Finger 2 → `ACTION_MOVE`: `event.x/y` now reads finger 2 (key B), far from
   trail's last point → `trailLen += hypot(B−A)` exceeds the 26 dp threshold →
   `gesturing = true`, trail line drawn.
5. Finger 2 → `ACTION_UP`: `gesturing && trail.size >= 3` → fires a garbage
   swipe path → decodes to a wrong word.

### Change

Introduce `gesturePointerId: Int` (init `MotionEvent.INVALID_POINTER_ID`), the
id of the finger that owns the current letter press.

- **`ACTION_DOWN`** (letter press branch): `gesturePointerId = event.getPointerId(0)`.
- **`ACTION_MOVE`**: resolve `idx = event.findPointerIndex(gesturePointerId)`;
  if `idx < 0` return (owner not in this event); else read `event.getX(idx)/getY(idx)`.
  No other pointer contributes to the trail.
- **`ACTION_POINTER_DOWN`**: a second finger arrived.
  - If a letter is pending and **not** yet gesturing (`gestureCell != null && !gesturing`):
    commit the pending letter as a tap now (fire at its down point), then hand
    ownership to the new finger — `gesturePointerId = event.getPointerId(event.actionIndex)`,
    reset `trail`/`trailLen` to the new finger's point, `gesturing = false`,
    re-resolve `gestureCell` to the new finger's key, cancel the long-press timer.
  - If already gesturing (a genuine one-finger glide is under way): ignore the
    extra pointer (a swipe cannot be two-fingered).
  - If the current press is not a letter (`gestureCell == null`): ignore.
- **`ACTION_POINTER_UP`**: if the lifting pointer id == `gesturePointerId`,
  finalize exactly as `ACTION_UP` does (commit swipe if `gesturing && size>=3`,
  else tap), then `resetGesture()`.
- **`ACTION_UP`/`ACTION_CANCEL`**: existing behavior; also clear `gesturePointerId`
  via `resetGesture()`.

`resetGesture()` gains `gesturePointerId = INVALID_POINTER_ID`.

### Outcome

Two near-simultaneous taps → both commit as letters, no trail drawn, no swipe.
A single-finger glide is byte-for-byte unchanged (one pointer throughout).

### Testability

The pointer bookkeeping is inseparable from `MotionEvent` (no plain-JUnit
harness for it in this repo; swipe lifecycle is device-verified by convention —
see the `proper-noun-capitalization` memory). Verification is a scripted BDD
scenario (Gherkin, `@BR-41`) describing the observable behavior, plus an
on-device acceptance pass. No pure logic is extracted for Fix 1 because none
exists independent of the event stream.

## Fix 2 — Strip icons; drop the bottom bar

### Change (layout)

Current strip build (`buildCells`): three equal cells
`Cell.Suggest(i*cw .. (i+1)*cw, i)` across the full width; the globe/mic are a
separate bottom-bar block (`KeyboardView.kt:519-524`) of height `bottomBarHeight`
(46 dp).

New strip build:
- `iconW = band` (square icon zone, band = strip height).
- Left: `Cell.Special(RectF(0, 0, iconW, band), Sp.GLOBE)`.
- Right: `Cell.Special(RectF(w - iconW, 0, w, band), Sp.MIC)`.
- Middle: three `Cell.Suggest` cells dividing `[iconW, w - iconW]` into thirds.
- **Delete** the bottom-bar block and drop **only** the `barPx` term from
  `KeyboardGeometry.totalHeightPx` (the ~46 dp reclaimed → shorter keyboard).

### Bottom inset is preserved (audit finding)

`totalHeightPx = strip + rowPx*rows + funcPx + barPx + insetPx` keeps `barPx`
(bottom bar) and `insetPx` (system nav-bar reservation) as **separate additive
terms**, and `buildCells` lays cells out top-down. So removing the bottom bar
drops `barPx` alone: the function row (space/return) becomes the lowest drawn
content and still sits above the reserved `insetPx` region where the OS draws
its IME nav buttons (hide-keyboard + IME switcher). The implementer must set
`barPx = 0` (or remove the parameter) but **must not** touch `insetPx`, or the
space bar would be drawn under the system nav buttons. IME switching remains the
OS button in that inset region — unaffected by moving FeatherKey's own Settings
(globe) up into the strip.

The relocated cells reuse `Sp.GLOBE`/`Sp.MIC`: their `drawGlobe`/`drawMic`
dispatch and `onFunctionKey(GLOBE/MIC)` wiring are untouched (globe → Settings,
mic → voice). Globe glyph kept (user's choice). Because they are `Cell.Special`,
they fire immediately on `ACTION_DOWN` — never swipe-tracked.

### Pure helper (TDD surface)

A pure function `KeyboardGeometry.stripSubRects(width, band, iconW)` returns
`(settingsRect, [sugg0, sugg1, sugg2], voiceRect)` as plain float 4-tuples
(PointF-free, plain-JUnit-testable). It lives **in `KeyboardGeometry`** — the
object that already owns keyboard geometry (`contentTopPx`, `totalHeightPx`) — not
a new object, so responsibility stays in one place (CODEMAP shows no existing
strip-layout helper; `TypingRules.SuggestionStrip` is about suggestion *content*,
not rects). `buildCells` calls it. The `totalHeightPx` change (drop `barPx`) is
likewise pure and host-testable. Both get failing tests **first**.

### Edge cases

- **Emoji page:** has no suggestion strip (`stripReserved = page != Page.EMOJI`)
  and previously still showed the bottom bar. The emoji page must retain its own
  return-to-letters control (ABC) so removing the bar strands nothing. Verified
  during build; if the emoji page relied on the bar for globe/mic, that is
  out-of-scope reachable via ABC → letter strip.
- **No suggestions:** the strip still renders both icons; the three middle cells
  are simply empty (as today). Icons are always present on standard pages.
- **Narrow screens:** `iconW = band` (~42 dp) on each side leaves the middle
  thirds narrower than before; acceptable — suggestions were already ellipsized.

## Alternatives rejected

- **Fix 1 via a bigger swipe threshold / time gate:** treats the symptom
  (raise 26 dp so the jump is tolerated) — brittle, and a legitimately fast
  short swipe would break. Pointer-locking fixes the root cause.
- **Fix 1 "ignore the second finger entirely":** simpler, but drops the second
  near-simultaneous key. User wants both letters typed. Rejected.
- **Fix 2 keep the bottom bar, just add icons to the strip too:** duplicates the
  controls and wastes the 46 dp the user explicitly wants reclaimed. Rejected.

## Non-goals

- No Rust core / FFI / bindings changes.
- No change to swipe decoding, suggestion ranking, voice, or settings behavior —
  only touch-ownership (Fix 1) and cell placement/height (Fix 2).

## Audit log

### Pass 1 — 🚧 Incomplete → resolved in this pass
Gaps found:
1. **DRY / placement:** the strip-rect helper was specified as a free-floating
   pure function without saying where it lives. CODEMAP queried
   (`GestureGeometry.shiftCenters`, `KeyboardGeometry.{contentTopPx,totalHeightPx}`,
   `TypingRules.SuggestionStrip` = content not layout) — no existing strip-layout
   helper, so it is genuinely new, but it belongs *in* `KeyboardGeometry`.
2. **Missing edge case (nav-bar inset):** removing the bottom bar risked drawing
   the space/return row under the OS IME nav buttons. Traced `totalHeightPx`:
   `barPx` and `insetPx` are separate additive terms, so dropping `barPx` alone
   is safe and the inset reservation must be preserved.

Changed:
- §"Pure helper" now places `stripSubRects` in `KeyboardGeometry` with the
  CODEMAP evidence.
- New §"Bottom inset is preserved" documents the `barPx`-only removal and the
  `insetPx` constraint for the implementer.

### Pass 2 — ✅ Complete and verified (design phase)
Evidence:
- **Requirements mapped:** BR-41 (Fix 1, direct defect), BR-42/BR-43 (Fix 2,
  presentation only) — table in §"Requirements closed".
- **Existing code named (§2/CODEMAP):** all four touched modules marked
  pre-existing; no new crate/module; the one new symbol placed in an existing
  object with a DRY check.
- **Root cause proven, not guessed:** Fix 1 traced through the actual
  `onTouchEvent` `actionMasked` handling (`ACTION_POINTER_DOWN/UP` unhandled,
  `event.x/y` = pointer 0).
- **Edge cases enumerated:** emoji page, no-suggestions, narrow screens,
  nav-bar inset.
- **Alternatives recorded** with rejection reasons.
- Design is internally consistent; no TBD/placeholder remains.
Not verified (correctly deferred to build): actual on-device behavior and the
pure-helper tests — those are the build phase's gate, not the design's.

**Verdict: ✅ Complete and verified (design).**

### Pass 3 — Build phase (⚠️ host-verified; device acceptance pending user)
What was required (both fixes) mapped to evidence:
- **Fix 2 pure geometry (BR-42/43 layout):** DONE — `KeyboardGeometry.stripSubRects`
  + `Rect4`/`StripRects` added; 2 new host tests + 4 updated `totalHeightPx`
  tests. Seen RED first (`No value passed for parameter 'barPx'`,
  `Unresolved reference 'stripSubRects'/'Rect4'`), then GREEN.
- **Fix 2 wiring:** DONE — `buildCells` strip now `[globe][3 sugg][mic]`;
  bottom-bar block + `bottomBarHeight` deleted; `totalHeightPx` call drops
  `barPx` (insetPx preserved); class KDoc updated.
- **Fix 1 pointer lock (BR-41):** DONE (code) — `gesturePointerId` state,
  `ACTION_MOVE` reads only the owner via `findPointerIndex`,
  `ACTION_POINTER_DOWN` commits-first-then-hands-over, `ACTION_POINTER_UP`
  finalizes via extracted `finalizeGestureOrTap`, `resetGesture` clears the id.

Verification run (evidence):
- `./gradlew :keyboard-view:testDebugUnitTest` — BUILD SUCCESSFUL (RED→GREEN cycle
  observed for the geometry tests).
- `:app:assembleRelease` — BUILD SUCCESSFUL.
- `bash core/tools/ci-local.sh` — ALL GATES PASSED (codemap fresh, bindings
  byte-identical — no FFI touched, Rust unchanged).
- `adb install -r app-release.apk` — Success (release-signed, installed over the
  existing app with **no uninstall/data wipe**; device `RZCY51D0T1K`).

Not yet verified (correctly handed to the user):
- **Fix 1 device acceptance** — two-finger near-simultaneous taps type two
  letters with no trail/no swipe word. adb cannot reliably inject simultaneous
  multi-touch, so this is real-finger user acceptance.
- **Fix 2 device acceptance** — strip shows `[globe] suggestions [mic]`, bottom
  bar gone, keyboard visibly shorter, globe→Settings, mic→voice, space/return not
  clipped by the system nav buttons, emoji page still returns via ABC.

**Verdict: ⚠️ Done, host-verified; on-device behavioural acceptance pending user.**
