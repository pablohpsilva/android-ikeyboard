package com.featherkey.keyboard

/**
 * Pure keyboard geometry — no Android types, so it is unit-testable off-device.
 * The strip band is reserved on strip-bearing pages regardless of whether any
 * suggestions are currently shown, so the reported IME height never changes on
 * suggestion open/close (the host app stops shifting).
 */
object KeyboardGeometry {
    /** Total keyboard height in px: content rows + function row + system inset,
     *  plus a reserved suggestion band ([stripPx]) when [stripReserved].
     *  Deliberately has no `suggestions` parameter — the height cannot depend on
     *  strip contents. */
    fun totalHeightPx(
        stripReserved: Boolean,
        rowPx: Float,
        funcPx: Float,
        insetPx: Float,
        stripPx: Float,
        contentRows: Int = 3,
    ): Float = (if (stripReserved) stripPx else 0f) + rowPx * contentRows + funcPx + insetPx

    /** The y-offset where the key grid starts: below a reserved strip band, else 0. */
    fun contentTopPx(stripReserved: Boolean, stripPx: Float): Float =
        if (stripReserved) stripPx else 0f

    /** Sub-rects of the strip band `[0,width] x [0,band]`: a square settings icon
     *  (left), a square voice icon (right), and three equal suggestion cells
     *  filling the middle. [iconW] is clamped to `[0, width/3]` so the middle
     *  never collapses on very narrow screens. */
    fun stripSubRects(width: Float, band: Float, iconW: Float): StripRects {
        val ic = iconW.coerceIn(0f, width / 3f)
        val settings = Rect4(0f, 0f, ic, band)
        val voice = Rect4(width - ic, 0f, width, band)
        val cw = (width - 2f * ic) / 3f
        val suggestions = (0..2).map { i -> Rect4(ic + i * cw, 0f, ic + (i + 1) * cw, band) }
        return StripRects(settings, suggestions, voice)
    }
}

/** A rectangle as plain floats — Android-type-free, so it unit-tests off-device. */
data class Rect4(val left: Float, val top: Float, val right: Float, val bottom: Float)

/** Layout of the suggestion strip band: a square settings icon pinned left, a
 *  square voice icon pinned right, and three equal suggestion cells between. */
data class StripRects(val settings: Rect4, val suggestions: List<Rect4>, val voice: Rect4)

/**
 * Identity of a computed key-cell layout. `buildCells` output depends on exactly
 * these inputs; a repeated draw with an equal key can reuse the cached cells.
 * Excludes `shifted` (applied at draw time) and `suggestions` (strip band is
 * always reserved), which do not change cell geometry.
 */
data class CellLayoutKey(
    val width: Int,
    val height: Int,
    val pageOrdinal: Int,
    val keysVersion: Int,
    val affixKeys: List<String> = emptyList(),
)
