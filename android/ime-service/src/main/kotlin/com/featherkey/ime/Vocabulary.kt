package com.featherkey.ime

/*
 * The active languages' vocabulary, loaded from frequency-ordered word lists
 * (assets/freq/<tag>.txt, most-common first). It powers both:
 *  - tap suggestions: prefix completions ranked by the user's learned usage,
 *    then word frequency, fairly interleaved across languages so every active
 *    language is represented (not just whichever has the shortest words);
 *  - swipe decoding: the candidate word set plus a per-word frequency rank.
 *
 * All local: the lists ship in the APK; nothing is fetched.
 */

import android.content.Context

class Vocabulary private constructor(private val langs: List<Lang>) {

    /** One language: words sorted for prefix search, plus each word's freq rank. */
    private class Lang(val sorted: Array<String>, val rank: HashMap<String, Int>)

    /** Every active-language word (swipe searches this). */
    val words: List<String> = langs.flatMap { it.rank.keys }.distinct()

    /** Frequency rank of [word] (0 = most common); [Int.MAX_VALUE] if unknown. */
    fun rankOf(word: String): Int {
        var best = Int.MAX_VALUE
        for (l in langs) l.rank[word]?.let { if (it < best) best = it }
        return best
    }

    /**
     * Up to [limit] completions of [prefix]: learned words first (by use count),
     * then the most frequent completion from each language in round-robin, so a
     * multilingual user sees every active language.
     */
    fun suggestions(prefix: String, learned: Map<String, Int>, limit: Int): List<String> {
        if (prefix.isEmpty()) return emptyList()
        val out = LinkedHashSet<String>()

        // 1) Words the user has actually used that match the prefix, most-used first.
        learned.keys.asSequence()
            .filter { it.startsWith(prefix) }
            .sortedWith(compareByDescending<String> { learned[it] ?: 0 }.thenBy { rankOf(it) })
            .forEach { if (out.size < limit) out.add(it) }
        if (out.size >= limit) return out.toList()

        // 2) Round-robin the most-frequent matches across languages.
        val perLang = langs.map { prefixMatches(it, prefix, limit + 2) }
        var i = 0
        while (out.size < limit) {
            var added = false
            for (matches in perLang) {
                if (i < matches.size) {
                    out.add(matches[i]); added = true
                    if (out.size >= limit) break
                }
            }
            if (!added) break
            i++
        }
        return out.toList()
    }

    /** The [k] most frequent words in one language that start with [prefix]. */
    private fun prefixMatches(lang: Lang, prefix: String, k: Int): List<String> {
        val a = lang.sorted
        var lo = lowerBound(a, prefix)
        val hits = ArrayList<String>()
        while (lo < a.size && a[lo].startsWith(prefix)) { hits.add(a[lo]); lo++ }
        hits.sortBy { lang.rank[it] ?: Int.MAX_VALUE }
        return if (hits.size > k) hits.subList(0, k) else hits
    }

    /** First index whose word is >= [prefix]. */
    private fun lowerBound(a: Array<String>, prefix: String): Int {
        var lo = 0; var hi = a.size
        while (lo < hi) {
            val mid = (lo + hi) ushr 1
            if (a[mid] < prefix) lo = mid + 1 else hi = mid
        }
        return lo
    }

    companion object {
        /** An empty vocabulary, used until the real one finishes loading. */
        fun empty(): Vocabulary = Vocabulary(emptyList())

        /** Load the frequency lists for [tags]; languages with no list are skipped. */
        fun load(context: Context, tags: List<String>): Vocabulary {
            val langs = tags.mapNotNull { tag ->
                val ordered = readFreq(context, tag)
                if (ordered.isEmpty()) return@mapNotNull null
                val rank = HashMap<String, Int>(ordered.size * 2)
                ordered.forEachIndexed { i, w -> rank.putIfAbsent(w, i) }
                Lang(ordered.toTypedArray().also { it.sort() }, rank)
            }
            return Vocabulary(langs)
        }

        private fun readFreq(context: Context, tag: String): List<String> =
            runCatching {
                context.assets.open("freq/$tag.txt").bufferedReader().useLines { lines ->
                    lines.map { it.trim() }.filter { it.isNotEmpty() }.toList()
                }
            }.getOrDefault(emptyList())
    }
}
