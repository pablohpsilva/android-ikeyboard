package com.featherkey.ime

import org.junit.Assert.assertEquals
import org.junit.Test

class DiacriticsTest {
    @Test fun folds_portuguese_accents_to_base_letters() {
        assertEquals("tambem", Diacritics.fold("também"))
        assertEquals("acao", Diacritics.fold("ação"))
        assertEquals("voce", Diacritics.fold("você"))
    }

    @Test fun folds_cedilla_and_french_accents() {
        assertEquals("franca", Diacritics.fold("frança"))
        assertEquals("cafe", Diacritics.fold("café"))
        assertEquals("etre", Diacritics.fold("être"))
    }

    @Test fun folds_to_lowercase_so_it_is_a_case_insensitive_match_key() {
        assertEquals("sao", Diacritics.fold("São"))
        assertEquals("hello", Diacritics.fold("Hello"))
    }

    @Test fun strips_apostrophes_so_contractions_match_base_typing() {
        assertEquals("im", Diacritics.fold("I'm"))
        assertEquals("ive", Diacritics.fold("I've"))
        assertEquals("dont", Diacritics.fold("don't"))
        assertEquals("its", Diacritics.fold("it's")) // same key as the word "its"
        assertEquals("dont", Diacritics.fold("don’t")) // curly apostrophe too
    }

    @Test fun leaves_plain_lowercase_ascii_untouched() {
        assertEquals("hello", Diacritics.fold("hello"))
        assertEquals("", Diacritics.fold(""))
    }

    @Test fun folds_single_characters_to_their_base_key() {
        assertEquals('e', Diacritics.foldChar('é'))
        assertEquals('c', Diacritics.foldChar('ç'))
        assertEquals('a', Diacritics.foldChar('ã'))
        assertEquals('a', Diacritics.foldChar('a')) // ASCII passes through
    }
}
