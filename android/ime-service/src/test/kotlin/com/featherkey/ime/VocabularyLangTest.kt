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

    @Test fun candidatesByLanguage_prefers_learned_word_over_more_frequent_one() {
        // "apple" is the most frequent (bundled rank 0), but the user has
        // actually used "apply" a lot — that learned usage must outrank raw
        // bundled frequency within the language.
        val v = Vocabulary.forTest(mapOf("en" to listOf("apple", "apply", "apt")))
        val c = v.candidatesByLanguage("app", mapOf("apply" to 5), emptyMap(), 3)
        assertEquals(0, c.first { it.word == "apply" }.sourceRank)
        assertTrue(
            "apply should rank ahead of apple",
            c.first { it.word == "apply" }.sourceRank < c.first { it.word == "apple" }.sourceRank
        )
    }

    @Test fun candidatesByLanguage_prefers_context_continuation_first() {
        // "apple" is most frequent and "apply" is learned, but a context
        // continuation (the word that usually follows the previous word)
        // must outrank both.
        val v = Vocabulary.forTest(mapOf("en" to listOf("apple", "apply", "approve")))
        val c = v.candidatesByLanguage("app", mapOf("apply" to 5), mapOf("approve" to 9), 3)
        assertEquals(0, c.first { it.word == "approve" }.sourceRank)
    }
}
