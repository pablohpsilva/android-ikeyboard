package com.featherkey.ime

import android.text.InputType
import android.view.inputmethod.EditorInfo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Pure decision logic behind the reported bugs. Android SDK constants used are
 *  compile-time literals, so these run under plain JUnit (no Robolectric). */
class TypingRulesTest {

    // --- Bug: double-space -> period -----------------------------------------

    @Test fun double_space_after_a_word_makes_a_period() {
        assertTrue(PunctuationRules.doubleSpaceMakesPeriod("o ")) // "...casa " + space
        assertTrue(PunctuationRules.doubleSpaceMakesPeriod("5 ")) // after a digit too
    }

    @Test fun double_space_does_not_fire_without_a_word_before_the_space() {
        assertFalse(PunctuationRules.doubleSpaceMakesPeriod(". ")) // already punctuation
        assertFalse(PunctuationRules.doubleSpaceMakesPeriod("  ")) // two spaces
        assertFalse(PunctuationRules.doubleSpaceMakesPeriod("x"))  // no trailing space
        assertFalse(PunctuationRules.doubleSpaceMakesPeriod(null))
    }

    // --- Bug: auto-capitalization (sentence start), precise but flexible ------

    private val text = InputType.TYPE_CLASS_TEXT
    private val password = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
    private val email = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS

    @Test fun caps_at_field_start_even_when_the_field_declares_no_flag() {
        assertTrue(AutoCaps.shouldCapitalize(text, 0, "")) // empty = start of field
        assertTrue(AutoCaps.shouldCapitalize(text, 0, null))
    }

    @Test fun caps_after_a_sentence_terminator_and_space() {
        assertTrue(AutoCaps.shouldCapitalize(text, 0, ". "))
        assertTrue(AutoCaps.shouldCapitalize(text, 0, "! "))
        assertTrue(AutoCaps.shouldCapitalize(text, 0, "\n"))
    }

    @Test fun no_caps_mid_sentence() {
        assertFalse(AutoCaps.shouldCapitalize(text, 0, "o ")) // ordinary word gap
        assertFalse(AutoCaps.shouldCapitalize(text, 0, "lo")) // mid-word
    }

    @Test fun honors_the_fields_own_caps_request() {
        assertTrue(AutoCaps.shouldCapitalize(text, InputType.TYPE_TEXT_FLAG_CAP_WORDS, "o "))
    }

    @Test fun never_capitalizes_password_or_email_fields() {
        assertFalse(AutoCaps.shouldCapitalize(password, 0, ""))
        assertFalse(AutoCaps.shouldCapitalize(email, 0, ". "))
    }

    // --- Bug: Enter is a dead key --------------------------------------------

    @Test fun enter_is_a_newline_in_a_no_action_field() {
        // Samsung Notes body: TEXT | CAP_SENTENCES, action NONE -> newline.
        val notes = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
        assertTrue(EnterKey.insertsNewline(notes, EditorInfo.IME_ACTION_NONE))
    }

    @Test fun enter_is_a_newline_in_a_multiline_field() {
        val multi = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE
        assertTrue(EnterKey.insertsNewline(multi, EditorInfo.IME_ACTION_SEND))
    }

    @Test fun enter_fires_the_action_in_a_single_line_action_field() {
        assertFalse(EnterKey.insertsNewline(InputType.TYPE_CLASS_TEXT, EditorInfo.IME_ACTION_SEARCH))
        assertFalse(EnterKey.insertsNewline(InputType.TYPE_CLASS_TEXT, EditorInfo.IME_ACTION_SEND))
    }

    // --- Bug: fat-finger tap between keys types nothing / wrong letter --------

    private val live = setOf("bring", "cat", "casa")
    private fun isLive(p: String) = live.any { it.startsWith(p) }

    @Test fun returns_the_geometric_best_when_there_is_no_word_in_progress() {
        val cands = listOf("c" to 1.0f, "x" to 0.9f)
        assertEquals("c", TapDisambiguator.choose("c", cands, "", 0.5f, ::isLive))
    }

    @Test fun keeps_best_when_it_already_spells_a_word_even_if_a_rival_would_too() {
        // prefix "ca": best 's' -> "cas" is live (casa); it is kept, no override.
        val cands = listOf("s" to 1.0f, "t" to 0.9f)
        assertEquals("s", TapDisambiguator.choose("s", cands, "ca", 0.5f, ::isLive))
    }

    @Test fun rescues_a_dead_best_toward_a_near_confidence_live_neighbor() {
        // Prefix "brin"; best 'g' spells "bring" (live). Here best is 'h' (dead:
        // "brinh"), rival 'g' at 0.8*best keeps "bring" alive -> choose 'g'.
        val cands = listOf("h" to 1.0f, "g" to 0.8f)
        assertEquals("g", TapDisambiguator.choose("h", cands, "brin", 0.5f, ::isLive))
    }

    @Test fun does_not_rescue_toward_a_far_low_confidence_neighbor() {
        // 'g' would help but its confidence is far below best -> leave best as typed.
        val cands = listOf("h" to 1.0f, "g" to 0.2f)
        assertEquals("h", TapDisambiguator.choose("h", cands, "brin", 0.5f, ::isLive))
    }

    // --- Bug: picked suggestion / swipe drops the sentence-start capital ------

    @Test fun case_match_carries_a_leading_capital_to_the_replacement() {
        assertEquals("Hello", CaseMatch.matchLeading("Hel", "hello"))
        assertEquals("Também", CaseMatch.matchLeading("Tambem", "também"))
        assertEquals("hello", CaseMatch.matchLeading("hel", "hello")) // lowercase stays
    }

    // --- Bug: accented/contracted forms never show in the suggestion strip ----

    @Test fun guaranteed_variant_is_inserted_after_the_top_prediction() {
        // Typing "hell": the strip is full of commoner words; "he'll" must still
        // be offered, right after the leading prediction, dropping the overflow.
        val strip = SuggestionStrip.withGuaranteedVariant(
            ranked = listOf("hello", "hell", "help"),
            variants = listOf("he'll"),
            cap = 3,
        )
        assertEquals(listOf("hello", "he'll", "hell"), strip)
    }

    @Test fun guaranteed_variant_is_a_noop_when_already_shown() {
        val strip = SuggestionStrip.withGuaranteedVariant(
            ranked = listOf("I've", "ivy", "ive"),
            variants = listOf("I've"),
            cap = 3,
        )
        assertEquals(listOf("I've", "ivy", "ive"), strip) // unchanged
    }

    @Test fun guaranteed_variant_matches_case_insensitively_before_inserting() {
        // "It's" already present as "it's" — don't add a duplicate.
        val strip = SuggestionStrip.withGuaranteedVariant(
            ranked = listOf("its", "it's"),
            variants = listOf("It's"),
            cap = 3,
        )
        assertEquals(listOf("its", "it's"), strip)
    }

    @Test fun guaranteed_variant_handles_empty_and_short_strips() {
        assertEquals(
            listOf("você"),
            SuggestionStrip.withGuaranteedVariant(emptyList(), listOf("você"), 3),
        )
        assertEquals(
            listOf("voce", "você"),
            SuggestionStrip.withGuaranteedVariant(listOf("voce"), listOf("você"), 3),
        )
        // No variant to add: just capped and de-duped.
        assertEquals(
            listOf("a", "b", "c"),
            SuggestionStrip.withGuaranteedVariant(listOf("a", "b", "c", "d"), emptyList(), 3),
        )
    }

    // --- Bug: emoji needs two backspaces to delete ---------------------------
    // A single backspace must remove one grapheme cluster, not one UTF-16 code
    // unit; an emoji is a surrogate pair (2 units) so a code-unit delete left an
    // orphaned half that required a second press. BreakIterator is ICU-backed on
    // Android, so skin-tone/flag/ZWJ clusters also delete whole on-device; the
    // host JDK under-segments those, so we only assert the surrogate-pair symptom.

    @Test fun backspace_deletes_a_whole_emoji_not_half_a_surrogate_pair() {
        assertEquals(2, GraphemeDeletion.lastClusterLength("😀"))   // 1 emoji = 2 UTF-16 units
        assertEquals(2, GraphemeDeletion.lastClusterLength("hi😀")) // trailing emoji after text
    }

    @Test fun backspace_deletes_one_ordinary_character() {
        assertEquals(1, GraphemeDeletion.lastClusterLength("hello")) // last cluster "o"
        assertEquals(1, GraphemeDeletion.lastClusterLength("a"))
    }

    @Test fun backspace_deletes_a_base_plus_combining_mark_together() {
        // Decomposed e + combining acute (U+0301) is one cluster: both units
        // go on a single press.
        assertEquals(2, GraphemeDeletion.lastClusterLength("e\u0301"))
        // A precomposed accented letter (U+00E9) is a single unit.
        assertEquals(1, GraphemeDeletion.lastClusterLength("caf\u00E9"))
    }

    @Test fun backspace_on_empty_or_null_deletes_nothing() {
        assertEquals(0, GraphemeDeletion.lastClusterLength(""))
        assertEquals(0, GraphemeDeletion.lastClusterLength(null))
    }
}
