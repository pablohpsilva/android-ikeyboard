package com.featherkey.platform

/*
 * The device's own dictionary, used as the base vocabulary for languages we do
 * not bundle a word list for — notably the non-Latin scripts (Russian, Greek)
 * and any other of the ~90 locales the installed spell checker covers.
 *
 * It is Android's TextServices spell checker (AOSP/Gboard's
 * AndroidSpellCheckerService), reached through TextServicesManager. Fully
 * on-device and offline: the spell checker reads its own bundled dictionaries,
 * never the network — consistent with FeatherKey's no-network invariant.
 *
 * One session per active language is kept open (diffed via [SessionPlan]), so
 * every active language gets its own answers for the two questions the input
 * path needs:
 *   - is a typed word a real word in language X? (so autocorrect never
 *     clobbers it)
 *   - what are the corrections/completions for a partial or mistyped word in
 *     language X?
 *
 * Results arrive asynchronously; [onResult] fires on the main thread when a
 * lookup completes, letting the caller refresh the suggestion strip (mirroring
 * how the bundled vocabulary refreshes the strip when its async load finishes).
 *
 * PRIVACY: the caller MUST NOT query in a password/secure field. The queried
 * word is handed to the system spell-checker service (another process), so
 * secure text must never be sent here (E-2 / BR-26).
 */

import android.content.Context
import android.view.textservice.SentenceSuggestionsInfo
import android.view.textservice.SpellCheckerSession
import android.view.textservice.SuggestionsInfo
import android.view.textservice.TextInfo
import android.view.textservice.TextServicesManager
import java.util.Locale

class DeviceDictionary(
    context: Context,
    private val onResult: () -> Unit,
) {

    private val tsm =
        context.getSystemService(Context.TEXT_SERVICES_MANAGER_SERVICE) as? TextServicesManager

    /** One session per active language, keyed by the bare language code. */
    private val sessions = LinkedHashMap<String, SpellCheckerSession>()

    /** The word the outstanding/most-recent lookup was fired for. */
    private var queried: String = ""
    /** Corrections/completions per language for [queried], ranked best-first. */
    @Volatile private var buckets: Map<String, List<String>> = emptyMap()
    /** Languages that confirmed [queried] as a real word. */
    @Volatile private var knownIn: Map<String, Set<String>> = emptyMap()

    /**
     * Point the dictionary at [tags]'s languages (e.g. "ru", "el", "fr"). Diffs
     * against the currently open sessions via [SessionPlan]: unchanged
     * languages keep their session (and cached results); removed languages are
     * closed; added languages are opened. A tag with no supported spell
     * checker yields no session, so every query below simply no-ops for that
     * language and the caller falls back to its bundled lists.
     */
    fun setLanguages(tags: List<String>) {
        val plan = SessionPlan.of(sessions.keys.toSet(), tags)
        for (lang in plan.close) { sessions.remove(lang)?.close() }
        for (lang in plan.open) {
            // referToSpellCheckerLanguageSettings = false: bind the exact
            // locale we chose, regardless of the user's spell-check language
            // settings.
            val s = runCatching {
                tsm?.newSpellCheckerSession(null, Locale(lang), Listener(lang), false)
            }.getOrNull()
            if (s != null) sessions[lang] = s
        }
        queried = ""; buckets = emptyMap(); knownIn = emptyMap()
    }

    /** Fire an async lookup for [word] on every open session, unless it matches
     * the last one queried. */
    fun refresh(word: String) {
        if (word.isEmpty() || word == queried) return
        queried = word
        for (s in sessions.values) runCatching { s.getSentenceSuggestions(arrayOf(TextInfo(word)), MAX_PER_WORD) }
    }

    /** Cached corrections/completions for the last queried word, per language. */
    fun candidatesByLanguage(): Map<String, List<String>> = buckets

    /** Languages that confirmed [word] is a real word (best-effort). */
    fun knownLanguages(word: String): Set<String> =
        if (word.isNotEmpty()) knownIn.filterValues { it.contains(word) }.keys else emptySet()

    /** One listener instance per language: the callback itself carries no
     * session identity, so we close over [lang] to land results in the right
     * per-language bucket. */
    private inner class Listener(private val lang: String) : SpellCheckerSession.SpellCheckerSessionListener {
        override fun onGetSentenceSuggestions(sentences: Array<out SentenceSuggestionsInfo>?) {
            val out = LinkedHashSet<String>()
            val known = LinkedHashSet<String>()
            sentences?.forEach { s ->
                for (i in 0 until s.suggestionsCount) {
                    val info = s.getSuggestionsInfoAt(i)
                    if (info.suggestionsAttributes and SuggestionsInfo.RESULT_ATTR_IN_THE_DICTIONARY != 0) known.add(queried)
                    for (j in 0 until info.suggestionsCount) out.add(info.getSuggestionAt(j))
                }
            }
            buckets = buckets + (lang to out.toList())
            knownIn = knownIn + (lang to known)
            onResult()
        }

        // Legacy single-word callback; we use the sentence API above, so this
        // is never invoked, but the listener interface requires it.
        override fun onGetSuggestions(results: Array<out SuggestionsInfo>?) = Unit
    }

    fun close() { sessions.values.forEach { it.close() }; sessions.clear() }

    private companion object {
        /** Corrections requested per word — a short, strip-sized list. */
        const val MAX_PER_WORD = 5
    }
}
