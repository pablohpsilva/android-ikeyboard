package com.featherkey.ime

/*
 * Match folding. The on-screen keyboard only carries base lowercase keys (a–z),
 * so swipe decoding and prefix completion type in unaccented, apostrophe-less,
 * lowercase letters, while the dictionaries hold the correctly written words
 * ("também", "café", "I'm", "don't"). Folding a word to a bare match key —
 * lowercased, diacritics stripped (é→e, ç→c), apostrophes removed (I'm→im,
 * don't→dont) — lets that base input match the real entry; the real form is
 * always what gets committed. Fold is only ever a match key, never displayed, so
 * lowercasing it is safe. Pure and JVM-only (NFD via java.text.Normalizer), so
 * it is directly unit-testable.
 */

import java.text.Normalizer

object Diacritics {

    /** [s] as a bare match key: lowercased, combining diacritics removed (é→e,
     *  ç→c), apostrophes dropped (I'm→im, don't→dont). */
    fun fold(s: String): String {
        // Plain lowercase ASCII (most dictionary words) is already its own key.
        if (isPlainLowerAscii(s)) return s
        val src = if (isAscii(s)) s else Normalizer.normalize(s, Normalizer.Form.NFD)
        val sb = StringBuilder(src.length)
        for (c in src) {
            if (isCombiningMark(c) || isApostrophe(c)) continue
            sb.append(c.lowercaseChar())
        }
        return sb.toString()
    }

    /** A single character folded to its base lowercase letter (É→e, ç→c). */
    fun foldChar(c: Char): Char {
        if (c.code < 0x80) return c.lowercaseChar()
        val decomposed = Normalizer.normalize(c.toString(), Normalizer.Form.NFD)
        return (decomposed.firstOrNull { !isCombiningMark(it) } ?: c).lowercaseChar()
    }

    private fun isPlainLowerAscii(s: String): Boolean {
        for (c in s) if (c.code >= 0x80 || c in 'A'..'Z' || isApostrophe(c)) return false
        return true
    }

    private fun isAscii(s: String): Boolean {
        for (c in s) if (c.code >= 0x80) return false
        return true
    }

    private fun isApostrophe(c: Char): Boolean = c == '\'' || c == '’'

    /** A non-spacing combining mark — the accent left dangling by NFD. */
    private fun isCombiningMark(c: Char): Boolean =
        Character.getType(c) == Character.NON_SPACING_MARK.toInt()
}
