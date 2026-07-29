package com.featherkey.keyboard

/**
 * Pure keyboard geometry — no Android types, so it is unit-testable off-device.
 * The strip band is reserved on strip-bearing pages regardless of whether any
 * suggestions are currently shown, so the reported IME height never changes on
 * suggestion open/close (the host app stops shifting).
 */
object KeyboardGeometry {
    /** Total keyboard height in px: three letter rows + function row + bottom bar
     *  + system inset, plus a reserved suggestion band ([stripPx]) when
     *  [stripReserved]. Deliberately has no `suggestions` parameter — the height
     *  cannot depend on strip contents. */
    fun totalHeightPx(
        stripReserved: Boolean,
        rowPx: Float,
        funcPx: Float,
        barPx: Float,
        insetPx: Float,
        stripPx: Float,
    ): Float = (if (stripReserved) stripPx else 0f) + rowPx * 3 + funcPx + barPx + insetPx

    /** The y-offset where the key grid starts: below a reserved strip band, else 0. */
    fun contentTopPx(stripReserved: Boolean, stripPx: Float): Float =
        if (stripReserved) stripPx else 0f
}

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
