package com.featherkey.ime

import org.junit.Assert.assertEquals
import org.junit.Test

class VocabularyPrefixTest {
    // Frequency rank = index in the list (0 = most common). "sun" is the MOST frequent
    // s-word but alphabetically LAST here — the bound must still pick it over rarer,
    // alphabetically-earlier s-words (proves we select by rank, not by scan position).
    private val vocab = Vocabulary.forTest(
        mapOf("en" to listOf("sun", "sea", "sky", "sad", "set", "sit", "six", "saw", "sir", "ski", "soy", "spa"))
    )

    @Test fun returns_top_k_by_frequency_not_alphabetical_prefix_order() {
        // k=3 completions of "s": the three most frequent are sun(0), sea(1), sky(2).
        val got = vocab.candidatesByLanguage("s", emptyMap(), emptyMap(), 3).map { it.word }
        assertEquals(listOf("sun", "sea", "sky"), got)
    }

    @Test fun a_high_frequency_word_is_never_crowded_out_by_earlier_rarer_matches() {
        // "sun" (rank 0) must appear even though 8 rarer s-words sort before it alphabetically.
        val got = vocab.candidatesByLanguage("s", emptyMap(), emptyMap(), 1).map { it.word }
        assertEquals(listOf("sun"), got)
    }

    @Test fun fewer_matches_than_k_returns_all_of_them() {
        val got = vocab.candidatesByLanguage("sk", emptyMap(), emptyMap(), 5).map { it.word }.sorted()
        assertEquals(listOf("ski", "sky"), got) // only two "sk" words exist
    }
}
