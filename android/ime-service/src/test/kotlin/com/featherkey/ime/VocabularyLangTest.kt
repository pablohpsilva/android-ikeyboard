package com.featherkey.ime

import org.junit.Assert.*
import org.junit.Test

class VocabularyLangTest {
    // Vocabulary.load needs a Context; expose a test constructor instead.
    @Test fun languagesOf_reports_each_language_containing_the_word() {
        val v = Vocabulary.forTest(mapOf("en" to listOf("no", "yes"), "es" to listOf("no", "hola")))
        assertEquals(setOf("en", "es"), v.languagesOf("no"))
        assertEquals(setOf("es"), v.languagesOf("hola"))
    }

    @Test fun candidatesByLanguage_ranks_within_each_language() {
        val v = Vocabulary.forTest(mapOf("es" to listOf("hola", "hombre", "hoy")))
        val c = v.candidatesByLanguage("ho", emptyMap(), emptyMap(), 3)
        assertTrue(c.all { it.lang == "es" })
        assertEquals(0, c.first { it.word == "hola" }.sourceRank)
    }
}
