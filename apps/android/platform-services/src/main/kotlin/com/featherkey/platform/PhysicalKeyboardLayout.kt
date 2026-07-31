package com.featherkey.platform

import android.view.InputDevice
import android.view.KeyEvent

/**
 * Fingerprints an attached *physical* keyboard's layout. `classify` is a pure
 * function of a `probe` (which wraps InputDevice.getKeyCodeForKeyLocation), so the
 * decision rule is unit-tested without a device; `detect` is the thin, untested
 * glue that finds an attached full keyboard and calls `classify`.
 *
 * getKeyCodeForKeyLocation(k) returns the keycode the key at k's US-QWERTY
 * location actually produces on the attached device: on AZERTY the US-Q slot
 * yields A; on QWERTZ the US-Y slot yields Z.
 */
object PhysicalKeyboardLayout {

    /** Q→A ⇒ AZERTY, Y→Z ⇒ QWERTZ, identity ⇒ QWERTY, anything else ⇒ null. */
    fun classify(probe: (Int) -> Int): KeyboardLayoutChoice? {
        val q = probe(KeyEvent.KEYCODE_Q)
        if (q == KeyEvent.KEYCODE_A) return KeyboardLayoutChoice.AZERTY
        val y = probe(KeyEvent.KEYCODE_Y)
        if (y == KeyEvent.KEYCODE_Z) return KeyboardLayoutChoice.QWERTZ
        if (q == KeyEvent.KEYCODE_Q && y == KeyEvent.KEYCODE_Y) return KeyboardLayoutChoice.QWERTY
        return null
    }

    /** Probe the first attached, non-virtual alphabetic keyboard. Null if none. */
    fun detect(): KeyboardLayoutChoice? {
        for (id in InputDevice.getDeviceIds()) {
            val dev = InputDevice.getDevice(id) ?: continue
            if (dev.isVirtual) continue
            if (dev.keyboardType != InputDevice.KEYBOARD_TYPE_ALPHABETIC) continue
            classify { keyCode -> dev.getKeyCodeForKeyLocation(keyCode) }?.let { return it }
        }
        return null
    }
}
