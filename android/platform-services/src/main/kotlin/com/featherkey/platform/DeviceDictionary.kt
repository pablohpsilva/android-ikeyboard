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
 * For the *primary* active language it answers the two questions the input path
 * needs:
 *   - is a typed word a real word? (so autocorrect never clobbers it)
 *   - what are the corrections/completions for a partial or mistyped word?
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
) : SpellCheckerSession.SpellCheckerSessionListener {

    private val tsm =
        context.getSystemService(Context.TEXT_SERVICES_MANAGER_SERVICE) as? TextServicesManager

    private var session: SpellCheckerSession? = null
    private var language: String? = null

    /** The word the outstanding/most-recent lookup was fired for. */
    private var queried: String = ""
    /** Corrections/completions for [queried], ranked best-first. */
    @Volatile private var results: List<String> = emptyList()
    /** [queried] iff the device confirmed it is a real word, else null. */
    @Volatile private var confirmed: String? = null

    /**
     * Point the dictionary at [tag]'s language (e.g. "ru", "el", "fr"). A no-op
     * when the language is unchanged; otherwise it reopens the session and drops
     * any cached results. A tag with no supported spell checker yields no
     * session, so every query below simply no-ops and the caller falls back to
     * its bundled lists.
     */
    fun setPrimary(tag: String) {
        val lang = Locale.forLanguageTag(tag).language.ifEmpty { tag }
        if (lang == language && session != null) return
        language = lang
        session?.close()
        // referToSpellCheckerLanguageSettings = false: bind the exact locale we
        // chose, regardless of the user's spell-check language settings.
        session = runCatching {
            tsm?.newSpellCheckerSession(null, Locale(lang), this, false)
        }.getOrNull()
        queried = ""; results = emptyList(); confirmed = null
    }

    /** Fire an async lookup for [word], unless it matches the last one queried. */
    fun refresh(word: String) {
        val s = session ?: return
        if (word.isEmpty() || word == queried) return
        queried = word
        runCatching { s.getSentenceSuggestions(arrayOf(TextInfo(word)), MAX_PER_WORD) }
    }

    /** Best-effort: did the device confirm [word] is a real word? */
    fun isKnown(word: String): Boolean = word.isNotEmpty() && word == confirmed

    /** Cached corrections/completions for the last queried word (ranked). */
    fun suggestions(): List<String> = results

    override fun onGetSentenceSuggestions(sentences: Array<out SentenceSuggestionsInfo>?) {
        val out = LinkedHashSet<String>()
        var known: String? = null
        sentences?.forEach { sentence ->
            for (i in 0 until sentence.suggestionsCount) {
                val info = sentence.getSuggestionsInfoAt(i)
                if (info.suggestionsAttributes and SuggestionsInfo.RESULT_ATTR_IN_THE_DICTIONARY != 0) {
                    known = queried
                }
                for (j in 0 until info.suggestionsCount) out.add(info.getSuggestionAt(j))
            }
        }
        results = out.toList()
        confirmed = known
        onResult()
    }

    // Legacy single-word callback; we use the sentence API above, so this is
    // never invoked, but the listener interface requires it.
    override fun onGetSuggestions(results: Array<out SuggestionsInfo>?) = Unit

    fun close() {
        session?.close()
        session = null
    }

    private companion object {
        /** Corrections requested per word — a short, strip-sized list. */
        const val MAX_PER_WORD = 5
    }
}
