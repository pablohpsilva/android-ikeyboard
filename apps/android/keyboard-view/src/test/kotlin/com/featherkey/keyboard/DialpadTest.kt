package com.featherkey.keyboard

import org.junit.Assert.assertEquals
import org.junit.Test

class DialpadTest {
    @Test fun rows_are_the_e161_telephone_keypad() {
        val rows = Dialpad.ROWS
        assertEquals(4, rows.size)
        assertEquals(listOf(3, 3, 3, 3), rows.map { it.size })
        // labels, row-major
        assertEquals(
            listOf("1","2","3","4","5","6","7","8","9",".",",","0"),
            rows.flatten().map { it.label },
        )
        // E.161 letter subtitles ("" where the key has none)
        assertEquals(
            listOf("","ABC","DEF","GHI","JKL","MNO","PQRS","TUV","WXYZ","","",""),
            rows.flatten().map { it.sub },
        )
    }
}
