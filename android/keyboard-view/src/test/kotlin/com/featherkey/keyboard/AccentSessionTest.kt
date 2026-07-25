package com.featherkey.keyboard

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AccentSessionTest {

    @Test fun opening_a_vowel_activates_with_variants() {
        val s = AccentSession()
        assertTrue(s.open('e'))
        assertTrue(s.active)
        assertEquals(listOf("ë", "é", "è", "ê"), s.variants)
        assertEquals(-1, s.index)
    }

    @Test fun opening_a_letter_without_variants_does_not_activate() {
        val s = AccentSession()
        assertFalse(s.open('q'))
        assertFalse(s.active)
    }

    @Test fun release_without_moving_commits_the_base_letter() {
        val s = AccentSession()
        s.open('e')
        assertEquals("e", s.release())
    }

    @Test fun release_after_moving_commits_the_highlighted_variant() {
        val s = AccentSession()
        s.open('e')
        s.moveTo(x = 150f, left = 100f, cellW = 40f) // (150-100)/40 = 1 -> é
        assertEquals("é", s.release())
    }

    @Test fun release_when_inactive_is_null() {
        assertNull(AccentSession().release())
    }

    @Test fun reset_clears_state() {
        val s = AccentSession()
        s.open('e')
        s.reset()
        assertFalse(s.active)
        assertEquals(-1, s.index)
    }
}
