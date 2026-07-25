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

    /** One language: its tag, words sorted for prefix search, plus each word's freq rank. */
    private class Lang(val tag: String, val sorted: Array<String>, val rank: HashMap<String, Int>)

    /** One prefix/correction candidate tagged by language and its rank within it. */
    data class Candidate(val word: String, val lang: String, val sourceRank: Int)

    /** Every active-language word (swipe searches this). */
    val words: List<String> = langs.flatMap { it.rank.keys }.distinct()

    /** Frequency rank of [word] (0 = most common); [Int.MAX_VALUE] if unknown. */
    fun rankOf(word: String): Int {
        var best = Int.MAX_VALUE
        for (l in langs) l.rank[word]?.let { if (it < best) best = it }
        return best
    }

    /** Active languages whose frequency list contains [word] (soft momentum signal). */
    fun languagesOf(word: String): Set<String> =
        langs.asSequence().filter { it.rank.containsKey(word) }.map { it.tag }.toSet()

    /**
     * Up to [k] prefix matches per language, ranked within each language by the
     * same priority: context continuation first, then the
     * user's learned usage, then bundled frequency. This keeps
     * personalization/context ordering WITHIN a language; the core momentum
     * ranker (fed [Candidate.sourceRank]) still decides ACROSS languages.
     */
    fun candidatesByLanguage(
        prefix: String,
        learned: Map<String, Int>,
        context: Map<String, Int>,
        k: Int,
    ): List<Candidate> {
        if (prefix.isEmpty()) return emptyList()
        val out = ArrayList<Candidate>()
        for (l in langs) {
            val matches = prefixMatches(l, prefix, k + CANDIDATE_MARGIN)
            val ordered = matches.sortedWith(
                compareByDescending<String> { context[it] ?: 0 }
                    .thenByDescending { learned[it] ?: 0 }
                    .thenBy { l.rank[it] ?: Int.MAX_VALUE }
            )
            ordered.take(k).forEachIndexed { i, w -> out.add(Candidate(w, l.tag, i)) }
        }
        return out
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
        private const val CANDIDATE_MARGIN = 5 // extra frequency-ranked matches gathered
                                                // per language before re-sorting by
                                                // context/learned so a less-frequent but
                                                // learned/context word can still surface

        /** An empty vocabulary, used until the real one finishes loading. */
        fun empty(): Vocabulary = Vocabulary(emptyList())

        /** Load the frequency lists for [tags]; languages with no list are skipped. */
        fun load(context: Context, tags: List<String>): Vocabulary {
            val langs = tags.mapNotNull { tag ->
                val ordered = readFreq(context, tag)
                if (ordered.isEmpty()) return@mapNotNull null
                val rank = HashMap<String, Int>(ordered.size * 2)
                ordered.forEachIndexed { i, w -> rank.putIfAbsent(w, i) }
                Lang(tag, ordered.toTypedArray().also { it.sort() }, rank)
            }
            return Vocabulary(langs)
        }

        /** Test-only builder from in-memory frequency lists (index = frequency rank). */
        fun forTest(byLang: Map<String, List<String>>): Vocabulary {
            val langs = byLang.map { (tag, words) ->
                val rank = HashMap<String, Int>(words.size * 2)
                words.forEachIndexed { i, w -> rank.putIfAbsent(w, i) }
                Lang(tag, words.toTypedArray().also { it.sort() }, rank)
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
