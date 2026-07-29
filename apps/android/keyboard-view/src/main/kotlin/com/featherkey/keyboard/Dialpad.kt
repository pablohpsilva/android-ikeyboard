package com.featherkey.keyboard

/** One dialpad key: the digit/char it types, plus its E.161 telephone letters
 *  ("" when the key has none). The letters are decorative — a tap types [label]. */
data class DialKey(val label: String, val sub: String)

/** Telephone keypad (E.161) for numeric-only fields. Row-major. Row 4 lists the
 *  three character keys ". , 0"; the trailing backspace is a function key added by
 *  the view, not a DialKey. */
object Dialpad {
    val ROWS: List<List<DialKey>> = listOf(
        listOf(DialKey("1", ""),     DialKey("2", "ABC"),  DialKey("3", "DEF")),
        listOf(DialKey("4", "GHI"),  DialKey("5", "JKL"),  DialKey("6", "MNO")),
        listOf(DialKey("7", "PQRS"), DialKey("8", "TUV"),  DialKey("9", "WXYZ")),
        listOf(DialKey(".", ""),     DialKey(",", ""),     DialKey("0", "")),
    )
}
