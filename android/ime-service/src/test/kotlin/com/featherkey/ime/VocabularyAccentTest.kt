package com.featherkey.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class VocabularyAccentTest {
    // Ranks = list order: também(0) casa(1) voce/você… — também outranks its
    // unaccented twin tambem, exactly as in the real pt frequency list.
    private val vocab = Vocabulary.forTest(
        mapOf("pt" to listOf("também", "casa", "você", "voce", "tambem"))
    )

    @Test fun base_letter_prefix_surfaces_the_accented_word() {
        // Typing "tambe" (no key for 'é') must still complete to "também".
        val got = vocab.candidatesByLanguage("tambe", emptyMap(), emptyMap(), 3).map { it.word }
        assertTrue("expected também in $got", got.contains("também"))
        assertEquals("também", got.first()) // and it ranks above its unaccented twin
    }

    @Test fun accented_canonical_upgrades_an_unaccented_typo() {
        assertEquals("também", vocab.accentedCanonical("tambem"))
        assertEquals("você", vocab.accentedCanonical("voce"))
    }

    @Test fun accented_canonical_leaves_the_already_best_form_alone() {
        assertNull(vocab.accentedCanonical("também")) // already the top spelling
        assertNull(vocab.accentedCanonical("casa"))   // no accented twin exists
    }

    @Test fun accent_insensitive_prefix_still_matches_when_typed_with_the_accent() {
        val got = vocab.candidatesByLanguage("també", emptyMap(), emptyMap(), 3).map { it.word }
        assertTrue(got.contains("também"))
    }

    @Test fun has_word_prefix_probes_accent_insensitively_for_fat_finger_rescue() {
        assertTrue(vocab.hasWordPrefix("cas"))    // "casa" continues it
        assertTrue(vocab.hasWordPrefix("tamb"))   // "também" continues it (folded)
        assertTrue(vocab.hasWordPrefix(""))       // empty prefix is trivially alive
        // A dead end: no word begins with "casx".
        org.junit.Assert.assertFalse(vocab.hasWordPrefix("casx"))
    }
}
