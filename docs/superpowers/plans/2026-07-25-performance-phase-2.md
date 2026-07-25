# FeatherKey Performance — Phase 2 (safe wins) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Cut the per-keystroke suggestion-compute cost with two **low-risk, result-preserving** changes — bound the short-prefix scan, and coalesce the device-dictionary strip re-run — then re-measure on the benchmark build. No behavior change, no test weakened.

**Architecture:** Both changes are in the `ime-service` module. `Vocabulary.prefixMatches` becomes a bounded top-k selection (identical result, no full-range list/sort). The device-dictionary callback coalesces multiple same-keystroke re-runs into one. No threading/concurrency change this phase (the off-thread async move is deferred pending these measurements).

**Tech Stack:** Kotlin, Android IME, JUnit4 (`ime-service` already has a test source set).

## Global Constraints
- **Result-preserving:** `prefixMatches` must return exactly the same words as today (the `k` lowest-rank matches, rank-ascending, ties in scan/alphabetical order). Proven by a unit test comparing against the old collect-all→sort→take-k reference.
- No feature removed, no existing test deleted/weakened. No runtime network. No sensitive-field behavior change (device dict still gated by `!field.isSensitive()`).
- Verified anchors (current code): `Vocabulary.prefixMatches` at `Vocabulary.kt:66-73`; `candidatesByLanguage` calls it with `k + CANDIDATE_MARGIN` (`:54`); the device-dict callback is wired at `FeatherKeyImeService.kt:101` as `DeviceDictionary(this) { keyboard?.post { updateSuggestions() } }`.
- Measure on the reference A16 benchmark build with `tools/perf/jank.sh`.

---

### Task 1: Bound `prefixMatches` to a top-k selection (result-identical)

**Files:**
- Modify: `android/ime-service/src/main/kotlin/com/featherkey/ime/Vocabulary.kt`
- Test: `android/ime-service/src/test/kotlin/com/featherkey/ime/VocabularyPrefixTest.kt` (create)

**Interfaces:**
- `prefixMatches` keeps its signature `(lang, prefix, k) -> List<String>` and its exact result; only the internals change from collect-all→sort→take-k to a bounded scan.

**Context:** For a 1-char prefix, the current code appends every matching word (hundreds–thousands) to an `ArrayList` and sorts the whole list before taking `k`. Selecting the `k` best by rank *during* the scan gives the identical result with O(k) memory and no full sort.

- [ ] **Step 1: Write the failing result-identity test**

Create `android/ime-service/src/test/kotlin/com/featherkey/ime/VocabularyPrefixTest.kt`:
```kotlin
package com.featherkey.ime

import org.junit.Assert.assertEquals
import org.junit.Test

class VocabularyPrefixTest {
    // Frequency rank = index in the list (0 = most common). "sun" is the MOST frequent
    // s-word but alphabetically LAST here — the bound must still pick it over rarer,
    // alphabetically-earlier s-words (proves we select by rank, not by scan position).
    private val vocab = Vocabulary.forTest(
        mapOf("en" to listOf("sun", "sea", "sky", "sad", "set", "sit", "six", "saw", "sir", "ski", "soy", "spa"))
    )

    @Test fun returns_top_k_by_frequency_not_alphabetical_prefix_order() {
        // k=3 completions of "s": the three most frequent are sun(0), sea(1), sky(2).
        val got = vocab.candidatesByLanguage("s", emptyMap(), emptyMap(), 3).map { it.word }
        assertEquals(listOf("sun", "sea", "sky"), got)
    }

    @Test fun a_high_frequency_word_is_never_crowded_out_by_earlier_rarer_matches() {
        // "sun" (rank 0) must appear even though 8 rarer s-words sort before it alphabetically.
        val got = vocab.candidatesByLanguage("s", emptyMap(), emptyMap(), 1).map { it.word }
        assertEquals(listOf("sun"), got)
    }

    @Test fun fewer_matches_than_k_returns_all_of_them() {
        val got = vocab.candidatesByLanguage("sk", emptyMap(), emptyMap(), 5).map { it.word }.sorted()
        assertEquals(listOf("ski", "sky"), got) // only two "sk" words exist
    }
}
```

- [ ] **Step 2: Run the test against the CURRENT (collect-all) implementation to verify it passes**

Run: `cd android && ./gradlew :ime-service:testDebugUnitTest --tests '*VocabularyPrefixTest'`
Expected: PASS — this pins the *current* behavior as the reference before refactoring. (If it fails, the test encodes a wrong expectation; fix the test first.)

- [ ] **Step 3: Replace `prefixMatches` with a bounded top-k selection**

In `Vocabulary.kt`, replace `prefixMatches` (lines 66-73) with:
```kotlin
    /** The [k] most frequent words in one language that start with [prefix].
     *  Selects the top-k by rank during the scan — identical result to
     *  collect-all → sortBy(rank) → take(k), but O(k) memory and no full sort. */
    private fun prefixMatches(lang: Lang, prefix: String, k: Int): List<String> {
        if (k <= 0) return emptyList()
        val a = lang.sorted
        var lo = lowerBound(a, prefix)
        // keptWords/keptRanks stay rank-ascending; ties keep scan (alphabetical) order,
        // matching the old stable sortBy on an alphabetically-ordered input.
        val keptWords = ArrayList<String>(k)
        val keptRanks = ArrayList<Int>(k)
        while (lo < a.size && a[lo].startsWith(prefix)) {
            val w = a[lo]; lo++
            val r = lang.rank[w] ?: Int.MAX_VALUE
            if (keptWords.size < k) {
                insertByRank(keptWords, keptRanks, w, r)
            } else if (r < keptRanks[keptRanks.size - 1]) { // beats the current worst
                keptWords.removeAt(keptWords.size - 1)
                keptRanks.removeAt(keptRanks.size - 1)
                insertByRank(keptWords, keptRanks, w, r)
            }
        }
        return keptWords
    }

    /** Insert (w,r) keeping ranks ascending; on equal rank insert AFTER existing ones,
     *  so ties preserve the caller's scan (alphabetical) order (stable-sort equivalent). */
    private fun insertByRank(words: ArrayList<String>, ranks: ArrayList<Int>, w: String, r: Int) {
        var i = ranks.size
        while (i > 0 && ranks[i - 1] > r) i--
        words.add(i, w); ranks.add(i, r)
    }
```

- [ ] **Step 4: Run the identity test + the full ime-service suite**

Run: `cd android && ./gradlew :ime-service:testDebugUnitTest`
Expected: PASS, including `VocabularyPrefixTest` and all pre-existing tests (candidatesByLanguage callers must be unaffected).

- [ ] **Step 5: Commit**

```bash
git add android/ime-service/src/main/kotlin/com/featherkey/ime/Vocabulary.kt android/ime-service/src/test/kotlin/com/featherkey/ime/VocabularyPrefixTest.kt
git commit -m "perf(ime): bound prefixMatches to top-k selection (result-identical, no full-range sort)"
```

---

### Task 2: Coalesce the device-dictionary strip re-run

**Files:**
- Modify: `android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt`

**Interfaces:**
- Consumes: the `DeviceDictionary(context, onResult)` callback contract (fires on the main thread when a per-language lookup lands).

**Context:** Each active language's device-dictionary result arrives separately and each calls `onResult` → `keyboard?.post { updateSuggestions() }` (`FeatherKeyImeService.kt:101`). With N languages, one keystroke can trigger up to N full `rankForStrip` re-runs on the main thread. Coalescing multiple results that arrive within a frame into a single refresh removes the redundant passes. Still main-thread (no concurrency change); the only behavior change is that near-simultaneous device results refresh the strip once instead of N times — the final strip content is identical.

- [ ] **Step 1: Add a coalescing refresh Runnable and use it in the callback**

In `FeatherKeyImeService.kt`, add a field and a small constant, and change the `deviceDict` construction:
```kotlin
    // Coalesces multiple device-dictionary results that land within a frame into a
    // single strip refresh (each language answers separately; without this a
    // keystroke re-runs rankForStrip once per answering language).
    private val refreshStrip = Runnable { updateSuggestions() }
```
Replace the `deviceDict = DeviceDictionary(this) { keyboard?.post { updateSuggestions() } }` line (`:101`) with:
```kotlin
        deviceDict = DeviceDictionary(this) {
            keyboard?.removeCallbacks(refreshStrip)
            keyboard?.postDelayed(refreshStrip, DEVICE_REFRESH_COALESCE_MS)
        }
```
Add the constant to the existing companion object (find `companion object`):
```kotlin
        private const val DEVICE_REFRESH_COALESCE_MS = 16L // ~one frame; batch same-keystroke device results
```

- [ ] **Step 2: Clear the pending refresh on teardown**

To avoid a refresh firing after the input view is gone, remove the callback in `onFinishInput()` (after the existing body) — the `keyboard` may be reused, so cancel any queued refresh:
```kotlin
        keyboard?.removeCallbacks(refreshStrip)
```
(Place it in `onFinishInput`, alongside the existing `keyboard?.suggestions = emptyList()`.)

- [ ] **Step 3: Build + run the suite**

Run: `cd android && ./gradlew :ime-service:testDebugUnitTest :app:assembleDebug`
Expected: BUILD SUCCESSFUL; existing tests pass (no test targets this glue directly, so this is a compile + regression check).

- [ ] **Step 4: Commit**

```bash
git add android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt
git commit -m "perf(ime): coalesce device-dictionary strip re-runs into one refresh per frame"
```

---

### Task 3: Measure (Phase 2 exit)

**Files:**
- Modify: `docs/superpowers/specs/2026-07-25-performance-optimization-design.md` (appendix)

- [ ] **Step 1: Build + install the benchmark build; re-measure**

`./gradlew :app:assembleBenchmark`; install; re-select the IME; run `tools/perf/jank.sh RZCY51D0T1K 5` a few times against a focused field. Record janky% / p95 / p99 / slow-UI.

- [ ] **Step 2: Record before/after in the spec appendix and decide on the async move**

Append the Phase 2 numbers next to the Phase 1 benchmark numbers. If per-keystroke stalls persist materially, the deferred off-thread async move (snapshot-on-main + latest-wins) becomes Phase 2b; if not, note that the safe wins sufficed.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-07-25-performance-optimization-design.md
git commit -m "docs(perf): record Phase 2 measurements"
```

---

## Phase 2 Exit Criteria
- `prefixMatches` result-identical (unit-proven); no full-range list/sort per short-prefix keystroke.
- Device-dictionary results coalesced to one strip refresh per frame.
- ime-service suite green; benchmark build green; before/after measured on the A16.
- No feature removed; decision recorded on whether the off-thread async move (Phase 2b) is warranted.
