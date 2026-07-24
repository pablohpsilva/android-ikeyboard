package com.featherkey.ime

/*
 * On-device next-word (bigram) learning: for each word the user commits, it
 * remembers which words they tend to type next, as prev -> {next -> count}.
 * It powers two things once the user has some history:
 *  - next-word predictions on an empty prefix (right after a word + space);
 *  - a context bias for prefix completions, so a word that usually follows the
 *    previous one outranks an equally-frequent word that does not.
 *
 * All local: a small TSV in the app's private files; nothing leaves the device
 * (BR-13). Recording is gated by the learning-consent toggle and field
 * sensitivity upstream, exactly like [UsageModel].
 */

import android.content.Context
import java.io.File

class ContextModel(context: Context) {

    private val file = File(context.filesDir, "context.tsv")
    /** prev -> (next -> count). */
    private val counts = HashMap<String, HashMap<String, Int>>()
    @Volatile private var dirty = false

    /** Load persisted transitions (best-effort; a missing/corrupt file starts empty). */
    fun load() {
        counts.clear()
        runCatching {
            if (!file.exists()) return
            file.bufferedReader().useLines { lines ->
                for (line in lines) {
                    val p = line.split('\t')
                    if (p.size != 3) continue
                    val prev = p[0]
                    val next = p[1]
                    val c = p[2].toIntOrNull() ?: continue
                    if (prev.isEmpty() || next.isEmpty() || c <= 0) continue
                    counts.getOrPut(prev) { HashMap() }[next] = c
                }
            }
        }
    }

    /** Record that [next] followed [prev]. Skips very short words (weak signal). */
    fun record(prev: String, next: String) {
        if (prev.length < 2 || next.length < 2) return
        val m = counts.getOrPut(prev) { HashMap() }
        m[next] = (m[next] ?: 0) + 1
        dirty = true
    }

    /** The words most often typed after [prev], most-frequent first. */
    fun nextWords(prev: String, limit: Int): List<String> =
        counts[prev]?.entries
            ?.sortedByDescending { it.value }
            ?.take(limit)
            ?.map { it.key }
            ?: emptyList()

    /** Raw next-word counts after [prev] (empty if none / no context). */
    fun nextCounts(prev: String?): Map<String, Int> =
        if (prev == null) emptyMap() else counts[prev] ?: emptyMap()

    /** Write transitions back to disk if changed. Call off the input path. */
    fun persist() {
        if (!dirty) return
        runCatching {
            file.bufferedWriter().use { w ->
                for ((prev, m) in counts) for ((next, c) in m) {
                    w.write(prev); w.write("\t"); w.write(next); w.write("\t"); w.write(c.toString())
                    w.newLine()
                }
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
