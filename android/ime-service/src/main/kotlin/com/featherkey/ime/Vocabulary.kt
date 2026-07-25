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
import kotlin.math.ln

class Vocabulary private constructor(private val langs: List<Lang>) {

    /** One language: its tag, words sorted for prefix search, plus each word's freq rank. */
    private class Lang(val tag: String, val sorted: Array<String>, val rank: HashMap<String, Int>)

    /** One prefix/correction candidate tagged by language and its rank within it. */
    data class Candidate(val word: String, val lang: String, val sourceRank: Int)

    /** One live hypothesis in the tap-decode beam: a prefix + its spatial log-prob. */
    private class Hyp(val prefix: String, val score: Float)

    /** Every active-language word (swipe searches this). */
    val words: List<String> = langs.flatMap { it.rank.keys }.distinct()

    /** Frequency rank of [word] (0 = most common); [Int.MAX_VALUE] if unknown. */
    fun rankOf(word: String): Int {
        var best = Int.MAX_VALUE
        for (l in langs) l.rank[word]?.let { if (it < best) best = it }
        return best
    }

    /**
     * Up to [limit] completions of [prefix]: words that usually follow the
     * previous word ([context]) first, then the user's learned words (by use
     * count), then the most frequent completion from each language in
     * round-robin, so a multilingual user sees every active language.
     */
    fun suggestions(
        prefix: String,
        learned: Map<String, Int>,
        limit: Int,
        context: Map<String, Int> = emptyMap(),
    ): List<String> {
        if (prefix.isEmpty()) return emptyList()
        val out = LinkedHashSet<String>()

        // 1) Context continuations: words that usually follow the previous word
        //    and match the prefix, most-likely first — the strongest signal.
        context.keys.asSequence()
            .filter { it.startsWith(prefix) }
            .sortedWith(compareByDescending<String> { context[it] ?: 0 }.thenBy { rankOf(it) })
            .forEach { if (out.size < limit) out.add(it) }
        if (out.size >= limit) return out.toList()

        // 2) Words the user has actually used that match the prefix, most-used first.
        learned.keys.asSequence()
            .filter { it.startsWith(prefix) }
            .sortedWith(compareByDescending<String> { learned[it] ?: 0 }.thenBy { rankOf(it) })
            .forEach { if (out.size < limit) out.add(it) }
        if (out.size >= limit) return out.toList()

        // 3) Round-robin the most-frequent matches across languages.
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

    /** Active languages whose frequency list contains [word] (soft momentum signal). */
    fun languagesOf(word: String): Set<String> =
        langs.asSequence().filter { it.rank.containsKey(word) }.map { it.tag }.toSet()

    /** Up to [k] prefix matches per language, ranked within each by frequency. */
    fun candidatesByLanguage(
        prefix: String,
        learned: Map<String, Int>,
        context: Map<String, Int>,
        k: Int,
    ): List<Candidate> {
        if (prefix.isEmpty()) return emptyList()
        val out = ArrayList<Candidate>()
        for (l in langs) {
            prefixMatches(l, prefix, k).forEachIndexed { i, w ->
                out.add(Candidate(w, l.tag, i))
            }
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

    /**
     * Noisy-channel tap decode. Given the per-tap key-probability distributions
     * [taps] (each a char -> probability map, from the decoder's ranked
     * candidates), return the most likely intended complete words: it scores how
     * well each word's spelling fits the taps *spatially* against a language
     * model — word frequency, the user's learned words, and the previous-word
     * context. A bounded beam over the taps lets a fat-fingered key resolve to
     * the word the user meant (e.g. taps landing on "rhe" -> "the"), rather than
     * treating each tap as a hard, discrete key.
     */
    fun probableWords(
        taps: List<Map<Char, Float>>,
        learned: Map<String, Int>,
        context: Map<String, Int>,
        limit: Int,
    ): List<String> {
        if (taps.isEmpty() || langs.isEmpty()) return emptyList()

        // Beam search over the taps: keep the most spatially-likely prefixes that
        // are still real dictionary prefixes, so cost stays bounded regardless of
        // vocabulary size.
        var beam = listOf(Hyp("", 0f))
        for (dist in taps) {
            val cands = dist.entries.sortedByDescending { it.value }.take(BRANCH)
            if (cands.isEmpty()) break
            val next = ArrayList<Hyp>(beam.size * cands.size)
            for (h in beam) for ((ch, p) in cands) {
                val np = h.prefix + ch
                if (anyStartsWith(np)) next.add(Hyp(np, h.score + ln(maxOf(p, FLOOR))))
            }
            if (next.isEmpty()) break
            next.sortByDescending { it.score }
            beam = if (next.size > BEAM) ArrayList(next.subList(0, BEAM)) else next
        }

        // Complete each surviving prefix to real words and fold in the language
        // model: frequency, a short-word bias, learned use, and context.
        val n = taps.size
        val scored = HashMap<String, Float>()
        for (h in beam) {
            for (lang in langs) for (w in prefixMatches(lang, h.prefix, COMPLETIONS)) {
                val lm = LM_WEIGHT * ln(freqProb(w)) -
                    TAIL_PENALTY * maxOf(0, w.length - n) +
                    LEARN_WEIGHT * ln(1f + (learned[w] ?: 0)) +
                    CONTEXT_WEIGHT * ln(1f + (context[w] ?: 0))
                val s = h.score + lm
                val cur = scored[w]
                if (cur == null || s > cur) scored[w] = s
            }
        }
        return scored.entries.sortedByDescending { it.value }.take(limit).map { it.key }
    }

    /** A frequency-derived probability in (0,1]; unknown words get the floor. */
    private fun freqProb(word: String): Float {
        val r = rankOf(word)
        return if (r == Int.MAX_VALUE) FLOOR else 1f / (1f + r / FREQ_K)
    }

    /** Whether any active-language word starts with [prefix] (bounds the beam). */
    private fun anyStartsWith(prefix: String): Boolean {
        for (l in langs) {
            val i = lowerBound(l.sorted, prefix)
            if (i < l.sorted.size && l.sorted[i].startsWith(prefix)) return true
        }
        return false
    }

    companion object {
        // Noisy-channel tap-decode tuning.
        private const val BRANCH = 3          // key candidates considered per tap
        private const val BEAM = 12           // hypotheses kept between taps
        private const val COMPLETIONS = 6     // completions scored per beam prefix
        private const val FLOOR = 0.03f       // prob floor for an unlikely key/word
        private const val FREQ_K = 2000f      // frequency-rank softness
        private const val LM_WEIGHT = 0.9f    // weight of word frequency (vs spatial fit)
        private const val LEARN_WEIGHT = 0.8f // weight of the user's learned words
        private const val CONTEXT_WEIGHT = 1.2f // weight of previous-word context
        private const val TAIL_PENALTY = 0.25f // mild bias against over-long completions

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
