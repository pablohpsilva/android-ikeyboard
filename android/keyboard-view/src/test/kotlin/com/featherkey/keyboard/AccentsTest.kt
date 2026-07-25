package com.featherkey.keyboard

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AccentsTest {

    @Test fun e_offers_luxembourgish_accents_first() {
        assertEquals(listOf("ë", "é", "è", "ê"), Accents.variantsFor('e'))
    }

    @Test fun uppercase_base_maps_same_as_lowercase() {
        assertEquals(Accents.variantsFor('a'), Accents.variantsFor('A'))
    }

    @Test fun letters_without_accents_are_empty() {
        assertTrue(Accents.variantsFor('q').isEmpty())
        assertFalse(Accents.hasVariants('q'))
        assertTrue(Accents.hasVariants('e'))
    }

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
