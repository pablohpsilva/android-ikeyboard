package com.featherkey.platform

import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PhysicalKeyboardLayoutTest {
    @Test fun q_maps_to_a_means_azerty() {
        val choice = PhysicalKeyboardLayout.classify { kc ->
            if (kc == KeyEvent.KEYCODE_Q) KeyEvent.KEYCODE_A else kc
        }
        assertEquals(KeyboardLayoutChoice.AZERTY, choice)
    }

    @Test fun y_maps_to_z_means_qwertz() {
        val choice = PhysicalKeyboardLayout.classify { kc ->
            when (kc) {
                KeyEvent.KEYCODE_Y -> KeyEvent.KEYCODE_Z
                else -> kc
            }
        }
        assertEquals(KeyboardLayoutChoice.QWERTZ, choice)
    }

    @Test fun identity_means_qwerty() {
        assertEquals(KeyboardLayoutChoice.QWERTY, PhysicalKeyboardLayout.classify { it })
    }

    @Test fun unrecognised_mapping_is_null() {
        // e.g. Dvorak: Q location produces neither A nor identity.
        assertNull(PhysicalKeyboardLayout.classify { kc ->
            if (kc == KeyEvent.KEYCODE_Q) KeyEvent.KEYCODE_SEMICOLON else KeyEvent.KEYCODE_UNKNOWN
        })
    }
}
