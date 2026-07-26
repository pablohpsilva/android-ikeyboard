package com.featherkey.ime

import org.junit.Assert.assertEquals
import org.junit.Test

/** Pure key-center arithmetic for swipe decoding, PointF-free so it runs under
 *  plain JUnit (like [TypingRulesTest]). */
class GestureGeometryTest {

    @Test fun offsets_are_added_per_key() {
        val centers = mapOf('a' to (10f to 20f), 'b' to (30f to 40f))
        val offsets = mapOf('a' to (1f to -2f), 'b' to (-3f to 4f))
        assertEquals(
            mapOf('a' to (11f to 18f), 'b' to (27f to 44f)),
            GestureGeometry.shiftCenters(centers, offsets),
        )
    }

    @Test fun keys_without_an_offset_are_unchanged() {
        val centers = mapOf('a' to (10f to 20f), 'b' to (30f to 40f))
        val offsets = mapOf('a' to (5f to 5f))
        assertEquals(
            mapOf('a' to (15f to 25f), 'b' to (30f to 40f)),
            GestureGeometry.shiftCenters(centers, offsets),
        )
    }

    @Test fun offsets_for_unknown_keys_are_ignored() {
        val centers = mapOf('a' to (10f to 20f))
        val offsets = mapOf('a' to (1f to 1f), 'z' to (99f to 99f))
        assertEquals(
            mapOf('a' to (11f to 21f)),
            GestureGeometry.shiftCenters(centers, offsets),
        )
    }

    @Test fun empty_offsets_leave_centers_untouched() {
        val centers = mapOf('a' to (10f to 20f), 'b' to (30f to 40f))
        assertEquals(centers, GestureGeometry.shiftCenters(centers, emptyMap()))
    }

    @Test fun empty_centers_yield_empty() {
        assertEquals(
            emptyMap<Char, Pair<Float, Float>>(),
            GestureGeometry.shiftCenters(emptyMap(), mapOf('a' to (1f to 1f))),
        )
    }
}
