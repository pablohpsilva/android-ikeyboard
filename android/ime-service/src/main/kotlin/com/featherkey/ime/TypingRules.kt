package com.featherkey.ime

/*
 * Pure decision logic for the input path, split out so it is unit-testable
 * without a live InputConnection or the native bridge. Each object answers one
 * question the IME service asks on a keystroke; the service supplies the live
 * inputs (text before the cursor, the field's inputType/imeOptions, the decoder's
 * candidates) and applies the answer. Android SDK constants used here are
 * compile-time literals, so these functions run in a plain JVM unit test.
 */

import android.text.InputType
import android.view.inputmethod.EditorInfo

/** Double-space → ". " (Gboard/iOS convention). */
object PunctuationRules {
    /** True when a space press should replace the preceding space with ". " —
     *  i.e. the two characters before the cursor are "<letter-or-digit><space>".
     *  [before] is the up-to-two characters immediately before the cursor. */
    fun doubleSpaceMakesPeriod(before: CharSequence?): Boolean {
        if (before == null || before.length < 2) return false
        return before[before.length - 1] == ' ' && before[before.length - 2].isLetterOrDigit()
    }
}

/** Auto-capitalization: precise (never in password/email/URL) but flexible
 *  (works even when the editor declares no caps flag). */
object AutoCaps {
    /** Ordinary text entry only — never password/email/URL/number fields, where a
     *  leading capital would be wrong. */
    fun isCapitalizableTextField(inputType: Int): Boolean {
        if (inputType and InputType.TYPE_MASK_CLASS != InputType.TYPE_CLASS_TEXT) return false
        return when (inputType and InputType.TYPE_MASK_VARIATION) {
            InputType.TYPE_TEXT_VARIATION_PASSWORD,
            InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD,
            InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD,
            InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS,
            InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS,
            InputType.TYPE_TEXT_VARIATION_URI,
            InputType.TYPE_TEXT_VARIATION_FILTER -> false
            else -> true
        }
    }

    /**
     * Whether the next letter should be capitalized. Honours the field's own
     * request first ([declaredCapsMode] = the value of getCursorCapsMode, non-zero
     * ⇒ cap here); otherwise falls back to detecting a sentence start from [before]
     * (start of field, after a newline, or after ". "/"! "/"? ") so capitalization
     * works even in editors that set no caps flag.
     */
    fun shouldCapitalize(inputType: Int, declaredCapsMode: Int, before: CharSequence?): Boolean {
        if (!isCapitalizableTextField(inputType)) return false
        if (declaredCapsMode != 0) return true
        if (before.isNullOrEmpty()) return true // start of the field
        return when (before[before.length - 1]) {
            '\n' -> true
            ' ' -> before.length >= 2 && before[before.length - 2].let { it == '.' || it == '!' || it == '?' }
            else -> false
        }
    }
}

/** What the Enter key should do in the active field. */
object EnterKey {
    /** True when Enter should insert a literal newline — a multi-line field, or one
     *  that requests no real editor action (IME_ACTION_NONE/UNSPECIFIED, as note
     *  bodies do). False when it should fire the field's action (Search/Send/Go/
     *  Next/Done). Guarantees Enter is never a dead key. */
    fun insertsNewline(inputType: Int, imeOptions: Int): Boolean {
        val multiLine = inputType and InputType.TYPE_TEXT_FLAG_MULTI_LINE != 0
        val action = imeOptions and EditorInfo.IME_MASK_ACTION
        val hasRealAction = action != EditorInfo.IME_ACTION_NONE &&
            action != EditorInfo.IME_ACTION_UNSPECIFIED
        return multiLine || !hasRealAction
    }
}

/** Fat-finger / low-vision tap disambiguation using word-continuation context. */
object TapDisambiguator {
    /**
     * The key a touch most likely meant. Starts from the decoder's geometric
     * [best]; but when a word is in progress ([prefix]) and best would dead-end it
     * (no dictionary word continues prefix+best), prefer a near-confidence rival
     * candidate — within [ratio] of best's confidence — that keeps a word alive.
     * [candidates] are (key, confidence) best-first; [isLivePrefix] answers whether
     * any dictionary word starts with the given string. Purely additive: with no
     * prefix, a single candidate, or no live rival, returns [best] unchanged.
     */
    fun choose(
        best: String,
        candidates: List<Pair<String, Float>>,
        prefix: String,
        ratio: Float,
        isLivePrefix: (String) -> Boolean,
    ): String {
        if (prefix.isEmpty() || candidates.size < 2) return best
        if (isLivePrefix(prefix + best)) return best
        val bestConf = candidates.firstOrNull { it.first == best }?.second ?: 0f
        val alt = candidates.firstOrNull { (key, conf) ->
            key != best && conf >= bestConf * ratio && key.length == 1 && isLivePrefix(prefix + key)
        }
        return alt?.first ?: best
    }
}

/** Composing the suggestion strip so accented/contracted forms are never crowded out. */
object SuggestionStrip {
    /**
     * The [ranked] strip (frequency/momentum order, capacity [cap]) with the
     * best accent/apostrophe [variant] of the typed token guaranteed a slot.
     *
     * The problem this solves: an accented or contracted form (he'll, você, I've)
     * shares its base letters with a commoner plain word (hell, voce-the-typo,
     * "ive"), so on pure ranking the plain twin fills every slot and the variant
     * never appears. Here the best not-already-shown [variant] is inserted just
     * after the top prediction — so the most likely word still leads, but the
     * accented/contracted alternative is always offered within the [cap] slots.
     * Existing order is otherwise preserved and entries are de-duped
     * case-insensitively. No-op when [variants] is empty or all already shown.
     */
    fun withGuaranteedVariant(ranked: List<String>, variants: List<String>, cap: Int): List<String> {
        val shown = ranked.mapTo(HashSet()) { it.lowercase() }
        val variant = variants.firstOrNull { it.lowercase() !in shown }
            ?: return dedupCap(ranked, cap)
        val out = ArrayList(ranked)
        out.add(minOf(1, out.size), variant)
        return dedupCap(out, cap)
    }

    private fun dedupCap(words: List<String>, cap: Int): List<String> {
        val seen = HashSet<String>()
        return words.filter { seen.add(it.lowercase()) }.take(cap)
    }
}

/** Case-matching for auto-corrections/accent restorations. */
object CaseMatch {
    /** [target] recased to match [source]'s leading case: if [source] starts with
     *  an uppercase letter, [target] is capitalized; otherwise returned as-is. Used
     *  so a picked suggestion or accented form keeps the word's sentence-start
     *  capital ("Hel" → "Hello", "tambem" → "também"). */
    fun matchLeading(source: String, target: String): String =
        if (source.isNotEmpty() && source[0].isUpperCase()) target.replaceFirstChar { it.uppercaseChar() }
        else target
}
