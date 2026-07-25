# Luxembourgish (`lb`) Keyboard Support — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Luxembourgish as a first-class keyboard language — QWERTZ layout, prediction data, automatic de+fr+en code-switching, and long-press accent input for its diacritics.

**Architecture:** The Rust core is already multi-language (opaque tag strings + per-word detection + language momentum), so it needs only a 1-line layout-map addition. The rest is Android shell work: a companion-language auto-activation rule, a new long-press accent-popup subsystem in the keyboard view, two committed data files, and IME/metadata wiring. Tricky logic is extracted into pure, unit-tested Kotlin helpers; Canvas/MotionEvent/SharedPreferences plumbing is verified by module compile + on-device.

**Tech Stack:** Rust (workspace crates, `cargo test`), Kotlin/Android (Jetpack Compose settings; custom `View` keyboard; `InputMethodService`), `junit:junit:4.13.2` for pure-JVM unit tests (no Robolectric — no `Context`/`View`/`Canvas` in tests), UniFFI bridge, cargo-ndk for the native `.so`.

## Global Constraints

- **Licensing (Option A):** vocabulary derived from `spellchecker-lu/dictionary-lb-lu` `unmunched.dic` (**EUPL v1.1**); frequency order from Leipzig Wortschatz Luxembourgish (**CC-BY 3.0**). A `NOTICES` file must credit both. Repo code license is `Apache-2.0 OR MIT`; the `lb` data is a separately-licensed asset.
- **Lexicon file contract:** `assets/lexicons/*.txt` are one word per line, **all-lowercase**, **`LC_ALL=C`-sorted** (pure byte order). The FST loader (`crates/dictionary/src/lib.rs:82`) throws `DictionaryError::Unsorted` on any backward step. Keep diacritic forms verbatim (precedent: `lexicons/de.txt` retains umlauts).
- **Frequency file contract:** `assets/freq/*.txt` are one word per line, most-common-first, no counts.
- **`lb.txt` size:** cap ~12k entries to match the other lexicons (decided default).
- **Authored-not-compiled shell:** the Kotlin side is not device-tested in CI; every task touching Android-framework code ends with an on-device verification step, and the whole feature ends with a device install.
- **Commit trailers (mandatory on every commit):**
  ```
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01CzDG6D2Jhq4hK7YUbWmVWD
  ```
- **Primary language = first tag** in `LanguagePrefs` order (`LanguagePrefs.kt:19`); `lb` must stay first so its QWERTZ layout and momentum head-start win.
- Do **not** touch the stale `cyclePrimary` path or its comment (unrelated scope).

---

### Task 1: Luxembourgish QWERTZ layout (Rust)

**Files:**
- Modify: `crates/layout-engine/src/scripts.rs:70` (the `alpha_for` match) and its test at `:137`.

**Interfaces:**
- Consumes: existing `Layout::qwertz()` (`scripts.rs:52`), `Layout::alpha_for(tag: &str) -> Layout` (`scripts.rs:65`).
- Produces: `Layout::alpha_for("lb")` and `alpha_for("lb-LU")` return the QWERTZ block. No signature change.

- [ ] **Step 1: Add the failing assertions to the existing layout test**

In `crates/layout-engine/src/scripts.rs`, inside `fn alpha_for_selects_by_primary_subtag()` (`:137`), add after the `de_DE` assertion (`:146`):

```rust
        // Luxembourgish shares the German QWERTZ block (Swiss national standard).
        assert_eq!(chars(&Layout::alpha_for("lb"))[0], 'q');
        assert_eq!(chars(&Layout::alpha_for("lb"))[5], 'z');
        assert_eq!(chars(&Layout::alpha_for("lb-LU"))[5], 'z');
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p featherkey-layout-engine alpha_for_selects_by_primary_subtag`
Expected: FAIL — `lb` currently falls through the `_ => Layout::qwerty()` arm, so `chars(...)[5]` is `'t'` (qwerty), not `'z'`.

- [ ] **Step 3: Add `lb` to the QWERTZ arm**

In `crates/layout-engine/src/scripts.rs:70`, change:

```rust
            "de" => Layout::qwertz(),
```
to:
```rust
            "de" | "lb" => Layout::qwertz(),
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p featherkey-layout-engine`
Expected: PASS — all layout tests green (was 21 passing; still green with the new assertions).

- [ ] **Step 5: Commit**

```bash
git add crates/layout-engine/src/scripts.rs
git commit -m "feat(lb): route Luxembourgish to the QWERTZ alpha layout

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01CzDG6D2Jhq4hK7YUbWmVWD"
```

---

### Task 2: Register Luxembourgish in the language catalog

Makes `lb` selectable in Settings (layout-only until Task 8 ships the lexicon; `hasLexicon` auto-flips then). `displayName` is a pure function (no `Context`), so it is unit-testable.

**Files:**
- Modify: `android/platform-services/src/main/kotlin/com/featherkey/platform/LanguageCatalog.kt:21-33`
- Test: `android/platform-services/src/test/kotlin/com/featherkey/platform/LanguageCatalogTest.kt` (create)

**Interfaces:**
- Consumes: `LanguageCatalog.displayName(tag: String): String` (`LanguageCatalog.kt:41`).
- Produces: `LanguageCatalog.displayName("lb") == "Lëtzebuergesch"`; `all(context)` includes an `lb` entry.

- [ ] **Step 1: Write the failing test**

Create `android/platform-services/src/test/kotlin/com/featherkey/platform/LanguageCatalogTest.kt`:

```kotlin
package com.featherkey.platform

import org.junit.Assert.assertEquals
import org.junit.Test

class LanguageCatalogTest {
    @Test fun luxembourgish_has_a_native_display_name() {
        assertEquals("Lëtzebuergesch", LanguageCatalog.displayName("lb"))
    }

    @Test fun unknown_tag_falls_back_to_the_tag() {
        assertEquals("xx", LanguageCatalog.displayName("xx"))
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `./gradlew :platform-services:testDebugUnitTest --tests "com.featherkey.platform.LanguageCatalogTest"`
Expected: FAIL — `luxembourgish_has_a_native_display_name` gets `"lb"` (fallback), not `"Lëtzebuergesch"`.

- [ ] **Step 3: Add the catalog entry**

In `android/platform-services/src/main/kotlin/com/featherkey/platform/LanguageCatalog.kt`, add to the `KNOWN` list after the `"el" to "Ελληνικά",` line (`:32`):

```kotlin
        // Luxembourgish: QWERTZ layout (shared with German) + a bundled lexicon.
        // hasLexicon flips true automatically once assets/lexicons/lb.txt ships.
        "lb" to "Lëtzebuergesch",
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `./gradlew :platform-services:testDebugUnitTest --tests "com.featherkey.platform.LanguageCatalogTest"`
Expected: PASS — both tests green.

- [ ] **Step 5: Commit**

```bash
git add android/platform-services/src/main/kotlin/com/featherkey/platform/LanguageCatalog.kt android/platform-services/src/test/kotlin/com/featherkey/platform/LanguageCatalogTest.kt
git commit -m "feat(lb): register Luxembourgish in the language catalog

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01CzDG6D2Jhq4hK7YUbWmVWD"
```

---

### Task 3: Companion-bundle decision logic (pure, tested)

Extract the "when `lb` is newly added, also activate de+fr+en (once)" rule into a pure function with no Android dependencies, so it is fully unit-testable. Task 4 wires it into `LanguagePrefs`.

**Files:**
- Create: `android/platform-services/src/main/kotlin/com/featherkey/platform/LanguageBundle.kt`
- Test: `android/platform-services/src/test/kotlin/com/featherkey/platform/LanguageBundleTest.kt`

**Interfaces:**
- Produces: `LanguageBundle.withCompanions(current: List<String>, requested: List<String>, alreadyApplied: Boolean): LanguageBundle.Result` where `Result(val tags: List<String>, val bundleApplied: Boolean)`. Used by Task 4.

- [ ] **Step 1: Write the failing tests**

Create `android/platform-services/src/test/kotlin/com/featherkey/platform/LanguageBundleTest.kt`:

```kotlin
package com.featherkey.platform

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LanguageBundleTest {

    @Test fun adding_lb_first_time_appends_missing_companions_lb_stays_first() {
        val r = LanguageBundle.withCompanions(
            current = listOf("en"),
            requested = listOf("lb", "en"),
            alreadyApplied = false,
        )
        assertEquals(listOf("lb", "en", "de", "fr"), r.tags)
        assertTrue(r.bundleApplied)
    }

    @Test fun does_not_duplicate_already_active_companions() {
        val r = LanguageBundle.withCompanions(
            current = listOf("fr"),
            requested = listOf("lb", "fr"),
            alreadyApplied = false,
        )
        assertEquals(listOf("lb", "fr", "de", "en"), r.tags)
        assertTrue(r.bundleApplied)
    }

    @Test fun already_applied_is_a_noop() {
        val r = LanguageBundle.withCompanions(
            current = listOf("lb", "de", "fr", "en"),
            requested = listOf("lb"),
            alreadyApplied = true,
        )
        assertEquals(listOf("lb"), r.tags)
        assertTrue(r.bundleApplied)
    }

    @Test fun lb_already_active_does_not_retrigger() {
        // A reorder/rotate where lb is in both current and requested must not fire.
        val r = LanguageBundle.withCompanions(
            current = listOf("lb", "de"),
            requested = listOf("de", "lb"),
            alreadyApplied = false,
        )
        assertEquals(listOf("de", "lb"), r.tags)
        assertFalse(r.bundleApplied)
    }

    @Test fun no_lb_requested_is_a_noop() {
        val r = LanguageBundle.withCompanions(
            current = listOf("en"),
            requested = listOf("en", "pt"),
            alreadyApplied = false,
        )
        assertEquals(listOf("en", "pt"), r.tags)
        assertFalse(r.bundleApplied)
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `./gradlew :platform-services:testDebugUnitTest --tests "com.featherkey.platform.LanguageBundleTest"`
Expected: FAIL — `LanguageBundle` does not exist (compile error / unresolved reference).

- [ ] **Step 3: Write the minimal implementation**

Create `android/platform-services/src/main/kotlin/com/featherkey/platform/LanguageBundle.kt`:

```kotlin
package com.featherkey.platform

/*
 * Luxembourgish is written by mixing German, French and English words. The Rust
 * core already code-switches between active languages, so when the user first
 * adds Luxembourgish we silently activate those companions too — once. This is
 * the pure decision (no Android dependencies); LanguagePrefs.setActiveTags wires
 * it in and persists the one-shot flag.
 */
object LanguageBundle {

    const val LB = "lb"

    /** Appended (in this order) when lb is first activated, if not already active. */
    val COMPANIONS = listOf("de", "fr", "en")

    data class Result(val tags: List<String>, val bundleApplied: Boolean)

    /**
     * If [requested] newly introduces `lb` (it is not in [current]) and the
     * bundle has not been applied before ([alreadyApplied] false), append any
     * missing [COMPANIONS] while keeping the requested order (so lb stays first).
     * Otherwise return [requested] unchanged. Never re-applies once applied.
     */
    fun withCompanions(
        current: List<String>,
        requested: List<String>,
        alreadyApplied: Boolean,
    ): Result {
        val lbNewlyAdded = requested.contains(LB) && !current.contains(LB)
        if (alreadyApplied || !lbNewlyAdded) return Result(requested, alreadyApplied)
        val merged = requested.toMutableList()
        for (c in COMPANIONS) if (!merged.contains(c)) merged.add(c)
        return Result(merged, true)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `./gradlew :platform-services:testDebugUnitTest --tests "com.featherkey.platform.LanguageBundleTest"`
Expected: PASS — all five tests green.

- [ ] **Step 5: Commit**

```bash
git add android/platform-services/src/main/kotlin/com/featherkey/platform/LanguageBundle.kt android/platform-services/src/test/kotlin/com/featherkey/platform/LanguageBundleTest.kt
git commit -m "feat(lb): pure companion-bundle activation logic (de+fr+en)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01CzDG6D2Jhq4hK7YUbWmVWD"
```

---

### Task 4: Wire companion bundle into `LanguagePrefs`

Consume Task 3's tested logic inside `setActiveTags`, persisting the one-shot flag. `LanguagePrefs` touches `SharedPreferences` (Android framework), so no unit test — verified on-device in Task 10; correctness of the decision is already covered by Task 3.

**Files:**
- Modify: `android/platform-services/src/main/kotlin/com/featherkey/platform/LanguagePrefs.kt:30-33` and the companion object `:44-48`.

**Interfaces:**
- Consumes: `LanguageBundle.withCompanions(...)` (Task 3).
- Produces: no signature change — `setActiveTags` still `(List<String>) -> Unit`, but now may expand the set and set the `lb_bundle_applied` flag.

- [ ] **Step 1: Replace `setActiveTags` with the bundle-aware version**

In `android/platform-services/src/main/kotlin/com/featherkey/platform/LanguagePrefs.kt`, replace `setActiveTags` (`:30-33`):

```kotlin
    /** Replace the active set (order preserved; never empty). */
    fun setActiveTags(tags: List<String>) {
        val clean = tags.distinct().filter { it.isNotEmpty() }.ifEmpty { DEFAULT }
        prefs.edit().putString(KEY, clean.joinToString(",")).apply()
    }
```

with:

```kotlin
    /**
     * Replace the active set (order preserved; never empty). The first time
     * Luxembourgish is added, silently also activate its German/French/English
     * companions (see [LanguageBundle]) — once, tracked by a one-shot flag, so a
     * user who later removes a companion is not fought.
     */
    fun setActiveTags(tags: List<String>) {
        val requested = tags.distinct().filter { it.isNotEmpty() }.ifEmpty { DEFAULT }
        val result = LanguageBundle.withCompanions(
            current = activeTags(),
            requested = requested,
            alreadyApplied = prefs.getBoolean(KEY_BUNDLE_APPLIED, false),
        )
        prefs.edit()
            .putString(KEY, result.tags.joinToString(","))
            .putBoolean(KEY_BUNDLE_APPLIED, result.bundleApplied)
            .apply()
    }
```

- [ ] **Step 2: Add the flag key to the companion object**

In the same file, in `private companion object` (`:44-48`), add after `const val KEY = "active_tags"`:

```kotlin
        const val KEY_BUNDLE_APPLIED = "lb_bundle_applied"
```

- [ ] **Step 3: Compile the module to verify it builds**

Run: `./gradlew :platform-services:compileDebugKotlin`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 4: Re-run the platform-services unit tests (guard against regressions)**

Run: `./gradlew :platform-services:testDebugUnitTest`
Expected: PASS — `LanguageCatalogTest` + `LanguageBundleTest` green.

- [ ] **Step 5: Commit**

```bash
git add android/platform-services/src/main/kotlin/com/featherkey/platform/LanguagePrefs.kt
git commit -m "feat(lb): auto-activate de+fr+en when Luxembourgish is first added

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01CzDG6D2Jhq4hK7YUbWmVWD"
```

---

### Task 5: Accent map + hit-test helpers (pure, tested)

The character→variants map and the "which popup cell is the finger over" math, extracted as a pure object so the tricky parts are unit-tested. Task 6 renders/drives it.

**Files:**
- Create: `android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/Accents.kt`
- Test: `android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/AccentsTest.kt`

**Interfaces:**
- Produces:
  - `Accents.variantsFor(base: Char): List<String>` — accent variants (most-common-for-lb first), empty if none.
  - `Accents.hasVariants(base: Char): Boolean`
  - `Accents.variantIndexAt(x: Float, left: Float, cellW: Float, count: Int): Int?` — cell index under `x`, or null if outside the popup band.

- [ ] **Step 1: Write the failing tests**

Create `android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/AccentsTest.kt`:

```kotlin
package com.featherkey.keyboard

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AccentsTest {

    @Test fun e_offers_luxembourgish_accents_first() {
        assertEquals(listOf("ë", "é", "è", "ê"), Accents.variantsFor('e'))
    }

    @Test fun uppercase_base_maps_same_as_lowercase() {
        assertEquals(Accents.variantsFor('a'), Accents.variantsFor('A'))
    }

    @Test fun letters_without_accents_are_empty() {
        assertTrue(Accents.variantsFor('q').isEmpty())
        assertFalse(Accents.hasVariants('q'))
        assertTrue(Accents.hasVariants('e'))
    }

    @Test fun hit_test_maps_x_to_cell_index() {
        // Popup left=100, each cell 40 wide, 4 cells → spans [100,260).
        assertEquals(0, Accents.variantIndexAt(110f, 100f, 40f, 4))
        assertEquals(2, Accents.variantIndexAt(185f, 100f, 40f, 4))
        assertEquals(3, Accents.variantIndexAt(255f, 100f, 40f, 4))
    }

    @Test fun hit_test_is_null_outside_the_band() {
        assertNull(Accents.variantIndexAt(90f, 100f, 40f, 4))   // left of band
        assertNull(Accents.variantIndexAt(260f, 100f, 40f, 4))  // right of band
        assertNull(Accents.variantIndexAt(150f, 100f, 40f, 0))  // no variants
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `./gradlew :keyboard-view:testDebugUnitTest --tests "com.featherkey.keyboard.AccentsTest"`
Expected: FAIL — `Accents` unresolved.

- [ ] **Step 3: Write the implementation**

Create `android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/Accents.kt`:

```kotlin
package com.featherkey.keyboard

/*
 * Long-press accent variants for Latin letters (AOSP "more-keys" style). The map
 * is language-agnostic — one universal set that serves Luxembourgish (ä ë é è) as
 * well as fr/de/es/pt — with the variants most common in Luxembourgish listed
 * first so a straight-down slide lands on the likely choice. KeyboardView renders
 * and drives these; this object is pure so the map and hit-test are unit-tested.
 */
object Accents {

    private val MAP: Map<Char, List<String>> = mapOf(
        'e' to listOf("ë", "é", "è", "ê"),
        'a' to listOf("ä", "à", "â"),
        'u' to listOf("ü", "ù", "û"),
        'o' to listOf("ö", "ô"),
        'i' to listOf("ï", "î"),
        'c' to listOf("ç"),
        'n' to listOf("ñ"),
        'y' to listOf("ÿ"),
        's' to listOf("ß"),
    )

    fun variantsFor(base: Char): List<String> = MAP[base.lowercaseChar()] ?: emptyList()

    fun hasVariants(base: Char): Boolean = MAP.containsKey(base.lowercaseChar())

    /** The cell index [x] falls into for a popup of [count] cells of width [cellW]
     *  starting at [left]; null if [x] is outside `[left, left + cellW*count)`. */
    fun variantIndexAt(x: Float, left: Float, cellW: Float, count: Int): Int? {
        if (count <= 0 || cellW <= 0f || x < left) return null
        val i = ((x - left) / cellW).toInt()
        return if (i in 0 until count) i else null
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `./gradlew :keyboard-view:testDebugUnitTest --tests "com.featherkey.keyboard.AccentsTest"`
Expected: PASS — all five tests green.

- [ ] **Step 5: Commit**

```bash
git add android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/Accents.kt android/keyboard-view/src/test/kotlin/com/featherkey/keyboard/AccentsTest.kt
git commit -m "feat(lb): pure accent-variant map + popup hit-test helpers

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01CzDG6D2Jhq4hK7YUbWmVWD"
```

---

### Task 6: Long-press accent popup in `KeyboardView`

The new input subsystem: a held letter with accents opens a popup row; sliding highlights a variant; release commits it (release-in-place commits the base letter). Canvas/MotionEvent code — no unit test; verified by module compile + on-device (Task 10). Uses Task 5's tested helpers for all non-trivial logic.

**Files:**
- Modify: `android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt` — add a callback, popup state, touch-handler branches, and a draw pass.

**Interfaces:**
- Produces: `var onAccentKey: ((String) -> Unit)?` — invoked with the exact character to commit (a variant, or the base letter on release-in-place). Consumed by Task 7.
- Consumes: `Accents.variantsFor/hasVariants/variantIndexAt` (Task 5).

- [ ] **Step 1: Add the callback and popup state**

In `KeyboardView.kt`, after the `onEmoji` callback (`:61`), add:

```kotlin
    /** A long-press accent variant (or base letter) was chosen: commit it verbatim. */
    var onAccentKey: ((String) -> Unit)? = null
```

After the swipe-state block (`:201-205`, the `trail`/`gestureCell`/`gesturing`/`trailLen` fields), add:

```kotlin
    // --- Long-press accent popup state ---
    private var accentBase: Cell.Letter? = null
    private var accentVariants: List<String> = emptyList()
    private var accentIndex: Int = -1            // -1 = nothing highlighted (release = base letter)
    private var accentPopup: RectF? = null       // popup band in view pixels
    private val longPressRunnable = Runnable { startAccentMode() }
    private fun longPressTimeoutMs() = 300L
    private fun accentActive() = accentBase != null
```

- [ ] **Step 2: Add the accent-mode helpers**

Add these private methods near `resetGesture` (`:613`):

```kotlin
    /** Fired by the long-press timer: if the held letter has accents, open the popup. */
    private fun startAccentMode() {
        val base = gestureCell ?: return
        val variants = Accents.variantsFor(base.label.firstOrNull() ?: return)
        if (variants.isEmpty()) return
        gesturing = false                        // long-press wins over swipe
        accentBase = base
        accentVariants = variants
        accentIndex = -1
        accentPopup = accentPopupRect(base, variants.size)
        pressed = base
        invalidate()
    }

    /** The popup band above [base]: one key-width cell per variant, centred over the
     *  key and clamped into the view; if it would clip the top, it is pinned to y=0. */
    private fun accentPopupRect(base: Cell.Letter, count: Int): RectF {
        val cellW = base.rect.width()
        val totalW = cellW * count
        val left = (base.rect.centerX() - totalW / 2f)
            .coerceIn(sideMargin, (width - sideMargin - totalW).coerceAtLeast(sideMargin))
        val h = rowHeight
        val top = (base.rect.top - h - dp(6f)).coerceAtLeast(0f)
        return RectF(left, top, left + totalW, top + h)
    }

    private fun updateAccentSelection(x: Float) {
        val rect = accentPopup ?: return
        val n = accentVariants.size
        val cellW = rect.width() / n
        Accents.variantIndexAt(x, rect.left, cellW, n)?.let {
            if (it != accentIndex) { accentIndex = it; invalidate() }
        }
    }

    private fun resetAccent() {
        accentBase = null
        accentVariants = emptyList()
        accentIndex = -1
        accentPopup = null
        pressed = null
    }
```

- [ ] **Step 3: Hook the touch handler**

In `onTouchEvent` (`:556`), make these four edits.

(a) In `ACTION_DOWN`, the `if (page == Page.ALPHA && hit is Cell.Letter)` branch (`:566-571`), add the long-press schedule at the end of the branch (after `pressed = hit; invalidate()`):

```kotlin
                    if (Accents.hasVariants(hit.label.firstOrNull() ?: ' ')) {
                        postDelayed(longPressRunnable, longPressTimeoutMs())
                    }
```

(b) At the very top of `ACTION_MOVE` (`:579`), before `val g = gestureCell ?: return true`, add:

```kotlin
                if (accentActive()) { updateAccentSelection(event.x); return true }
```

(c) Still in `ACTION_MOVE`, inside the `if (!gesturing && trailLen > gestureStartThreshold())` block (`:585-587`), add the cancel:

```kotlin
                    removeCallbacks(longPressRunnable) // finger moved: it's a swipe
```

(d) At the very top of `ACTION_UP` (`:591`), before `val g = gestureCell`, add:

```kotlin
                removeCallbacks(longPressRunnable)
                if (accentActive()) {
                    val chosen = accentVariants.getOrNull(accentIndex) ?: accentBase?.label
                    if (chosen != null) onAccentKey?.invoke(chosen)
                    resetAccent(); resetGesture()
                    return true
                }
```

(e) In `ACTION_CANCEL` (`:606`), change `{ resetGesture(); return true }` to:

```kotlin
            MotionEvent.ACTION_CANCEL -> {
                removeCallbacks(longPressRunnable); resetAccent(); resetGesture(); return true
            }
```

- [ ] **Step 4: Draw the popup**

In `onDraw`, at the end (after the gesture-trail block, `:427`), add:

```kotlin
        if (accentActive()) drawAccentPopup(canvas, c)
```

Add the draw method after `drawTextKey` (`:480`):

```kotlin
    private fun drawAccentPopup(canvas: Canvas, c: Palette) {
        val rect = accentPopup ?: return
        val n = accentVariants.size
        if (n == 0) return
        val cellW = rect.width() / n
        // Shadow + base plate.
        canvas.drawRoundRect(rect.left, rect.top + dp(1f), rect.right, rect.bottom + dp(1.5f),
            keyRadius, keyRadius, shadowPaint)
        canvas.drawRoundRect(rect, keyRadius, keyRadius, keyPaint)
        labelPaint.textSize = rowHeight * 0.44f
        for (i in 0 until n) {
            val cell = RectF(rect.left + i * cellW, rect.top, rect.left + (i + 1) * cellW, rect.bottom)
            if (i == accentIndex) {
                iconFill.color = c.accent
                canvas.drawRoundRect(cell, keyRadius, keyRadius, iconFill)
                labelPaint.color = Color.WHITE
            } else {
                labelPaint.color = c.label
            }
            val text = if (shifted) accentVariants[i].uppercase() else accentVariants[i]
            val cy = cell.centerY() - (labelPaint.ascent() + labelPaint.descent()) / 2
            canvas.drawText(text, cell.centerX(), cy, labelPaint)
        }
        labelPaint.color = c.label
    }
```

- [ ] **Step 5: Compile the module**

Run: `./gradlew :keyboard-view:compileDebugKotlin`
Expected: BUILD SUCCESSFUL (a pre-existing `FLAG_IGNORE_GLOBAL_SETTING` deprecation warning is fine).

- [ ] **Step 6: Re-run keyboard-view unit tests (regression guard)**

Run: `./gradlew :keyboard-view:testDebugUnitTest`
Expected: PASS — `AccentsTest` green.

- [ ] **Step 7: Commit**

```bash
git add android/keyboard-view/src/main/kotlin/com/featherkey/keyboard/KeyboardView.kt
git commit -m "feat(lb): long-press accent popup subsystem in KeyboardView

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01CzDG6D2Jhq4hK7YUbWmVWD"
```

---

### Task 7: Route accent characters through the IME

Wire `onAccentKey` so a chosen accent is appended to the composing word like a typed letter (honouring shift), not committed as a word-breaking char. Android IME code — verified by module compile + on-device.

**Files:**
- Modify: `android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt` — add `handleAccent` and wire the callback where the other `keyboard.on*` callbacks are set.

**Interfaces:**
- Consumes: `KeyboardView.onAccentKey` (Task 6); `pending` (composing buffer, `:272`); `keyboard.shifted`.

- [ ] **Step 1: Add `handleAccent`**

In `FeatherKeyImeService.kt`, add after `handleTouch` (ends `:276`):

```kotlin
    /**
     * A long-press accent (or its base letter) chosen from the popup. Unlike a
     * normal tap this is an explicit pick, so it skips decode and tap-learning:
     * it is appended to the pending word exactly (upper-cased when shifted) and
     * committed, so it participates in the current word, autocorrect and learning
     * just like a decoded letter would.
     */
    private fun handleAccent(ch: String) {
        val ic = currentInputConnection ?: return
        val kb = keyboard
        val out = if (kb?.shifted == true) ch.uppercase() else ch
        pending.append(out)
        ic.commitText(out, 1)
        if (kb?.shifted == true) kb.shifted = false
        updateSuggestions()
    }
```

- [ ] **Step 2: Wire the callback**

Find where the keyboard callbacks are assigned (the block setting `keyboard.onKeyTouch`, `onCharKey`, `onGesture`, etc. — created in `onCreateInputView`). Add alongside them:

```kotlin
        keyboard.onAccentKey = ::handleAccent
```

- [ ] **Step 3: Compile the module**

Run: `./gradlew :ime-service:compileDebugKotlin`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 4: Commit**

```bash
git add android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt
git commit -m "feat(lb): commit long-press accents into the composing word

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01CzDG6D2Jhq4hK7YUbWmVWD"
```

---

### Task 8: Generate and commit the `lb` lexicon + frequency data (+ NOTICES)

Offline generation from the licensed sources, then commit the two data files and the attribution notice. There is no in-repo generator (matches the existing lexicon precedent); the steps below are the exact offline procedure plus an automated validation gate.

**Files:**
- Create: `android/ime-service/src/main/assets/lexicons/lb.txt`
- Create: `android/ime-service/src/main/assets/freq/lb.txt`
- Create: `NOTICES` (repo root)

**Interfaces:**
- Produces: two assets satisfying the file contracts in Global Constraints; `LanguageCatalog.all(...)` now reports `hasLexicon = true` for `lb`.

- [ ] **Step 1: Fetch the sources (offline, into the scratchpad — not committed)**

```bash
SCRATCH=/private/tmp/claude-501/-Users-pablohpsilva-Documents-android-ikeyboard/lb-data
mkdir -p "$SCRATCH" && cd "$SCRATCH"
curl -fsSL -o unmunched.dic https://raw.githubusercontent.com/spellchecker-lu/dictionary-lb-lu/master/unmunched.dic
# Leipzig: download a Luxembourgish word/frequency package from
#   https://wortschatz.uni-leipzig.de/en/download/Luxembourgish
# and extract its *-words.txt (columns: rank <tab> word <tab> count).
```
Expected: `unmunched.dic` present (surface forms, one per line, mixed case, with diacritics); a Leipzig `*-words.txt` present.

- [ ] **Step 2: Build `lb.txt` (lowercase, deduped vs de/fr/en, `LC_ALL=C`-sorted, ~12k cap)**

```bash
cd "$SCRATCH"
ASSETS=/Users/pablohpsilva/Documents/android-ikeyboard/android/ime-service/src/main/assets
# Union of existing companion lexicons, to subtract shared words.
cat "$ASSETS"/lexicons/de.txt "$ASSETS"/lexicons/fr.txt "$ASSETS"/lexicons/en.txt \
  | LC_ALL=C sort -u > companions.txt
# Lowercase unmunched surface forms, keep letters+diacritics, drop companions,
# byte-sort unique, cap to ~12k most (byte-)relevant. NOTE: if capping, keep the
# highest-Leipzig-frequency 12k rather than a blind head — see Step 3's freq order.
tr '[:upper:]' '[:lower:]' < unmunched.dic \
  | grep -E "^[a-zäëéèêöüùûôîïçñÿ']+$" \
  | LC_ALL=C sort -u \
  | LC_ALL=C comm -23 - companions.txt \
  > lb_all.txt
wc -l lb_all.txt
```
Expected: `lb_all.txt` is the distinctively-Luxembourgish, lowercase, byte-sorted candidate set. (If `wc -l` ≫ 12000, Step 3 selects the 12k by frequency, then re-sort.)

- [ ] **Step 3: Order `freq/lb.txt` by Leipzig frequency; finalise the 12k `lb.txt`**

```bash
cd "$SCRATCH"
# Leipzig words in frequency order (col 2 = word), lowercased, restricted to our set.
awk -F'\t' '{print tolower($2)}' *-words.txt > leipzig_order.txt
# freq/lb.txt: Leipzig-ranked words that are in lb_all.txt, then any remaining
# lb_all words appended (rare-but-valid), de-duped, preserving first occurrence.
grep -Fxf lb_all.txt leipzig_order.txt | awk '!seen[$0]++' > freq_lb.txt
grep -Fxvf freq_lb.txt lb_all.txt >> freq_lb.txt
# Cap the shipped vocabulary to the top ~12k by this frequency order.
head -12000 freq_lb.txt > "$ASSETS"/freq/lb.txt
# lexicons/lb.txt = the same 12k, but byte-sorted for the FST.
LC_ALL=C sort -u "$ASSETS"/freq/lb.txt > "$ASSETS"/lexicons/lb.txt
wc -l "$ASSETS"/freq/lb.txt "$ASSETS"/lexicons/lb.txt
```
Expected: both files ≤ 12000 lines; `freq/lb.txt` most-common-first; `lexicons/lb.txt` byte-sorted.

- [ ] **Step 4: Validate the data files (automated gate)**

```bash
ASSETS=/Users/pablohpsilva/Documents/android-ikeyboard/android/ime-service/src/main/assets
LC_ALL=C sort -c "$ASSETS"/lexicons/lb.txt && echo "SORTED-OK"
python3 -c "import sys; sys.exit(0 if open('$ASSETS/lexicons/lb.txt',encoding='utf-8').read() and open('$ASSETS/freq/lb.txt',encoding='utf-8').read() else 1)" && echo "UTF8-OK"
grep -qi 'lëtzebuergesch' "$ASSETS"/lexicons/lb.txt && echo "HAS-DIACRITIC-FORMS"
```
Expected: `SORTED-OK`, `UTF8-OK`, `HAS-DIACRITIC-FORMS`. If `sort -c` fails, the FST would reject at runtime — fix the sort (must be `LC_ALL=C`).

- [ ] **Step 5: Write the `NOTICES` file**

Create `NOTICES` at the repo root:

```
Third-party data notices
=========================

Luxembourgish word list (android/ime-service/src/main/assets/lexicons/lb.txt
and .../freq/lb.txt)

- Vocabulary derived from the Spellchecker.lu Luxembourgish Hunspell dictionary
  (unmunched surface forms): https://github.com/spellchecker-lu/dictionary-lb-lu
  © Michel Weimerskirch, Sandra Souza Morais and contributors.
  Licensed under the European Union Public Licence v.1.1 (EUPL v1.1).
  The derived word list is made available under the EUPL v1.1.

- Frequency ordering derived from the Leipzig Corpora Collection (Wortschatz),
  Luxembourgish: https://wortschatz.uni-leipzig.de/en/download/Luxembourgish
  © Universität Leipzig / Sächsische Akademie der Wissenschaften / InfAI.
  Licensed under Creative Commons Attribution (CC-BY 3.0).
```

- [ ] **Step 6: Commit**

```bash
cd /Users/pablohpsilva/Documents/android-ikeyboard
git add android/ime-service/src/main/assets/lexicons/lb.txt android/ime-service/src/main/assets/freq/lb.txt NOTICES
git commit -m "feat(lb): bundle Luxembourgish lexicon + frequency list (EUPL/CC-BY)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01CzDG6D2Jhq4hK7YUbWmVWD"
```

---

### Task 9: Declare the Luxembourgish IME subtype

Adds the `lb_LU` subtype so the system input-method picker and language switch know about it.

**Files:**
- Modify: `android/app/src/main/res/xml/method.xml:10-13`
- Modify: `android/app/src/main/res/values/strings.xml:5`

**Interfaces:** none (Android resource metadata).

- [ ] **Step 1: Add the subtype string**

In `android/app/src/main/res/values/strings.xml`, after the `subtype_en` line (`:5`):

```xml
    <string name="subtype_lb">Lëtzebuergesch</string>
```

- [ ] **Step 2: Add the subtype declaration**

In `android/app/src/main/res/xml/method.xml`, after the closing `/>` of the `en_US` `<subtype>` (`:13`):

```xml
    <subtype
        android:name="@string/subtype_lb"
        android:imeSubtypeLocale="lb_LU"
        android:imeSubtypeMode="keyboard" />
```

- [ ] **Step 3: Verify resources merge**

Run: `./gradlew :app:processDebugResources`
Expected: BUILD SUCCESSFUL (no resource-linking errors).

- [ ] **Step 4: Commit**

```bash
git add android/app/src/main/res/xml/method.xml android/app/src/main/res/values/strings.xml
git commit -m "feat(lb): declare the Luxembourgish IME subtype (lb_LU)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01CzDG6D2Jhq4hK7YUbWmVWD"
```

---

### Task 10: Full build, install, and on-device verification

Assemble the whole APK (native `.so` via cargo-ndk + Gradle), install on the connected device, and verify the feature end-to-end. This is the acceptance gate for the authored-not-compiled shell.

**Files:** none (build/verify only).

- [ ] **Step 1: Build the native core and assemble the debug APK**

Run the project's established build (cargo-ndk producing `libfeatherkey_core.so` into `jniLibs`, then `./gradlew :app:assembleDebug`). Confirm only `libfeatherkey_core.so` is copied into `jniLibs` (remove any stray `libredb-*.so`, per prior session).
Expected: `BUILD SUCCESSFUL`; APK at `android/app/build/outputs/apk/debug/`.

- [ ] **Step 2: Install on the connected device**

Run: `./gradlew :app:installDebug` (or `adb install -r <apk>`).
Expected: `Success`; device shows the FeatherKey app.

- [ ] **Step 3: Verify — Luxembourgish activation + companion bundle**

Open FeatherKey settings → Languages → add **Lëtzebuergesch**.
Expected: `lb` appears **first** (primary); `de`, `fr`, `en` are **auto-added** and visible/removable. Space-bar hint reads `LB DE FR`.

- [ ] **Step 4: Verify — QWERTZ + code-switching predictions**

Type a mixed sentence (a Luxembourgish word, a French noun, a German word) in a normal text field.
Expected: keyboard is QWERTZ (`z` where a QWERTY `y` sits); the strip surfaces Luxembourgish completions and switches languages per word without fighting corrections.

- [ ] **Step 5: Verify — long-press accents**

Long-press `e`, then `a`, `o`, `u`, `c` in turn; slide and release on a variant.
Expected: popup shows the variants (`e`→`ë é è ê` …); releasing on a variant commits it; a long-press released **in place** commits the base letter; top-row popups (`e u o i`) render without clipping off-screen (they sit in the strip band / pinned to the top edge). Committed accented words appear as suggestions on retype.

- [ ] **Step 6: Verify — one-shot + privacy**

Remove `de` from Languages, then leave and re-enter a field.
Expected: `de` stays removed (not silently re-added). In a password field, no learning occurs (no new suggestions from what was typed).

- [ ] **Step 7: Final confirmation**

Confirm the git log shows Tasks 1–9 committed with the mandatory trailers, working tree clean (native `.so`/overlay remain local-only per project convention).

---

## Self-Review

**Spec coverage:**
- §2 core-unchanged / layout +1 line → Task 1. ✅
- §3 licensing / NOTICES → Task 8 (Step 5). ✅
- §4 data pipeline (lowercase, keep diacritics, `LC_ALL=C` sort, ~12k) → Task 8. ✅
- §5 QWERTZ reuse + test → Task 1. ✅
- §6 accent popups (map, gesture, render, IME wiring) → Tasks 5 (map/hit-test), 6 (popup), 7 (IME). ✅
- §7 companion bundle (in `setActiveTags`, one-shot) → Tasks 3 (logic) + 4 (wiring). ✅
- §8 catalog + subtype → Tasks 2 + 9. ✅
- §9 testing (Rust test, data validation, on-device checklist) → Tasks 1, 8 (Step 4), 10. ✅
- §11 top-row clipping risk → Task 6 (`accentPopupRect` clamp) + Task 10 Step 5 verification. ✅

**Placeholder scan:** No `TBD`/`TODO`/"add error handling"/"similar to Task N". The offline data *generation* (Task 8 Steps 1–3) is inherently manual (external downloads) but every step has exact commands; the automated gate is Step 4. ✅

**Type consistency:** `LanguageBundle.withCompanions(...) → Result(tags, bundleApplied)` used identically in Tasks 3 and 4. `Accents.variantsFor/hasVariants/variantIndexAt` signatures match between Task 5 (definition) and Task 6 (use). `onAccentKey: ((String) -> Unit)?` defined in Task 6, consumed via `::handleAccent` in Task 7. `KEY_BUNDLE_APPLIED` defined and used within Task 4. ✅
