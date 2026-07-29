package com.featherkey.keyboard

/** What the shift key is currently asking for. */
enum class ShiftMode {
    /** Letters commit as typed. */
    OFF,

    /** One-shot: the next letter is upper-cased, then shift clears. Set by a
     *  single shift tap and by auto-capitalization at a sentence start. */
    ONE_SHOT,

    /** Caps lock: every letter is upper-cased until shift is tapped again. */
    LOCKED,
}

/**
 * The shift key's tap state machine, extracted from KeyboardView so the
 * double-tap → caps-lock decision is pure and unit-testable. KeyboardView owns
 * the clock and the MotionEvent plumbing; this owns only the transition.
 *
 * Two taps inside [DOUBLE_TAP_MS] lock — the universal Gboard/iOS convention —
 * and the lock always releases on the next tap, so the key never strands the
 * user in caps. A tap on any other key breaks the double-tap window (the view
 * passes a gap of [Long.MAX_VALUE]), so "shift, type, shift" can never lock by
 * accident.
 */
object ShiftKey {
    /** Longest gap between two shift taps that still reads as a double-tap.
     *  Matches the platform's `ViewConfiguration.getDoubleTapTimeout()`. */
    const val DOUBLE_TAP_MS = 300L

    /**
     * The mode after a shift tap that landed [gapMs] after the previous shift
     * tap ([Long.MAX_VALUE] when no shift tap immediately preceded this one).
     *
     * Note the lock check comes first: a third quick tap releases rather than
     * re-locking, so a triple-tap is not a no-op.
     */
    fun onTap(current: ShiftMode, gapMs: Long): ShiftMode = when {
        current == ShiftMode.LOCKED -> ShiftMode.OFF
        gapMs <= DOUBLE_TAP_MS -> ShiftMode.LOCKED
        current == ShiftMode.ONE_SHOT -> ShiftMode.OFF
        else -> ShiftMode.ONE_SHOT
    }

    /** The mode after a letter is committed: a one-shot is spent, a lock holds.
     *  This is what lets the user type many words in capitals off one double-tap. */
    fun afterLetter(current: ShiftMode): ShiftMode =
        if (current == ShiftMode.ONE_SHOT) ShiftMode.OFF else current

    /**
     * The mode after auto-capitalization re-evaluates the caret ([wantsCaps] is
     * the IME's `AutoCaps.shouldCapitalize` verdict, recomputed on every space,
     * backspace, enter and suggestion pick). Inert under caps lock: a mid-sentence
     * "wants lowercase" must not cancel a lock the user deliberately set.
     */
    fun afterAutoCaps(current: ShiftMode, wantsCaps: Boolean): ShiftMode = when {
        current == ShiftMode.LOCKED -> ShiftMode.LOCKED
        wantsCaps -> ShiftMode.ONE_SHOT
        else -> ShiftMode.OFF
    }
}
