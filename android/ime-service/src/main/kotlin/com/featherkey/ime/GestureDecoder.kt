package com.featherkey.ime

/*
 * Swipe/glide typing decoder. Given the finger path and the on-screen key
 * centres, it ranks the active-language words whose ideal path (the polyline
 * through their letters' keys) best matches the gesture.
 *
 * It combines two channels (a location model à la SHARK²):
 *  - location: absolute point-to-point distance after arc-length resampling, so
 *    a swipe that hovers the right keys scores well;
 *  - shape: the same distance after centring+scaling both paths, so an offset or
 *    smaller/larger swipe of the right shape still matches.
 * Words are pruned by first/last-letter proximity before the (cheap) scoring, so
 * the whole active vocabulary can be considered per gesture.
 *
 * Pure and coordinate-space-agnostic: pass path and key centres in the SAME
 * space (screen pixels here) and it needs no layout knowledge of its own.
 */

import android.graphics.PointF
import kotlin.math.hypot
import kotlin.math.sqrt

object GestureDecoder {

    private const val SAMPLES = 24
    private const val SHAPE_WEIGHT = 0.3f

    /** Best-matching words for [path], most likely first (empty if not a gesture). */
    fun decode(
        path: List<PointF>,
        centers: Map<Char, PointF>,
        words: List<String>,
        limit: Int = 4,
    ): List<String> {
        if (path.size < 3 || centers.isEmpty() || words.isEmpty()) return emptyList()
        val pts = resample(path, SAMPLES) ?: return emptyList()
        val step = avgKeyStep(centers)
        val pruneR = step * 1.7f
        val start = pts.first()
        val end = pts.last()
        val normPts = normalize(pts)

        val scored = ArrayList<Pair<String, Float>>()
        val poly = ArrayList<PointF>()
        words@ for (w in words) {
            if (w.length < 2) continue
            val firstC = centers[w.first()] ?: continue
            val lastC = centers[w.last()] ?: continue
            if (dist(start, firstC) > pruneR || dist(end, lastC) > pruneR) continue
            poly.clear()
            for (ch in w) poly.add(centers[ch] ?: continue@words)
            val ideal = resample(poly, SAMPLES) ?: continue
            var loc = 0f
            var shape = 0f
            val normIdeal = normalize(ideal)
            for (i in 0 until SAMPLES) {
                loc += dist(pts[i], ideal[i])
                shape += dist(normPts[i], normIdeal[i])
            }
            loc /= SAMPLES
            shape /= SAMPLES
            scored.add(w to loc + SHAPE_WEIGHT * step * shape)
        }
        scored.sortBy { it.second }
        val out = ArrayList<String>(limit)
        for ((w, _) in scored) {
            if (w !in out) out.add(w)
            if (out.size >= limit) break
        }
        return out
    }

    private fun resample(pts: List<PointF>, n: Int): List<PointF>? {
        if (pts.size < 2) return null
        val cum = FloatArray(pts.size)
        var total = 0f
        for (i in 1 until pts.size) {
            total += dist(pts[i - 1], pts[i])
            cum[i] = total
        }
        if (total <= 1e-3f) return null
        val out = ArrayList<PointF>(n)
        out.add(PointF(pts.first().x, pts.first().y))
        val stepLen = total / (n - 1)
        var seg = 1
        for (k in 1 until n - 1) {
            val target = stepLen * k
            while (seg < pts.size - 1 && cum[seg] < target) seg++
            val segStart = cum[seg - 1]
            val segEnd = cum[seg]
            val t = if (segEnd > segStart) (target - segStart) / (segEnd - segStart) else 0f
            val a = pts[seg - 1]
            val b = pts[seg]
            out.add(PointF(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t))
        }
        out.add(PointF(pts.last().x, pts.last().y))
        return out
    }

    private fun normalize(pts: List<PointF>): List<PointF> {
        var cx = 0f
        var cy = 0f
        for (p in pts) { cx += p.x; cy += p.y }
        cx /= pts.size; cy /= pts.size
        var rms = 0f
        for (p in pts) { val dx = p.x - cx; val dy = p.y - cy; rms += dx * dx + dy * dy }
        rms = sqrt(rms / pts.size)
        if (rms < 1e-3f) rms = 1f
        return pts.map { PointF((it.x - cx) / rms, (it.y - cy) / rms) }
    }

    /** Average nearest-neighbour distance between key centres (~one key pitch). */
    private fun avgKeyStep(centers: Map<Char, PointF>): Float {
        val list = centers.values.toList()
        if (list.size < 2) return 100f
        var sum = 0f
        var cnt = 0
        for (i in list.indices) {
            var nearest = Float.MAX_VALUE
            for (j in list.indices) if (i != j) nearest = minOf(nearest, dist(list[i], list[j]))
            if (nearest < Float.MAX_VALUE) { sum += nearest; cnt++ }
        }
        return if (cnt > 0) sum / cnt else 100f
    }

    private fun dist(a: PointF, b: PointF): Float = hypot(a.x - b.x, a.y - b.y)
}
