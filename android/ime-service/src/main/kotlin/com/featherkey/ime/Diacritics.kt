package com.featherkey.ime

/*
 * Accent folding. The on-screen keyboard only carries base Latin keys (a–z), so
 * both swipe decoding and prefix completion type in *unaccented* letters, while
 * the dictionaries hold the correctly accented words ("também", "café"). Folding
 * a word to its base letters lets base-letter input match an accented entry; the
 * accented form is always what gets committed. Pure and JVM-only (NFD via
 * java.text.Normalizer), so it is directly unit-testable.
 */

import java.text.Normalizer

object Diacritics {

    /** [s] with combining diacritics removed (é→e, ã→a, ç→c), case preserved. */
    fun fold(s: String): String {
        // Pure-ASCII is the overwhelming common case (all of English) — skip NFD.
        if (isAscii(s)) return s
        val decomposed = Normalizer.normalize(s, Normalizer.Form.NFD)
        val sb = StringBuilder(decomposed.length)
        for (c in decomposed) if (!isCombiningMark(c)) sb.append(c)
        return sb.toString()
    }

    /** A single character folded to its base letter (é→e, ç→c); ASCII unchanged. */
    fun foldChar(c: Char): Char {
        if (c.code < 0x80) return c
        val decomposed = Normalizer.normalize(c.toString(), Normalizer.Form.NFD)
        return decomposed.firstOrNull { !isCombiningMark(it) } ?: c
    }

    private fun isAscii(s: String): Boolean {
        for (c in s) if (c.code >= 0x80) return false
        return true
    }

    /** A non-spacing combining mark — the accent left dangling by NFD. */
    private fun isCombiningMark(c: Char): Boolean =
        Character.getType(c) == Character.NON_SPACING_MARK.toInt()
}
