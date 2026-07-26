package com.featherkey.keyboard

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AccentsTest {

    // --- Coverage: every common variant is offered ---------------------------

    @Test fun vowels_offer_the_full_western_european_set() {
        assertEquals(setOf("á", "à", "â", "ã", "ä", "å"), Accents.variantsFor('a').toSet())
        assertEquals(setOf("é", "è", "ê", "ë"), Accents.variantsFor('e').toSet())
        assertEquals(setOf("í", "ì", "î", "ï"), Accents.variantsFor('i').toSet())
        assertEquals(setOf("ó", "ò", "ô", "õ", "ö", "ø"), Accents.variantsFor('o').toSet())
        assertEquals(setOf("ú", "ù", "û", "ü"), Accents.variantsFor('u').toSet())
        assertEquals(setOf("ç"), Accents.variantsFor('c').toSet())
        assertEquals(setOf("ñ"), Accents.variantsFor('n').toSet())
    }

    // --- Ordering follows the primary (accented) language --------------------

    @Test fun default_order_leads_with_the_acute() {
        assertEquals(listOf("á", "à", "â", "ã", "ä", "å"), Accents.variantsFor('a'))
        assertEquals(listOf("ó", "ò", "ô", "õ", "ö", "ø"), Accents.variantsFor('o'))
    }

    @Test fun portuguese_leads_with_acute_then_tilde() {
        assertEquals("á", Accents.variantsFor('a', listOf("pt")).first())
        assertEquals("ã", Accents.variantsFor('a', listOf("pt"))[1])
        assertEquals("ó", Accents.variantsFor('o', listOf("pt")).first())
        assertEquals("õ", Accents.variantsFor('o', listOf("pt"))[1])
        assertEquals("í", Accents.variantsFor('i', listOf("pt")).first())
        assertEquals("ú", Accents.variantsFor('u', listOf("pt")).first())
        assertEquals("é", Accents.variantsFor('e', listOf("pt")).first())
    }

    @Test fun luxembourgish_leads_with_umlaut_and_grave() {
        assertEquals("ä", Accents.variantsFor('a', listOf("lb")).first())
        assertEquals("ë", Accents.variantsFor('e', listOf("lb")).first())
    }

    @Test fun german_leads_with_the_umlaut() {
        assertEquals("ä", Accents.variantsFor('a', listOf("de")).first())
        assertEquals("ö", Accents.variantsFor('o', listOf("de")).first())
        assertEquals("ü", Accents.variantsFor('u', listOf("de")).first())
    }

    @Test fun the_first_accented_language_wins_when_primary_has_no_accents() {
        // English carries no accents, so a secondary Portuguese decides the order.
        assertEquals(
            Accents.variantsFor('a', listOf("pt")),
            Accents.variantsFor('a', listOf("en", "pt")),
        )
    }

    @Test fun a_region_suffix_is_ignored() {
        assertEquals(Accents.variantsFor('a', listOf("pt")), Accents.variantsFor('a', listOf("pt-BR")))
    }

    @Test fun ordering_never_drops_or_duplicates_a_variant() {
        val langSets = listOf(
            emptyList(), listOf("pt"), listOf("lb"), listOf("fr"),
            listOf("de"), listOf("es"), listOf("en", "pt"),
        )
        for (base in listOf('a', 'e', 'i', 'o', 'u', 'c', 'n', 'y', 's')) {
            val full = Accents.variantsFor(base).toSet()
            for (langs in langSets) {
                val v = Accents.variantsFor(base, langs)
                assertEquals("no duplicates for '$base' $langs", v.size, v.toSet().size)
                assertEquals("same superset for '$base' $langs", full, v.toSet())
            }
        }
    }

    @Test fun uppercase_base_maps_same_as_lowercase() {
        assertEquals(Accents.variantsFor('a'), Accents.variantsFor('A'))
        assertEquals(Accents.variantsFor('a', listOf("pt")), Accents.variantsFor('A', listOf("pt")))
    }

    @Test fun letters_without_accents_are_empty() {
        assertTrue(Accents.variantsFor('q').isEmpty())
        assertFalse(Accents.hasVariants('q'))
        assertTrue(Accents.hasVariants('e'))
    }

    // --- Popup hit-test ------------------------------------------------------

    @Test fun hit_test_maps_x_to_cell_index() {
        // Popup left=100, each cell 40 wide, 4 cells → spans [100,260).
        assertEquals(0, Accents.variantIndexAt(110f, 100f, 40f, 4))
        assertEquals(2, Accents.variantIndexAt(185f, 100f, 40f, 4))
        assertEquals(3, Accents.variantIndexAt(255f, 100f, 40f, 4))
    }

    @Test fun hit_test_is_null_outside_the_band() {
        assertNull(Accents.variantIndexAt(90f, 100f, 40f, 4))   // left of band
        assertNull(Accents.variantIndexAt(260f, 100f, 40f, 4))  // right of band
        assertNull(Accents.variantIndexAt(150f, 100f, 40f, 0))  // no variants
    }
}
