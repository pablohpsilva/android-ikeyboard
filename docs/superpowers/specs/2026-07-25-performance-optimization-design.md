# FeatherKey Performance Optimization — Design

**Status:** Draft (awaiting review)
**Date:** 2026-07-25
**Owner:** (perf initiative)

## Goal

Make FeatherKey feel *feather-light* on entry/mid-range Samsung phones —
eliminate the suggestion-strip layout shift, remove the intermittent
typing/swipe lag, and cut startup/memory weight — **without removing any
feature, breaking any existing test, or lowering coverage.** Every change stays
behind the current test suites and the CI gate; correctness is preserved and the
work is verified with before/after on-device measurement.

## Reference hardware

Galaxy A16 (`SM-A166B`), Exynos `s5e8535`, Android 14 — a genuinely entry-level
target. All budgets and measurements below are taken on this device.

## Baseline (measured, before any change)

Captured via `adb shell dumpsys gfxinfo com.featherkey` on a **debug** build:

- Janky frames: **12.95%** (legacy 16.74%)
- Slow-UI-thread frames: 62; Slow issue-draw-commands: 94; Missed vsync: 27
- Frame time: 50th 12ms, 90th 19ms, 95th 31ms, **99th 125ms**, with outliers to 650ms
- Process PSS: **103 MB** (Graphics 6.6 MB, Code RSS 53 MB, Java heap ~9.8 MB)
- APK: **18.3 MB**, shipping **7 ABIs** (`arm64-v8a, armeabi-v7a, armeabi, mips,
  mips64, x86, x86_64`) — the extra ABIs are JNA's `libjnidispatch.so` dead weight.
- Frequency assets parsed into RAM: **~2.6 MB** text across 6 languages
  (English alone ≈ 48k words), plus a separate lexicon list per language.

## Diagnosis (root causes, with evidence)

### ① Suggestion-strip toggle shifts the host app — CERTAIN
`KeyboardView` draws the 42dp suggestion strip on the *same* canvas as the keys.
`onMeasure` adds `stripHeight` to the reported height **only when suggestions are
non-empty** (`keyboard-view/.../KeyboardView.kt:189`), and the `suggestions`
setter calls `requestLayout()` on every empty↔non-empty flip (`:71`). Because
`InputMethodService` sizes the IME window to the input view's measured height,
the window grows/shrinks by 42dp each time the strip appears/disappears, pushing
the host app up and down.

### ② Intermittent typing/swipe lag — three compounding UI-thread costs
1. **Full key-geometry rebuilt every frame.** `onDraw` calls `buildCells(width,
   height)` unconditionally (`KeyboardView.kt:339`); `buildCells` (`:196-317`) is
   not memoized — it allocates ~40 `RectF` + ~40 `Cell`, **two** `groupBy` maps, a
   `TreeMap`, and per-row `sortedBy` lists **per draw**. It redraws on every
   keystroke, every suggestion change, and **every finger-move during a swipe**
   (`invalidate()` at `:527`). Dominant GC-churn / "Slow issue draw commands ×94".
2. **Suggestions computed synchronously on the UI thread per keystroke.**
   `rankForStrip` (`ime-service/.../FeatherKeyImeService.kt:324`) →
   `Vocabulary.candidatesByLanguage` → `prefixMatches` (`Vocabulary.kt:66`) does a
   binary search then a **linear collect + sort of every dictionary word sharing
   the prefix, per language** — brutal for short prefixes ("s" → hundreds–thousands
   of words) — builds a fresh `FfiRankCandidate` list, and calls into Rust. It then
   **runs a second full pass** when each async device-dictionary result arrives
   (`DeviceDictionary` listener → `updateSuggestions`).
3. **Gesture decode scans the whole vocabulary on the UI thread per swipe.**
   `GestureDecoder.decode` (`GestureDecoder.kt:36`) loops all vocab words; every
   survivor allocates two 24-point `List<PointF>` (`resample`, `normalize`) + a
   `Pair`.
4. **Amplifier — debug build.** The measured baseline is a `debuggable` build:
   ART runs unoptimized and Compose adds debug overhead. Part of the "heaviness"
   is the build type, not the code.

### ③ Weight (size / memory / startup)
- **APK ships 7 ABIs**; only `arm64-v8a` (all modern devices + Apple-silicon
  emulator) and `armeabi-v7a` (old 32-bit) are needed. `mips/mips64/x86` are dead.
- **Memory duplication.** Each active language's words are resident ~3× in the
  Kotlin heap (`Vocabulary` sorted `Array<String>` + `HashMap` keys + the combined
  `words` list, `Vocabulary.kt:25,99,101`) *plus* the native core's own copy *plus*
  the separate lexicon list. ~150k+ word strings resident with a few languages on.
- **Startup on the main thread.** `onCreate` synchronously runs `Lexicons.load`
  (~12k-line parse per active language), `FeatherKeyBridge.open` (JNA native load +
  per-symbol checksum loop + redb open + native lexicon ingest), and
  `KeystoreKeyProvider.provisionDataKey` — whose **first-run StrongBox keygen can
  take hundreds of ms–seconds**. The large 294k-word freq corpus is *already*
  loaded off-thread (good; the pattern to copy).

### What is already good (do not "fix")
Persistence is debounced (3s) and fully off-thread (redb + AES-GCM + TSV on
`Dispatchers.IO`); the device dictionary is async/non-blocking; paints, paths,
matrices, and vector icons are cached (no per-draw allocation there); no per-draw
text measurement; module/crate boundaries are clean. **This is optimization, not
rework.**

## Measurement methodology & perf budget

Gains must be provable and regressions must fail a check (project fitness-function
culture).

- **On-device harness** (`tools/perf/jank.sh <serial>`): reset `gfxinfo`, drive a
  fixed scripted input sequence (a set of `input tap` keystrokes + a swipe over
  known key coordinates), dump `gfxinfo`, parse and print **janky %, 95th/99th
  frame time, missed-vsync, slow-UI-thread**. Exit non-zero if janky % exceeds the
  budget. Runs manually/locally against a connected device (no device in CI).
- **Budget (on the reference A16, optimized build):** janky frames **< 5%**, 99th
  percentile frame time **< 32ms** (2 vsync @ 60Hz) on the typing sequence. These
  are the Phase-exit targets, tightened as phases land.
- **Pure-logic regression guards (CI, JVM):** extract hot-path decisions into pure,
  Context-free functions (the project's established pattern — cf. `SessionPlan`,
  `DefaultImeStatus`) and unit-test them: keyboard height/layout computation,
  `buildCells` memo-key equality & determinism, prefix-scan bounding. These run in
  the normal gate.
- Record baseline and each phase's before/after numbers in the plan's progress
  ledger and this spec's appendix.

## Phase plan (sequenced, measured between)

The user chose **phased with measurement between** and **an installable optimized
build**. Phase 1 establishes the measurement baseline and takes the surgical,
low-risk, high-impact wins; Phases 2–3 are scoped here and detailed after Phase 1
numbers land.

### Phase 1 — Foundation + quick wins (surgical, low risk)

**1A. Installable optimized build + ABI trim (measure the real target).**
- Add a `benchmark` build type (`initWith release`, `isMinifyEnabled = true`,
  `isDebuggable = false`, signed with the debug keystore so it installs) so we can
  measure and ship an R8-optimized build without provisioning a release key yet.
  (A real release signing key is a follow-up when publishing.)
- `defaultConfig.ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a") }` — drops
  the dead `mips/mips64/x86/x86_64` native libs (incl. JNA's `jnidispatch`).
  Verify the arm64 emulator and the A16 still install/run.
- Verify R8 keep rules cover UniFFI/JNA reflective access and Compose (add
  `proguard-rules.pro` entries as needed) so minification doesn't break FFI.
- **Deliverable:** installable `benchmark` APK; re-baseline all metrics on it.

**1B. Reserve the suggestion-strip band (kills ① layout shift).**
- The reported symptom is the **suggestion box open/close** within the typing
  pages. Firm fix: on the strip-bearing pages (ALPHA / NUMBERS / SYMBOLS), the
  strip band is **always reserved** — `onMeasure` reports
  `stripHeight + rowHeight*3 + funcRowHeight + bottomBarHeight + bottomInset`
  regardless of `suggestions.isEmpty()`, `buildCells` offsets keys by `stripHeight`
  unconditionally, and the `suggestions` setter drops the toggle `requestLayout()`
  (keeps `invalidate()`). That alone stops the reported shift.
- **Emoji-page sub-decision (verified caveat):** the emoji page is a special case —
  `buildCells` returns `emptyList()` for it (`KeyboardView.kt:199`) because it
  draws/hit-tests its own grid, and `onMeasure` currently makes it *shorter* by
  `stripHeight` (`:189`). To make the keyboard height constant across **every**
  page (so alpha↔emoji also doesn't shift), the emoji page must adopt the same
  total height and its self-drawn grid must lay out within the taller envelope —
  a real (small) emoji-layout change, not merely "don't draw the strip." Recommend
  including it for a rock-steady height, but it is separable from the reported fix;
  the plan will treat it as its own step.
- **Tests:** unit-test the pure height function (identical for empty vs non-empty
  suggestions on strip-bearing pages; and, if the emoji step is included, identical
  across all pages). On-device: confirm no host-app shift on suggestion toggle
  (firm) and on page switches (with the emoji step).

**1C. Memoize `buildCells` (kills ② per-frame rebuild — biggest jank win).**
- Cache the built `List<Cell>` keyed on `(width, height, page, shifted)` (strip is
  now always reserved, so it drops out of the key). Rebuild only when the key
  changes; reuse on pure redraws (suggestion-text change, press highlight, trail).
- Pressed-key highlight and gesture trail are draw-time (color/overlay) and must
  not trigger a rebuild.
- **Tests:** pure-function unit tests for cell-build determinism and memo-key
  equality; a guard that a repeated draw with an unchanged key does not rebuild.
- **Measure:** expect the largest janky-% and swipe-smoothness improvement here.

**Phase 1 exit:** on the benchmark build, janky % materially down (target < 8%
after 1B/1C, trending to the < 5% budget), no layout shift, all tests green, gate
green.

### Phase 2 — Move the hot path off the UI thread (measured next)
- Compute `rankForStrip` on a background dispatcher with **cancellation +
  latest-wins** (a new keystroke cancels the in-flight computation; only the newest
  result is posted to the strip), so keystrokes never block on suggestion compute.
- **Debounce/coalesce** the device-dictionary re-run so a keystroke doesn't run the
  full pass twice.
- **Bound** short-prefix scans in `prefixMatches` (cap the collected range before
  sorting; the strip only shows a handful).
- Run `GestureDecoder.decode` off-thread with reused sample buffers.
- Reliability focus: strict ordering/staleness handling so async results never
  overwrite newer input; preserve all existing correctness tests.

### Phase 3 — Startup + memory (measured last)
- Async init/"warming" for `Lexicons.load`, `FeatherKeyBridge.open`, and keystore
  provisioning (mirror the existing off-thread `loadVocab`), with a warming state
  so the first frame isn't blocked; keep StrongBox keygen off the first frame.
- Deduplicate the vocabulary structures (unify the freq map / sorted array /
  combined `words` list; reduce Kotlin↔native duplication) to cut resident memory.

### Deferred / optional (not in this initiative unless measurement demands)
- Replace JNA with a direct-JNI UniFFI backend (large; UniFFI stable is JNA-only).
- Cache `chooseCorrection`'s per-call `LocaleManager`/pack clone
  (`correct.rs:37`) — per-word, not per-keystroke, so lower priority.

## Non-goals
- No feature removal, no behavior regressions, no test deletions/weakening.
- No dependency on a network at runtime (unchanged invariant).
- No new device requirement in CI (perf budget is a local/manual gate; pure-logic
  guards run in CI).

## Success criteria
- No host-app layout shift on suggestion open/close (on-device confirmed).
- Janky frames < 5% and 99th-percentile frame time < 32ms on the reference A16
  (optimized build, typing sequence).
- APK ships only `arm64-v8a` + `armeabi-v7a`; measurable size drop.
- Lower startup-to-first-frame and lower resident memory (before/after recorded).
- Full test suite + CI gate green throughout; no feature lost.

## Appendix — measurement log
- Baseline (debug, A16): janky 12.95%, 99th 125ms, PSS 103 MB, APK 18.3 MB, 7 ABIs.
- (Phase 1 benchmark-build re-baseline: TBD)
- (Phase 1 after 1B/1C: TBD)
