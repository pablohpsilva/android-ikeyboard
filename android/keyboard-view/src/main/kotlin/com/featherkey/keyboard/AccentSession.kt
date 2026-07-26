package com.featherkey.keyboard

/**
 * The state of an in-progress long-press accent selection, extracted from
 * KeyboardView so the decision logic (which variant a release commits) is pure
 * and unit-testable. KeyboardView owns the MotionEvent/timer/Canvas plumbing and
 * the popup pixel geometry; this holds only the selection state.
 */
class AccentSession {
    var base: Char? = null
        private set
    var variants: List<String> = emptyList()
        private set
    /** -1 = nothing highlighted, so a release commits the base letter. */
    var index: Int = -1
        private set

    val active: Boolean get() = base != null

    /** Open the popup for [base] if it has accent variants, ordered for the active
     *  [langs] (preference order, primary first); true if opened. */
    fun open(base: Char, langs: List<String> = emptyList()): Boolean {
        val v = Accents.variantsFor(base, langs)
        if (v.isEmpty()) return false
        this.base = base
        this.variants = v
        this.index = -1
        return true
    }

    /** Highlight the variant under finger x over the popup band [left, left+cellW*n). */
    fun moveTo(x: Float, left: Float, cellW: Float) {
        Accents.variantIndexAt(x, left, cellW, variants.size)?.let { index = it }
    }

    /** Char to commit on release: highlighted variant, else the base letter; null if inactive. */
    fun release(): String? {
        if (!active) return null
        return variants.getOrNull(index) ?: base?.toString()
    }

    fun reset() {
        base = null
        variants = emptyList()
        index = -1
    }
}
