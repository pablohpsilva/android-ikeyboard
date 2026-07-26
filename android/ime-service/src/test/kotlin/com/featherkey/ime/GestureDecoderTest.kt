package com.featherkey.ime

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Swipe decoding paths a word through its *typeable* letters only. The keyboard
 * has no apostrophe or accent key, so "I've" is glided i-v-e and "café" c-a-f-e;
 * the decoder must skip the non-key characters when tracing the ideal path
 * instead of dropping the whole word — the bug that made every apostrophe word
 * unreachable by swipe. [GestureDecoder.keyPath] is the pure core of that, so it
 * is tested directly (the full decode uses android.graphics.PointF geometry,
 * which is not functional under plain JUnit).
 */
class GestureDecoderTest {

    // The standard base-letter layout: every a–z key exists, nothing else.
    private val hasLetterKey: (Char) -> Boolean = { it in 'a'..'z' }

    @Test fun apostrophe_words_path_through_their_letters_only() {
        // The apostrophe has no key, so it is skipped — the gesture is i-v-e etc.
        assertEquals(listOf('i', 'v', 'e'), GestureDecoder.keyPath("I've", hasLetterKey))
        assertEquals(listOf('d', 'o', 'n', 't'), GestureDecoder.keyPath("don't", hasLetterKey))
        assertEquals(listOf('h', 'e', 'l', 'l'), GestureDecoder.keyPath("he'll", hasLetterKey))
    }

    @Test fun accented_words_still_fold_to_their_base_keys() {
        assertEquals(listOf('c', 'a', 'f', 'e'), GestureDecoder.keyPath("café", hasLetterKey))
        assertEquals(listOf('v', 'o', 'c', 'e'), GestureDecoder.keyPath("você", hasLetterKey))
        assertEquals(listOf('t', 'a', 'm', 'b', 'e', 'm'), GestureDecoder.keyPath("também", hasLetterKey))
    }

    @Test fun accents_and_apostrophes_are_dropped_together() {
        // A hypothetical mixed token: both the accent and the apostrophe vanish.
        assertEquals(listOf('c', 'e', 's', 't'), GestureDecoder.keyPath("c'est", hasLetterKey))
    }

    @Test fun a_trailing_apostrophe_does_not_become_the_last_key() {
        // "goin'" ends on 'n', not on the apostrophe (which has no key) — so first/
        // last-key pruning uses real letters.
        val keys = GestureDecoder.keyPath("goin'", hasLetterKey)
        assertEquals('n', keys.last())
        assertEquals(listOf('g', 'o', 'i', 'n'), keys)
    }

    @Test fun a_plain_word_is_unchanged() {
        assertEquals(listOf('h', 'e', 'l', 'l', 'o'), GestureDecoder.keyPath("hello", hasLetterKey))
    }
}
