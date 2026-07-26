package com.featherkey.keyboard

/*
 * Long-press accent variants for Latin letters (AOSP "more-keys" style).
 *
 * [INVENTORY] is the complete, de-duplicated set of variants each base letter can
 * produce — the union across the languages we serve (Luxembourgish, fr/de/es/pt).
 * [PRIORITY] then front-loads the variants a given language uses most, so a
 * straight-down slide (which commits the first cell) lands on the likely choice
 * for the primary language. Ordering is chosen by the first *active* language that
 * actually carries accents, so an English-primary user with Portuguese enabled
 * still gets the Portuguese order. The object is pure so the map, the language
 * ordering, and the hit-test are all unit-tested.
 */
object Accents {

    /** Every variant a base letter can produce, de-duplicated, in a neutral
     *  acute-first default order (used when no active language has a preference). */
    private val INVENTORY: Map<Char, List<String>> = mapOf(
        'a' to listOf("á", "à", "â", "ã", "ä", "å"),
        'e' to listOf("é", "è", "ê", "ë"),
        'i' to listOf("í", "ì", "î", "ï"),
        'o' to listOf("ó", "ò", "ô", "õ", "ö", "ø"),
        'u' to listOf("ú", "ù", "û", "ü"),
        'c' to listOf("ç"),
        'n' to listOf("ñ"),
        'y' to listOf("ý", "ÿ"),
        's' to listOf("ß"),
    )

    /** Per-language front-ordering. Keyed by primary-language tag (region stripped,
     *  lowercased). Only the variants a language leads with need listing; anything
     *  omitted keeps [INVENTORY]'s order behind them. Every entry must be a member
     *  of the letter's [INVENTORY] set. */
    private val PRIORITY: Map<String, Map<Char, List<String>>> = mapOf(
        // Portuguese: acute + tilde lead.
        "pt" to mapOf(
            'a' to listOf("á", "ã", "â", "à"),
            'e' to listOf("é", "ê", "è"),
            'i' to listOf("í"),
            'o' to listOf("ó", "õ", "ô", "ò"),
            'u' to listOf("ú"),
            'c' to listOf("ç"),
        ),
        // Luxembourgish: umlaut + grave lead (the keyboard's original default).
        "lb" to mapOf(
            'a' to listOf("ä", "à", "â"),
            'e' to listOf("ë", "é", "è", "ê"),
            'i' to listOf("ï", "î"),
            'o' to listOf("ö", "ô"),
            'u' to listOf("ü", "ù", "û"),
            'c' to listOf("ç"),
            'n' to listOf("ñ"),
        ),
        // French: grave/circumflex family, cedilla.
        "fr" to mapOf(
            'a' to listOf("à", "â"),
            'e' to listOf("é", "è", "ê", "ë"),
            'i' to listOf("î", "ï"),
            'o' to listOf("ô"),
            'u' to listOf("ù", "û", "ü"),
            'c' to listOf("ç"),
            'y' to listOf("ÿ"),
        ),
        // German: umlauts + eszett.
        "de" to mapOf(
            'a' to listOf("ä"),
            'o' to listOf("ö"),
            'u' to listOf("ü"),
            's' to listOf("ß"),
        ),
        // Spanish: acute vowels, eñe, diaeresis-u.
        "es" to mapOf(
            'a' to listOf("á"),
            'e' to listOf("é"),
            'i' to listOf("í"),
            'o' to listOf("ó"),
            'u' to listOf("ú", "ü"),
            'n' to listOf("ñ"),
        ),
    )

    /** Accent variants for [base], ordered for the first of [langs] (preference
     *  order) that has a front-ordering for this letter; otherwise the default
     *  [INVENTORY] order. Never contains duplicates and always spans the full
     *  inventory for the letter. Empty for a letter with no accents. */
    fun variantsFor(base: Char, langs: List<String> = emptyList()): List<String> {
        val c = base.lowercaseChar()
        val inventory = INVENTORY[c] ?: return emptyList()
        val front = langs.asSequence()
            .mapNotNull { PRIORITY[primaryTag(it)]?.get(c) }
            .firstOrNull()
            ?: return inventory
        // Language-preferred variants first (guarded to the inventory), then the
        // rest of the inventory; the set de-dupes while preserving first order.
        val ordered = LinkedHashSet<String>(inventory.size)
        front.forEach { if (it in inventory) ordered.add(it) }
        ordered.addAll(inventory)
        return ordered.toList()
    }

    fun hasVariants(base: Char): Boolean = INVENTORY.containsKey(base.lowercaseChar())

    /** Normalize a language tag to its primary subtag: "pt-BR" → "pt". */
    private fun primaryTag(tag: String): String =
        tag.substringBefore('-').substringBefore('_').lowercase()

    /** The cell index [x] falls into for a popup of [count] cells of width [cellW]
     *  starting at [left]; null if [x] is outside `[left, left + cellW*count)`. */
    fun variantIndexAt(x: Float, left: Float, cellW: Float, count: Int): Int? {
        if (count <= 0 || cellW <= 0f || x < left) return null
        val i = ((x - left) / cellW).toInt()
        return if (i in 0 until count) i else null
    }
}
