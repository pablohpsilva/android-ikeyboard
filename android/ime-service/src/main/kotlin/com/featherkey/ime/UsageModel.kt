package com.featherkey.ime

/*
 * On-device learning of how the user types: a word -> use-count map that biases
 * both tap suggestions and swipe decoding toward the words this user actually
 * uses. Persisted locally as a small TSV in the app's private files; nothing
 * leaves the device (BR-13). Recording is gated by the learning-consent toggle
 * and by field sensitivity upstream, exactly like the core's own learning.
 */

import android.content.Context
import java.io.File

class UsageModel(context: Context) {

    private val file = File(context.filesDir, "usage.tsv")
    private val counts = HashMap<String, Int>()
    @Volatile private var dirty = false

    /** Load persisted counts (best-effort; a missing/corrupt file starts empty). */
    fun load() {
        counts.clear()
        runCatching {
            if (!file.exists()) return
            file.bufferedReader().useLines { lines ->
                for (line in lines) {
                    val tab = line.indexOf('\t')
                    if (tab <= 0) continue
                    val w = line.substring(0, tab)
                    val c = line.substring(tab + 1).toIntOrNull() ?: continue
                    if (w.isNotEmpty() && c > 0) counts[w] = c
                }
            }
        }
    }

    /** Fold one committed word into the counts (call only when consent is on). */
    fun record(word: String) {
        if (word.length < 2) return
        counts[word] = (counts[word] ?: 0) + 1
        dirty = true
    }

    fun count(word: String): Int = counts[word] ?: 0

    /** Immutable-enough view for ranking (read-only use). */
    val map: Map<String, Int> get() = counts

    /** Write counts back to disk if changed. Call off the input path (debounced). */
    fun persist() {
        if (!dirty) return
        runCatching {
            file.bufferedWriter().use { w ->
                for ((word, c) in counts) { w.write(word); w.write("\t"); w.write(c.toString()); w.newLine() }
            }
            dirty = false
        }
    }

    /** Forget everything (used when the user clears learned data). */
    fun clear() {
        counts.clear(); dirty = false
        runCatching { file.delete() }
    }
}
