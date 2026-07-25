package com.featherkey.keyboard

/*
 * Long-press accent variants for Latin letters (AOSP "more-keys" style). The map
 * is language-agnostic — one universal set that serves Luxembourgish (ä ë é è) as
 * well as fr/de/es/pt — with the variants most common in Luxembourgish listed
 * first so a straight-down slide lands on the likely choice. KeyboardView renders
 * and drives these; this object is pure so the map and hit-test are unit-tested.
 */
object Accents {

    private val MAP: Map<Char, List<String>> = mapOf(
        'e' to listOf("ë", "é", "è", "ê"),
        'a' to listOf("ä", "à", "â"),
        'u' to listOf("ü", "ù", "û"),
        'o' to listOf("ö", "ô"),
        'i' to listOf("ï", "î"),
        'c' to listOf("ç"),
        'n' to listOf("ñ"),
        'y' to listOf("ÿ"),
        's' to listOf("ß"),
    )

    fun variantsFor(base: Char): List<String> = MAP[base.lowercaseChar()] ?: emptyList()

    fun hasVariants(base: Char): Boolean = MAP.containsKey(base.lowercaseChar())

    /** The cell index [x] falls into for a popup of [count] cells of width [cellW]
     *  starting at [left]; null if [x] is outside `[left, left + cellW*count)`. */
    fun variantIndexAt(x: Float, left: Float, cellW: Float, count: Int): Int? {
        if (count <= 0 || cellW <= 0f || x < left) return null
        val i = ((x - left) / cellW).toInt()
        return if (i in 0 until count) i else null
    }
}
