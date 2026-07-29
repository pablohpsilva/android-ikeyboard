package com.featherkey.keyboard

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class KeyboardGeometryTest {
    // px stand-ins (dp-independent): strip=42, row=52, func=54, bar=46, inset=10
    @Test fun strip_bearing_height_includes_the_strip_band() {
        val h = KeyboardGeometry.totalHeightPx(
            stripReserved = true, rowPx = 52f, funcPx = 54f, barPx = 46f, insetPx = 10f, stripPx = 42f,
        )
        assertEquals(42f + 52f * 3 + 54f + 46f + 10f, h, 0.001f)
    }

    @Test fun emoji_height_excludes_the_strip_band() {
        val h = KeyboardGeometry.totalHeightPx(
            stripReserved = false, rowPx = 52f, funcPx = 54f, barPx = 46f, insetPx = 10f, stripPx = 42f,
        )
        assertEquals(52f * 3 + 54f + 46f + 10f, h, 0.001f)
    }

    @Test fun content_top_offset_matches_the_reserved_band() {
        assertEquals(42f, KeyboardGeometry.contentTopPx(stripReserved = true, stripPx = 42f), 0.001f)
        assertEquals(0f, KeyboardGeometry.contentTopPx(stripReserved = false, stripPx = 42f), 0.001f)
    }
    // Note: neither function takes `suggestions` — height cannot depend on strip contents by construction.

    @Test fun dialpad_reserves_a_fourth_content_row() {
        val three = KeyboardGeometry.totalHeightPx(
            stripReserved = true, rowPx = 52f, funcPx = 54f, barPx = 46f, insetPx = 10f, stripPx = 42f,
        )
        val four = KeyboardGeometry.totalHeightPx(
            stripReserved = true, rowPx = 52f, funcPx = 54f, barPx = 46f, insetPx = 10f, stripPx = 42f,
            contentRows = 4,
        )
        assertEquals(three + 52f, four, 0.001f) // exactly one extra row
    }

    @Test fun dialpad_has_no_function_row() {
        // Fully-numeric dialpad: 4 content rows, NO shared function row (funcPx = 0).
        val dialpad = KeyboardGeometry.totalHeightPx(
            stripReserved = true, rowPx = 52f, funcPx = 0f, barPx = 46f, insetPx = 10f, stripPx = 42f,
            contentRows = 4,
        )
        assertEquals(42f + 52f * 4 + 46f + 10f, dialpad, 0.001f) // strip + 4 rows + bar + inset, no func row
        // And it is exactly one function-row shorter than a dialpad that still had one:
        val withFuncRow = KeyboardGeometry.totalHeightPx(
            stripReserved = true, rowPx = 52f, funcPx = 54f, barPx = 46f, insetPx = 10f, stripPx = 42f,
            contentRows = 4,
        )
        assertEquals(withFuncRow - 54f, dialpad, 0.001f)
    }

    @Test fun memo_key_is_equal_for_identical_inputs_and_differs_per_field() {
        val base = CellLayoutKey(width = 1080, height = 900, pageOrdinal = 0, keysVersion = 3)
        assertEquals(base, CellLayoutKey(1080, 900, 0, 3))
        // Any layout-affecting input change produces a different key:
        assertNotEquals(base, CellLayoutKey(1081, 900, 0, 3)) // width
        assertNotEquals(base, CellLayoutKey(1080, 901, 0, 3)) // height
        assertNotEquals(base, CellLayoutKey(1080, 900, 1, 3)) // page
        assertNotEquals(base, CellLayoutKey(1080, 900, 0, 4)) // keys version (language switch)
        assertNotEquals(base, CellLayoutKey(1080, 900, 0, 3, listOf("@", "."))) // affix keys
        assertEquals(
            CellLayoutKey(1080, 900, 0, 3, listOf("@", ".")),
            CellLayoutKey(1080, 900, 0, 3, listOf("@", ".")),
        ) // equal affixes still hit the cache
    }
}
