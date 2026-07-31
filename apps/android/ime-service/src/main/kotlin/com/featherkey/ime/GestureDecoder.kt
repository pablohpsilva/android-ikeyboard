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

    /**
     * The keys a swipe of [word] passes through: every character folded to its
     * base key (é→e, ç→c, case dropped) and any character with no key on the
     * layout — an apostrophe (I've, don't), a hyphen — simply skipped, because
     * the finger never crosses a key that isn't there. [hasKey] answers whether
     * a folded character is a real key.
     *
     * This is the whole reason apostrophe words are swipeable: the ideal path
     * runs through the *typeable* letters only. The earlier decoder dropped any
     * word the moment it hit a non-key character, so every "I've"/"don't"/"he'll"
     * was unreachable by glide (it could only ever fall back to a plainer word).
     * Pure and PointF-free, so it is directly unit-testable.
     */
    fun keyPath(word: String, hasKey: (Char) -> Boolean): List<Char> {
        val out = ArrayList<Char>(word.length)
        for (ch in word) {
            val k = Diacritics.foldChar(ch)
            if (hasKey(k)) out.add(k)
        }
        return out
    }

    // Frequency/learning discounts applied to the shape score, so a common or
    // user-used word wins over an obscure one of a similar shape.
    private const val LEARNED_BOOST = 0.55f   // words the user has typed/swiped
    private const val FREQ_MIN = 0.70f        // most-common dictionary words
    private const val FREQ_SPAN = 8000f       // rank where the frequency boost fades out

    /**
     * The swipe candidate set, precomputed once per vocabulary load and reused by
     * every [decode]. Words are bucketed by their first typeable key and carry
     * their last typeable key, so a gesture is pruned to just the words that begin
     * near where the finger started (and end near where it lifted) *without*
     * re-deriving every word's key path on every gesture — the per-word work that
     * made swipe over a large multi-language vocabulary take hundreds of ms.
     *
     * The candidate set is identical to the old whole-vocabulary scan: a word was
     * (and is) scored exactly when its first key is within the prune radius of the
     * gesture start and its last key within the radius of the end. Build off the
     * UI thread (the vocabulary already loads asynchronously).
     */
    class Index private constructor(
        private val byFirstKey: Map<Char, List<Entry>>,
    ) {
        /** One swipeable word: its first key is the bucket it lives in; [last] is
         *  its final typeable key (for the end-of-gesture prune). */
        internal class Entry(val word: String, val last: Char)

        internal fun bucket(firstKey: Char): List<Entry> = byFirstKey[firstKey] ?: emptyList()

        /** Test seam: the words bucketed under [firstKey]. */
        fun wordsForFirstKey(firstKey: Char): List<String> = bucket(firstKey).map { it.word }

        /** Test seam: the recorded last key for [word], or null if it was skipped. */
        fun lastKeyOf(word: String): Char? =
            byFirstKey.values.asSequence().flatten().find { it.word == word }?.last

        companion object {
            /** An empty index, used until the real vocabulary finishes loading. */
            val EMPTY = Index(emptyMap())

            /**
             * Bucket [words] by first typeable key. A word's keys are its letters
             * folded to their base key ('é'→'e') with non-key characters (an
             * apostrophe) dropped, exactly as [keyPath] derives them at decode
             * time against a standard a–z letter layout. Words with fewer than two
             * keys can't be a gesture and are skipped.
             */
            fun build(words: List<String>): Index {
                val buckets = HashMap<Char, MutableList<Entry>>()
                for (w in words) {
                    if (w.length < 2) continue
                    val keys = keyPath(w) { it in 'a'..'z' }
                    if (keys.size < 2) continue
                    buckets.getOrPut(keys.first()) { ArrayList() }.add(Entry(w, keys.last()))
                }
                return Index(buckets)
            }
        }
    }

    /** The longest key path scored; words with more typeable keys than this are
     *  skipped (a swipe never traces 48 letters). Bounds the reusable buffers so
     *  the per-word scoring below allocates nothing on the heap. */
    private const val MAX_KEYS = 48

    /** Best-matching words for [path], most likely first (empty if not a gesture).
     *
     * Hot path: the inner loop scores each surviving candidate into **reused**
     * float buffers — no `PointF`/list allocation per word — because a broad
     * gesture can leave thousands of candidates after pruning, and the old
     * per-word `resample`/`normalize` allocations dominated the cost. The maths
     * (arc-length resample to [SAMPLES] points, centre+scale normalise, location
     * + shape distance, frequency/learning discount) is unchanged. */
    fun decode(
        path: List<PointF>,
        centers: Map<Char, PointF>,
        index: Index,
        rankOf: (String) -> Int,
        learned: Map<String, Int>,
        limit: Int = 4,
    ): List<String> {
        if (path.size < 3 || centers.isEmpty()) return emptyList()
        val step = avgKeyStep(centers)
        val pruneR = step * 1.7f

        // The gesture path, resampled once into reused arrays, then normalised.
        val pathX = FloatArray(path.size) { path[it].x }
        val pathY = FloatArray(path.size) { path[it].y }
        val gx = FloatArray(SAMPLES)
        val gy = FloatArray(SAMPLES)
        if (!resampleInto(pathX, pathY, path.size, FloatArray(path.size), gx, gy)) return emptyList()
        val ngx = FloatArray(SAMPLES)
        val ngy = FloatArray(SAMPLES)
        normalizeInto(gx, gy, SAMPLES, ngx, ngy)
        val startX = gx[0]; val startY = gy[0]
        val endX = gx[SAMPLES - 1]; val endY = gy[SAMPLES - 1]

        // Per-candidate scratch, reused across every word.
        val polyX = FloatArray(MAX_KEYS)
        val polyY = FloatArray(MAX_KEYS)
        val cum = FloatArray(MAX_KEYS)
        val ix = FloatArray(SAMPLES); val iy = FloatArray(SAMPLES)
        val nix = FloatArray(SAMPLES); val niy = FloatArray(SAMPLES)

        val scored = ArrayList<Pair<String, Float>>()
        // Only words whose first key lies within the prune radius of the gesture
        // start can match, so scan just those buckets rather than every word. The
        // end-key prune and the per-key centre lookups below reproduce the old
        // whole-vocabulary scan's accept condition exactly.
        for ((firstKey, firstC) in centers) {
            if (hypot(startX - firstC.x, startY - firstC.y) > pruneR) continue
            for (e in index.bucket(firstKey)) {
                val lastC = centers[e.last] ?: continue
                if (hypot(endX - lastC.x, endY - lastC.y) > pruneR) continue
                // Fold the word's characters straight into the poly buffer: accents
                // fold to their base key ('é'→'e'), non-key characters (apostrophe)
                // are dropped — exactly [keyPath], but without allocating a list.
                var n = 0
                for (ch in e.word) {
                    val c = centers[Diacritics.foldChar(ch)] ?: continue
                    if (n >= MAX_KEYS) { n = -1; break }
                    polyX[n] = c.x; polyY[n] = c.y; n++
                }
                if (n < 2) continue
                if (!resampleInto(polyX, polyY, n, cum, ix, iy)) continue
                normalizeInto(ix, iy, SAMPLES, nix, niy)
                var loc = 0f
                var shape = 0f
                for (i in 0 until SAMPLES) {
                    loc += hypot(gx[i] - ix[i], gy[i] - iy[i])
                    shape += hypot(ngx[i] - nix[i], ngy[i] - niy[i])
                }
                loc /= SAMPLES
                shape /= SAMPLES
                var score = loc + SHAPE_WEIGHT * step * shape
                score *= when {
                    learned.containsKey(e.word) -> LEARNED_BOOST
                    else -> {
                        val r = rankOf(e.word)
                        if (r >= Int.MAX_VALUE) 1f
                        else FREQ_MIN + (1f - FREQ_MIN) * minOf(1f, r / FREQ_SPAN)
                    }
                }
                scored.add(e.word to score)
            }
        }
        scored.sortBy { it.second }
        val out = ArrayList<String>(limit)
        for ((w, _) in scored) {
            if (w !in out) out.add(w)
            if (out.size >= limit) break
        }
        return out
    }

    /** Arc-length resample the polyline in `xs`/`ys[0 until n]` to [SAMPLES] evenly
     *  spaced points, written into `outX`/`outY`. `cum` (length ≥ n) is scratch for
     *  the cumulative lengths. Returns false for a degenerate (zero-length) path. */
    private fun resampleInto(
        xs: FloatArray,
        ys: FloatArray,
        n: Int,
        cum: FloatArray,
        outX: FloatArray,
        outY: FloatArray,
    ): Boolean {
        if (n < 2) return false
        cum[0] = 0f
        var total = 0f
        for (i in 1 until n) {
            total += hypot(xs[i] - xs[i - 1], ys[i] - ys[i - 1])
            cum[i] = total
        }
        if (total <= 1e-3f) return false
        outX[0] = xs[0]; outY[0] = ys[0]
        val stepLen = total / (SAMPLES - 1)
        var seg = 1
        for (k in 1 until SAMPLES - 1) {
            val target = stepLen * k
            while (seg < n - 1 && cum[seg] < target) seg++
            val segStart = cum[seg - 1]
            val segEnd = cum[seg]
            val t = if (segEnd > segStart) (target - segStart) / (segEnd - segStart) else 0f
            outX[k] = xs[seg - 1] + (xs[seg] - xs[seg - 1]) * t
            outY[k] = ys[seg - 1] + (ys[seg] - ys[seg - 1]) * t
        }
        outX[SAMPLES - 1] = xs[n - 1]; outY[SAMPLES - 1] = ys[n - 1]
        return true
    }

    /** Centre and scale-normalise the `n` points in `xs`/`ys` into `outX`/`outY`
     *  (subtract the centroid, divide by RMS radius), so an offset or larger/
     *  smaller path of the same shape matches. */
    private fun normalizeInto(
        xs: FloatArray,
        ys: FloatArray,
        n: Int,
        outX: FloatArray,
        outY: FloatArray,
    ) {
        var cx = 0f
        var cy = 0f
        for (i in 0 until n) { cx += xs[i]; cy += ys[i] }
        cx /= n; cy /= n
        var rms = 0f
        for (i in 0 until n) { val dx = xs[i] - cx; val dy = ys[i] - cy; rms += dx * dx + dy * dy }
        rms = sqrt(rms / n)
        if (rms < 1e-3f) rms = 1f
        for (i in 0 until n) { outX[i] = (xs[i] - cx) / rms; outY[i] = (ys[i] - cy) / rms }
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
