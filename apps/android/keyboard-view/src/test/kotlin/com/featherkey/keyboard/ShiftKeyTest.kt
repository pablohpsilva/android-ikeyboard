package com.featherkey.keyboard

import org.junit.Assert.assertEquals
import org.junit.Test

/** The shift key's tap state machine (double-tap = caps lock). */
class ShiftKeyTest {

    /** No shift tap immediately preceded this one — what KeyboardView passes after
     *  any other key was pressed, and for the very first tap on a fresh field. */
    private val alone = Long.MAX_VALUE

    private fun tap(current: ShiftMode, gapMs: Long = alone) = ShiftKey.onTap(current, gapMs)

    @Test fun a_lone_tap_toggles_the_one_shot() {
        assertEquals(ShiftMode.ONE_SHOT, tap(ShiftMode.OFF))
        assertEquals(ShiftMode.OFF, tap(ShiftMode.ONE_SHOT))
    }

    @Test fun two_quick_taps_lock_caps() {
        val first = tap(ShiftMode.OFF)
        assertEquals(ShiftMode.ONE_SHOT, first)
        assertEquals(ShiftMode.LOCKED, tap(first, ShiftKey.DOUBLE_TAP_MS - 1))
    }

    @Test fun the_double_tap_window_is_inclusive_at_its_edge() {
        assertEquals(ShiftMode.LOCKED, tap(ShiftMode.ONE_SHOT, ShiftKey.DOUBLE_TAP_MS))
        assertEquals(ShiftMode.OFF, tap(ShiftMode.ONE_SHOT, ShiftKey.DOUBLE_TAP_MS + 1))
    }

    @Test fun two_slow_taps_do_not_lock() {
        val first = tap(ShiftMode.OFF)
        assertEquals(ShiftMode.OFF, tap(first, 900L))
    }

    @Test fun a_second_quick_tap_locks_even_when_the_first_disarmed_auto_caps() {
        // Auto-caps armed the one-shot; tap 1 clears it, tap 2 still means "lock".
        val afterFirst = tap(ShiftMode.ONE_SHOT)
        assertEquals(ShiftMode.OFF, afterFirst)
        assertEquals(ShiftMode.LOCKED, tap(afterFirst, 120L))
    }

    @Test fun the_next_tap_always_releases_the_lock() {
        assertEquals(ShiftMode.OFF, tap(ShiftMode.LOCKED, 50L))   // triple-tap
        assertEquals(ShiftMode.OFF, tap(ShiftMode.LOCKED, alone)) // much later
    }

    @Test fun releasing_the_lock_then_tapping_again_re_locks() {
        val released = tap(ShiftMode.LOCKED, 50L)
        assertEquals(ShiftMode.OFF, released)
        assertEquals(ShiftMode.LOCKED, tap(released, 50L))
    }

    // --- Typing under the lock ------------------------------------------------

    @Test fun a_letter_spends_a_one_shot_but_never_the_lock() {
        assertEquals(ShiftMode.OFF, ShiftKey.afterLetter(ShiftMode.ONE_SHOT))
        assertEquals(ShiftMode.LOCKED, ShiftKey.afterLetter(ShiftMode.LOCKED))
        assertEquals(ShiftMode.OFF, ShiftKey.afterLetter(ShiftMode.OFF))
    }

    @Test fun auto_caps_cannot_cancel_the_lock() {
        // applyAutoCaps() runs after every space/backspace/enter and says
        // "lowercase please" mid-sentence — that must not unlock.
        assertEquals(ShiftMode.LOCKED, ShiftKey.afterAutoCaps(ShiftMode.LOCKED, false))
        assertEquals(ShiftMode.LOCKED, ShiftKey.afterAutoCaps(ShiftMode.LOCKED, true))
        // Unlocked, it behaves exactly as before.
        assertEquals(ShiftMode.ONE_SHOT, ShiftKey.afterAutoCaps(ShiftMode.OFF, true))
        assertEquals(ShiftMode.OFF, ShiftKey.afterAutoCaps(ShiftMode.ONE_SHOT, false))
    }

    /** The whole journey the feature exists for: double-tap, type several words in
     *  capitals, then one tap to go back to normal typing. */
    @Test fun double_tap_type_many_words_then_one_tap_returns_to_default() {
        var mode = ShiftMode.OFF

        // Double-tap the shift key.
        mode = ShiftKey.onTap(mode, alone)
        mode = ShiftKey.onTap(mode, 140L)
        assertEquals("two quick taps must lock", ShiftMode.LOCKED, mode)

        // Type "HELLO BIG WORLD": every letter is upper-cased and the lock holds
        // across letters, spaces and the auto-caps pass that follows each space.
        for (ch in "hello big world") {
            if (ch == ' ') {
                mode = ShiftKey.afterAutoCaps(mode, wantsCaps = false) // mid-sentence
            } else {
                assertEquals("letter '$ch' must be capitalised", ShiftMode.LOCKED, mode)
                mode = ShiftKey.afterLetter(mode)
            }
            assertEquals("the lock must survive '$ch'", ShiftMode.LOCKED, mode)
        }

        // One more tap — long after the last one, since the user has been typing.
        mode = ShiftKey.onTap(mode, alone)
        assertEquals("a single tap returns to default", ShiftMode.OFF, mode)

        // And typing is lower-case again.
        assertEquals(ShiftMode.OFF, ShiftKey.afterLetter(mode))
    }

    @Test fun an_intervening_key_press_breaks_the_double_tap() {
        // "shift, a, shift" typed fast: the view reports no preceding shift tap,
        // so the second tap toggles the one-shot back on instead of locking.
        val first = tap(ShiftMode.OFF)
        assertEquals(ShiftMode.ONE_SHOT, tap(ShiftMode.OFF, alone)) // shift after the letter
        assertEquals(ShiftMode.ONE_SHOT, first)
    }
}
