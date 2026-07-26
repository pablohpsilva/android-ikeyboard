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

    @Test fun preserves_case_while_folding() {
        assertEquals("Sao", Diacritics.fold("São"))
    }

    @Test fun leaves_plain_ascii_untouched() {
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
