package com.featherkey.keyboard

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class KeyRepeatTest {

    @Test fun first_step_shortens_by_STEP() {
        assertEquals(KeyRepeat.START_MS - KeyRepeat.STEP_MS, KeyRepeat.next(KeyRepeat.START_MS))
    }

    @Test fun never_drops_below_the_floor() {
        assertEquals(KeyRepeat.MIN_MS, KeyRepeat.next(KeyRepeat.MIN_MS))
        assertEquals(KeyRepeat.MIN_MS, KeyRepeat.next(KeyRepeat.MIN_MS + KeyRepeat.STEP_MS / 2))
    }

    @Test fun the_train_accelerates_monotonically_to_the_floor() {
        var d = KeyRepeat.START_MS
        var prev = Long.MAX_VALUE
        // Enough ticks to cover the whole ramp several times over.
        repeat(64) {
            d = KeyRepeat.next(d)
            assertTrue("interval must never grow", d <= prev)
            assertTrue("interval must never pass the floor", d >= KeyRepeat.MIN_MS)
            prev = d
        }
        assertEquals("a long hold settles at the floor", KeyRepeat.MIN_MS, d)
    }

    @Test fun the_curve_is_sane() {
        // A held backspace should start deliberate and end fast, not the reverse.
        assertTrue(KeyRepeat.START_MS > KeyRepeat.MIN_MS)
        assertTrue(KeyRepeat.INITIAL_MS >= KeyRepeat.START_MS)
        assertTrue(KeyRepeat.STEP_MS > 0)
    }
}
