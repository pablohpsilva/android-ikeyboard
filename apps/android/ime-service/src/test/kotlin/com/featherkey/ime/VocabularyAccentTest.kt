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

    @Test fun accented_canonical_upgrades_an_unaccented_typo() {
        assertEquals("também", vocab.accentedCanonical("tambem"))
        assertEquals("você", vocab.accentedCanonical("voce"))
    }

    @Test fun accented_canonical_leaves_the_already_best_form_alone() {
        assertNull(vocab.accentedCanonical("também")) // already the top spelling
        assertNull(vocab.accentedCanonical("casa"))   // no accented twin exists
    }

    @Test fun contractions_are_restored_from_base_typing_but_never_clobber_a_real_word() {
        // "I'm"/"don't" outrank their apostrophe-less spellings; "its" (the word)
        // outranks "it's", so a real word is never rewritten.
        val en = Vocabulary.forTest(
            mapOf("en" to listOf("I'm", "don't", "its", "im", "dont", "it's"))
        )
        assertEquals("I'm", en.accentedCanonical("im"))   // im -> I'm
        assertEquals("don't", en.accentedCanonical("dont")) // dont -> don't
        assertNull(en.accentedCanonical("its"))            // its stays its (no clobber)
    }

    @Test fun accent_variants_surface_the_other_spellings_in_the_fold_group() {
        // Typing the base letters "voce"/"tambem" must offer the accented word as
        // a strip variant even though those exact letters aren't the dictionary form.
        assertEquals(listOf("você"), vocab.accentVariantsOf("voce"))
        assertEquals(listOf("também"), vocab.accentVariantsOf("tambem"))
    }

    @Test fun accent_variants_offer_contractions_even_when_a_plain_twin_outranks_them() {
        // "hell" and "its" are commoner than "he'll"/"it's" and would fill every
        // strip slot on frequency alone — the fold group still yields the variant.
        val en = Vocabulary.forTest(
            mapOf("en" to listOf("hello", "hell", "its", "he'll", "it's", "I've"))
        )
        assertEquals(listOf("he'll"), en.accentVariantsOf("hell")) // not "hell"/"hello"
        assertEquals(listOf("it's"), en.accentVariantsOf("its"))
        assertEquals(listOf("I've"), en.accentVariantsOf("ive"))
    }

    @Test fun accent_variants_are_empty_when_the_typed_word_is_the_only_spelling() {
        assertTrue(vocab.accentVariantsOf("casa").isEmpty()) // no accented twin
        assertTrue(vocab.accentVariantsOf("hello").isEmpty())
        assertTrue(vocab.accentVariantsOf("").isEmpty())
    }

    @Test fun accent_variants_rank_the_most_frequent_variant_first() {
        // Two words share a fold ("cafe"): the commoner one leads.
        val fr = Vocabulary.forTest(mapOf("fr" to listOf("café", "cafés")))
        // Typing "cafes" offers "cafés"; typing "cafe" offers "café".
        assertEquals(listOf("café"), fr.accentVariantsOf("cafe"))
        assertEquals(listOf("cafés"), fr.accentVariantsOf("cafes"))
    }

    @Test fun has_word_prefix_probes_accent_insensitively_for_fat_finger_rescue() {
        assertTrue(vocab.hasWordPrefix("cas"))    // "casa" continues it
        assertTrue(vocab.hasWordPrefix("tamb"))   // "também" continues it (folded)
        assertTrue(vocab.hasWordPrefix(""))       // empty prefix is trivially alive
        // A dead end: no word begins with "casx".
        org.junit.Assert.assertFalse(vocab.hasWordPrefix("casx"))
    }
}
