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
import com.featherkey.keyboard.InitialPage

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

    /**
     * Whether the word being committed *began* a new sentence, given the text
     * before the caret ([before], whose tail still holds the word's own
     * [wordLen] characters). True at field start, or when the text preceding the
     * word ends a sentence ('.'/'!'/'?', optionally + trailing spaces, or a
     * newline). Proper-noun capitalization (BR-69) uses this to defer sentence
     * starts to auto-capitalization rather than fight it.
     */
    fun precedingWordStartsSentence(before: CharSequence?, wordLen: Int): Boolean {
        if (before.isNullOrEmpty()) return true
        val prefix = before.dropLast(wordLen).trimEnd(' ')
        if (prefix.isEmpty()) return true
        return when (prefix.last()) {
            '\n', '.', '!', '?' -> true
            else -> false
        }
    }
}

/** Which initial layout a field should present, from its inputType. Pure so it
 *  unit-tests off-device like its siblings above. */
object FieldLayout {
    /** Which page a field should open on, from its inputType. Number and phone
     *  fields get the telephone dialpad; date/time keeps the 123 numbers page (it
     *  needs / : - separators the dialpad lacks); everything else opens on letters.
     *  Covers numeric-PIN password fields (TYPE_CLASS_NUMBER → dialpad). */
    fun initialPage(inputType: Int): InitialPage =
        when (inputType and InputType.TYPE_MASK_CLASS) {
            InputType.TYPE_CLASS_NUMBER,
            InputType.TYPE_CLASS_PHONE -> InitialPage.DIALPAD
            InputType.TYPE_CLASS_DATETIME -> InitialPage.NUMBERS
            else -> InitialPage.LETTERS
        }

    /** Punctuation keys to flank the space bar on the letter page for this field:
     *  [leftOfSpace, rightOfSpace], or empty for fields that need none. Email
     *  addresses always carry "@" and "."; URLs carry "." and "/". Only text-class
     *  fields qualify (number/symbol pages already carry these characters). */
    fun affixKeys(inputType: Int): List<String> {
        if (inputType and InputType.TYPE_MASK_CLASS != InputType.TYPE_CLASS_TEXT) {
            return emptyList()
        }
        return when (inputType and InputType.TYPE_MASK_VARIATION) {
            InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS,
            InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS -> listOf("@", ".")
            InputType.TYPE_TEXT_VARIATION_URI -> listOf(".", "/")
            else -> emptyList()
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

/** Backspace must remove a whole *grapheme cluster*, not one UTF-16 code unit.
 *  An emoji is a surrogate pair (2 units) — and skin-tone, flag, and ZWJ
 *  sequences are longer still — so a code-unit delete left an orphaned half that
 *  needed a second press to clear. */
object GraphemeDeletion {
    /** UTF-16 code-unit length of the last grapheme cluster in [before] (the text
     *  immediately preceding the cursor), i.e. how many units one backspace should
     *  delete. 0 for null/empty so nothing is deleted. On Android `BreakIterator`
     *  is ICU-backed, so it segments emoji ZWJ/skin-tone/flag clusters correctly;
     *  the host JVM segments at least surrogate pairs and combining marks. */
    fun lastClusterLength(before: CharSequence?): Int {
        if (before.isNullOrEmpty()) return 0
        val s = before.toString()
        val it = java.text.BreakIterator.getCharacterInstance()
        it.setText(s)
        val end = it.last() // == s.length
        val start = it.previous() // boundary starting the final cluster
        return if (start == java.text.BreakIterator.DONE) s.length else (end - start).coerceAtLeast(1)
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

    /** [target] recased to match [source]'s casing as a whole: an all-caps [source]
     *  (what caps lock produces — "HELL") gives an all-caps result ("HELLO"),
     *  otherwise this is [matchLeading]. Two cased characters are required before
     *  all-caps is inferred, so a lone sentence-start "I" still completes to "I'm"
     *  rather than "I'M". */
    fun matchCase(source: String, target: String): String {
        val cased = source.filter { it.isLetter() }
        val allCaps = cased.length >= 2 && cased.all { it.isUpperCase() }
        return if (allCaps) target.uppercase() else matchLeading(source, target)
    }
}
