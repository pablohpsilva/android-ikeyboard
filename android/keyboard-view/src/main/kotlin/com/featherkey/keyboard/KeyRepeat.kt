package com.featherkey.keyboard

/**
 * Timing curve for a held key that auto-repeats (currently backspace). The first
 * delete fires immediately on touch-down; after [INITIAL_MS] the repeat train
 * begins at [START_MS] and accelerates by [STEP_MS] each tick down to the
 * [MIN_MS] floor, so holding backspace clears text ever faster the longer it is
 * held. Pure (no Android types) so the acceleration is unit-testable off-device.
 */
object KeyRepeat {
    /** Delay from the first (immediate) delete until the repeat train starts. */
    const val INITIAL_MS = 350L
    /** Interval of the first repeat, before any acceleration. */
    const val START_MS = 90L
    /** Fastest interval the train accelerates to (the floor). */
    const val MIN_MS = 28L
    /** How much each successive interval shortens, until it hits [MIN_MS]. */
    const val STEP_MS = 8L

    /** The interval that follows one of [current] ms, clamped to the [MIN_MS] floor. */
    fun next(current: Long): Long = (current - STEP_MS).coerceAtLeast(MIN_MS)
}
