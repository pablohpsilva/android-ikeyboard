package com.featherkey.platform

import org.junit.Assert.assertEquals
import org.junit.Test

class KeyboardLayoutChoiceTest {
    @Test fun default_and_unknown_tags_fall_back_to_auto() {
        assertEquals(KeyboardLayoutChoice.AUTO, KeyboardLayoutChoice.fromTag(null))
        assertEquals(KeyboardLayoutChoice.AUTO, KeyboardLayoutChoice.fromTag("dvorak"))
        assertEquals(KeyboardLayoutChoice.AUTO, KeyboardLayoutChoice.fromTag(""))
    }

    @Test fun each_known_tag_round_trips_through_fromTag() {
        // Proves the tag written by setChoice(x) is exactly what fromTag reads back as x.
        for (choice in KeyboardLayoutChoice.entries) {
            assertEquals(choice, KeyboardLayoutChoice.fromTag(choice.tag))
        }
    }
}
