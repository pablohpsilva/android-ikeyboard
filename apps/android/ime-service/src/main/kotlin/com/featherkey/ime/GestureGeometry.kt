package com.featherkey.ime

/*
 * Pure key-center arithmetic for swipe decoding, kept PointF-free so it runs under
 * plain JUnit (mirrors the TypingRules pattern). The service holds the layout's
 * key centers and the per-key tap offsets learned by the core; this applies the
 * offsets before handing the shifted centers to the gesture decoder.
 */

/** Geometry helpers for the swipe path, operating on plain (x, y) pairs. */
object GestureGeometry {
    /**
     * [centers] with each key's per-key offset from [offsets] added to its (x, y).
     * Keys absent from [offsets] keep their original center; offsets for keys not
     * in [centers] are ignored. Returns a new map; inputs are not mutated. Order
     * follows [centers].
     */
    fun shiftCenters(
        centers: Map<Char, Pair<Float, Float>>,
        offsets: Map<Char, Pair<Float, Float>>,
    ): Map<Char, Pair<Float, Float>> {
        val out = LinkedHashMap<Char, Pair<Float, Float>>(centers.size)
        for ((key, center) in centers) {
            val offset = offsets[key]
            out[key] = if (offset == null) {
                center
            } else {
                (center.first + offset.first) to (center.second + offset.second)
            }
        }
        return out
    }
}
