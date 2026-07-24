package com.featherkey.keyboard

/*
 * The keyboard surface (SEDD §5.1). It renders an iOS-style keyboard — a
 * collapsible prediction strip, letter/number/symbol pages on white keys, a
 * shift / page-switch and backspace flanking the last content row, a
 * `123|ABC / space / return` row, and a globe/mic bar — and captures touch.
 *
 * Letter taps are reported in the core layout's *logical* space for the core to
 * decode (the fuzzy touch model matters for letters). Number and symbol pages
 * are static, view-owned, and commit their character directly. Every other key
 * is either handled in-view (shift, page switch) or reported as an intent.
 */

import android.content.Context
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PointF
import android.graphics.RectF
import android.graphics.Typeface
import android.util.AttributeSet
import android.util.TypedValue
import android.view.MotionEvent
import android.view.View
import androidx.core.graphics.PathParser
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

/** One renderable key in the layout's logical coordinate space. */
data class RenderKey(val label: String, val x: Float, val y: Float, val width: Float, val height: Float)

/** A non-character key the IME handles directly. */
enum class FunctionKey { SPACE, BACKSPACE, ENTER, GLOBE, MIC }

class KeyboardView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {

    /** Letter touch, mapped into the layout's logical space (decoded by the core). */
    var onKeyTouch: ((x: Float, y: Float) -> Unit)? = null

    /** A number/symbol key: commit this exact character. */
    var onCharKey: ((String) -> Unit)? = null

    /** A function key was pressed. */
    var onFunctionKey: ((FunctionKey) -> Unit)? = null

    /** A suggestion in the strip was tapped (index into [suggestions]). */
    var onSuggestion: ((Int) -> Unit)? = null

    /** A swipe gesture over the letters: the screen-space path and key centres. */
    var onGesture: ((path: List<PointF>, centers: Map<Char, PointF>) -> Unit)? = null

    /** The active alpha layout's keys (from the core). */
    var keys: List<RenderKey> = emptyList()
        set(value) { field = value; requestLayout(); invalidate() }

    /** Predictive suggestions; the strip collapses entirely when this is empty. */
    var suggestions: List<String> = emptyList()
        set(value) {
            val wasEmpty = field.isEmpty()
            field = value
            if (wasEmpty != value.isEmpty()) requestLayout()
            invalidate()
        }

    /** Shift state (next letter uppercase; highlights the shift key). */
    var shifted: Boolean = false
        set(value) { field = value; invalidate() }

    /** Active-language hint shown on the space bar, e.g. "EN" or "EN PT". */
    var spaceHint: String = "EN"
        set(value) { field = value; invalidate() }

    private enum class Page { ALPHA, NUMBERS, SYMBOLS }
    private var page = Page.ALPHA

    /**
     * Height of the system navigation bar under us. The globe/mic bar is lifted
     * by this so it clears the OS IME nav buttons (hide-keyboard + IME switcher)
     * that the system draws in that region.
     */
    private var bottomInset = 0

    init {
        ViewCompat.setOnApplyWindowInsetsListener(this) { _, insets ->
            val nav = insets.getInsets(WindowInsetsCompat.Type.navigationBars()).bottom
            if (nav != bottomInset) { bottomInset = nav; requestLayout() }
            insets
        }
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        ViewCompat.requestApplyInsets(this)
    }

    /** Return to the letter page (called by the IME when a new field starts). */
    fun resetPage() { page = Page.ALPHA; shifted = false; requestLayout(); invalidate() }

    private val renderKeys: List<RenderKey> get() = keys.ifEmpty { FALLBACK_QWERTY }

    private fun dp(v: Float) =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, resources.displayMetrics)

    // --- Geometry ---
    private val stripHeight get() = dp(42f)
    private val rowHeight get() = dp(52f)
    private val funcRowHeight get() = dp(54f)
    private val bottomBarHeight get() = dp(46f)
    private val sideMargin get() = dp(4f)
    private val keyGap get() = dp(5f)     // horizontal gap
    private val rowGap get() = dp(6f)     // vertical gap (inset per row)
    private val keyRadius get() = dp(7f)

    // --- Paints (colours set per-draw from the active theme). ---
    private val keyPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val shadowPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val pressedPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val labelPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        textAlign = Paint.Align.CENTER
        typeface = Typeface.create("sans-serif", Typeface.NORMAL)
    }
    private val hintPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        textAlign = Paint.Align.RIGHT
        typeface = Typeface.create("sans-serif", Typeface.NORMAL)
    }
    private val dividerPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val iconPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND
    }
    private val iconFill = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val trailPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND
    }
    private val path = Path()

    // --- Swipe/glide typing state ---
    private val trail = ArrayList<PointF>()
    private var gestureCell: Cell.Letter? = null
    private var gesturing = false
    private var trailLen = 0f

    private enum class Sp { SHIFT, BACKSPACE, ENTER, SPACE, GLOBE, MIC, TO_NUMBERS, TO_SYMBOLS, TO_ALPHA }

    private sealed class Cell(val rect: RectF) {
        /** [lx],[ly] = key centre and [lw] = key width, in the core's logical
         *  space, so a finger's pixel offset within [rect] can be mapped back to
         *  a logical touch point for the adaptive tap model (single-key fallback). */
        class Letter(
            rect: RectF, val label: String,
            val lx: Float, val ly: Float, val lw: Float,
        ) : Cell(rect)
        class Char(rect: RectF, val label: String) : Cell(rect)
        class Special(rect: RectF, val kind: Sp) : Cell(rect)
        class Suggest(rect: RectF, val index: Int) : Cell(rect)
    }

    private var cells: List<Cell> = emptyList()
    private var pressed: Cell? = null

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val w = MeasureSpec.getSize(widthMeasureSpec)
        val strip = if (suggestions.isEmpty()) 0f else stripHeight
        val h = strip + rowHeight * 3 + funcRowHeight + bottomBarHeight + bottomInset
        setMeasuredDimension(w, h.toInt())
    }

    // ---- Layout -------------------------------------------------------------

    private fun buildCells(w: Int, h: Int): List<Cell> {
        val out = ArrayList<Cell>()
        val contentW = w - sideMargin * 2
        // Letter-key width adapts to the widest alpha row so a 12-column Cyrillic
        // block fits; number/symbol pages keep their fixed 10 columns. A bottom
        // alpha row of `n` letters plus the two 1.5-width side keys spans `n + 3`
        // columns, so the widest row (max 12) sets the count and everything else
        // fits within it.
        val cols = if (page == Page.ALPHA) {
            maxOf(10, renderKeys.groupBy { it.y }.values.maxOfOrNull { it.size } ?: 10)
        } else {
            10
        }
        val baseKeyW = (contentW - keyGap * (cols - 1)) / cols
        val sideW = baseKeyW * 1.5f

        fun rowStart(n: Int) = sideMargin + (contentW - (n * baseKeyW + (n - 1) * keyGap)) / 2f

        var top = 0f
        if (suggestions.isNotEmpty()) {
            val cw = w / 3f
            for (i in 0..2) out += Cell.Suggest(RectF(i * cw, 0f, (i + 1) * cw, stripHeight), i)
            top = stripHeight
        }

        // A char/letter row of equal-width keys, centred.
        fun charRow(labels: List<String>, decodeKeys: List<RenderKey>?) {
            val n = labels.size
            var x = rowStart(n)
            val kt = top + rowGap / 2f; val kb = top + rowHeight - rowGap / 2f
            for (i in 0 until n) {
                val r = RectF(x, kt, x + baseKeyW, kb)
                out += if (decodeKeys != null) {
                    val k = decodeKeys[i]
                    Cell.Letter(r, k.label, k.x + k.width / 2f, k.y + k.height / 2f, k.width)
                } else Cell.Char(r, labels[i])
                x += baseKeyW + keyGap
            }
            top += rowHeight
        }

        // Last content row: [left special] middle [backspace]. `fill` spreads the
        // middle keys edge-to-edge (number/symbol pages); otherwise they are
        // centred at their natural width (letter page).
        fun lastRow(left: Sp, middleLabels: List<String>, decodeKeys: List<RenderKey>?, fill: Boolean) {
            val kt = top + rowGap / 2f; val kb = top + rowHeight - rowGap / 2f
            out += Cell.Special(RectF(sideMargin, kt, sideMargin + sideW, kb), left)
            val bsLeft = w - sideMargin - sideW
            out += Cell.Special(RectF(bsLeft, kt, w - sideMargin, kb), Sp.BACKSPACE)
            val n = middleLabels.size
            if (fill) {
                val avail = (bsLeft - keyGap) - (sideMargin + sideW + keyGap)
                val kw = (avail - keyGap * (n - 1)) / n
                var x = sideMargin + sideW + keyGap
                for (i in 0 until n) {
                    out += Cell.Char(RectF(x, kt, x + kw, kb), middleLabels[i]); x += kw + keyGap
                }
            } else {
                var x = rowStart(n)
                for (i in 0 until n) {
                    val r = RectF(x, kt, x + baseKeyW, kb)
                    out += if (decodeKeys != null) {
                        val k = decodeKeys[i]
                        Cell.Letter(r, k.label, k.x + k.width / 2f, k.y + k.height / 2f, k.width)
                    } else Cell.Char(r, middleLabels[i])
                    x += baseKeyW + keyGap
                }
            }
            top += rowHeight
        }

        when (page) {
            Page.ALPHA -> {
                val rows = renderKeys.groupBy { it.y }.toSortedMap().values.map { it.sortedBy { k -> k.x } }
                val r1 = rows.getOrElse(0) { emptyList() }
                val r2 = rows.getOrElse(1) { emptyList() }
                val r3 = rows.getOrElse(2) { emptyList() }
                charRow(r1.map { it.label }, r1)
                charRow(r2.map { it.label }, r2)
                lastRow(Sp.SHIFT, r3.map { it.label }, r3, fill = false)
            }
            Page.NUMBERS -> {
                charRow(NUMBERS_R1, null)
                charRow(NUMBERS_R2, null)
                lastRow(Sp.TO_SYMBOLS, PUNCT_R3, null, fill = true)
            }
            Page.SYMBOLS -> {
                charRow(SYMBOLS_R1, null)
                charRow(SYMBOLS_R2, null)
                lastRow(Sp.TO_NUMBERS, PUNCT_R3, null, fill = true)
            }
        }

        // Function row: [123|ABC] [ space ] [ return ].
        run {
            val kt = top + rowGap / 2f; val kb = top + funcRowHeight - rowGap / 2f
            val fSideW = baseKeyW * 2f
            val leftKind = if (page == Page.ALPHA) Sp.TO_NUMBERS else Sp.TO_ALPHA
            out += Cell.Special(RectF(sideMargin, kt, sideMargin + fSideW, kb), leftKind)
            val retLeft = w - sideMargin - fSideW
            out += Cell.Special(RectF(retLeft, kt, w - sideMargin, kb), Sp.ENTER)
            out += Cell.Special(RectF(sideMargin + fSideW + keyGap, kt, retLeft - keyGap, kb), Sp.SPACE)
            top += funcRowHeight
        }

        // Bottom bar: globe (left) + mic (right), icon-only.
        run {
            val sz = bottomBarHeight
            out += Cell.Special(RectF(sideMargin, top, sideMargin + sz, top + sz), Sp.GLOBE)
            out += Cell.Special(RectF(w - sideMargin - sz, top, w - sideMargin, top + sz), Sp.MIC)
        }
        return out
    }

    // ---- Draw ---------------------------------------------------------------

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val dark = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) ==
            Configuration.UI_MODE_NIGHT_YES
        val c = if (dark) Palette.DARK else Palette.LIGHT
        canvas.drawColor(c.bg)

        keyPaint.color = c.key
        shadowPaint.color = c.shadow
        pressedPaint.color = c.pressed
        labelPaint.color = c.label
        dividerPaint.color = c.divider
        hintPaint.color = c.hint

        cells = buildCells(width, height)

        // Suggestion strip.
        if (suggestions.isNotEmpty()) {
            labelPaint.color = c.suggestion
            labelPaint.textSize = stripHeight * 0.42f
            for (cell in cells.filterIsInstance<Cell.Suggest>()) {
                val word = suggestions.getOrNull(cell.index) ?: continue
                val cy = cell.rect.centerY() - (labelPaint.ascent() + labelPaint.descent()) / 2
                canvas.drawText(word, cell.rect.centerX(), cy, labelPaint)
            }
            for (i in 1 until 3) if (i < suggestions.size) {
                val x = width / 3f * i
                canvas.drawLine(x, stripHeight * 0.28f, x, stripHeight * 0.72f, dividerPaint)
            }
            labelPaint.color = c.label
        }

        for (cell in cells) when (cell) {
            is Cell.Letter -> drawTextKey(canvas, cell.rect, c, cell === pressed,
                if (cell.label.length == 1) cell.label.uppercase() else cell.label, rowHeight * 0.44f)
            is Cell.Char -> drawTextKey(canvas, cell.rect, c, cell === pressed, cell.label, cell.rect.height() * 0.5f)
            is Cell.Special -> drawSpecial(canvas, cell, c)
            is Cell.Suggest -> Unit
        }

        if (gesturing && trail.size > 1) {
            trailPaint.color = c.accent
            trailPaint.alpha = 150
            trailPaint.strokeWidth = dp(5f)
            path.reset()
            path.moveTo(trail[0].x, trail[0].y)
            for (i in 1 until trail.size) path.lineTo(trail[i].x, trail[i].y)
            canvas.drawPath(path, trailPaint)
        }
    }

    /**
     * The finger's pixel x, mapped to a logical-space point by piecewise-linear
     * interpolation across [cell]'s letter row (through each key's logical
     * centre), extrapolating past the end keys. This keeps the map continuous
     * across key boundaries — a touch between two keys resolves to a logical
     * point between their centres — so the core's adaptive tap model can decide
     * the winner. The logical y is the row's, since taps rarely cross rows.
     */
    private fun logicalTouch(cell: Cell.Letter, tx: Float): PointF {
        val row = cells.asSequence()
            .filterIsInstance<Cell.Letter>()
            .filter { kotlin.math.abs(it.rect.top - cell.rect.top) < 1f }
            .sortedBy { it.rect.centerX() }
            .toList()
        if (row.size < 2) {
            val sx = if (cell.rect.width() > 0f) cell.lw / cell.rect.width() else 1f
            return PointF(cell.lx + (tx - cell.rect.centerX()) * sx, cell.ly)
        }
        var i = 0
        while (i < row.size - 2 && tx >= row[i + 1].rect.centerX()) i++
        val lo = row[i]; val hi = row[i + 1]
        val cxLo = lo.rect.centerX(); val cxHi = hi.rect.centerX()
        val t = if (cxHi != cxLo) (tx - cxLo) / (cxHi - cxLo) else 0f
        return PointF(lo.lx + t * (hi.lx - lo.lx), cell.ly)
    }

    private fun letterCenters(): Map<Char, PointF> {
        val m = HashMap<Char, PointF>()
        for (cell in cells) if (cell is Cell.Letter && cell.label.length == 1) {
            m[cell.label.first().lowercaseChar()] = PointF(cell.rect.centerX(), cell.rect.centerY())
        }
        return m
    }

    private fun keyBg(canvas: Canvas, r: RectF, c: Palette, isPressed: Boolean) {
        canvas.drawRoundRect(r.left, r.top + dp(1f), r.right, r.bottom + dp(1.5f), keyRadius, keyRadius, shadowPaint)
        canvas.drawRoundRect(r, keyRadius, keyRadius, if (isPressed) pressedPaint else keyPaint)
    }

    private fun drawTextKey(canvas: Canvas, r: RectF, c: Palette, isPressed: Boolean, text: String, size: Float) {
        keyBg(canvas, r, c, isPressed)
        labelPaint.textSize = size
        val cy = r.centerY() - (labelPaint.ascent() + labelPaint.descent()) / 2
        canvas.drawText(text, r.centerX(), cy, labelPaint)
    }

    private fun drawSpecial(canvas: Canvas, cell: Cell.Special, c: Palette) {
        val r = cell.rect; val p = cell === pressed
        when (cell.kind) {
            Sp.GLOBE -> drawGlobe(canvas, r, c)
            Sp.MIC -> drawMic(canvas, r, c)
            Sp.SHIFT -> { keyBg(canvas, r, c, p); drawShift(canvas, r, c) }
            Sp.BACKSPACE -> { keyBg(canvas, r, c, p); drawBackspace(canvas, r, c) }
            Sp.ENTER -> { keyBg(canvas, r, c, p); drawReturn(canvas, r, c) }
            Sp.SPACE -> {
                keyBg(canvas, r, c, p)
                hintPaint.textSize = funcRowHeight * 0.26f
                canvas.drawText(spaceHint, r.right - dp(12f), r.bottom - dp(10f), hintPaint)
            }
            Sp.TO_NUMBERS -> drawTextKey(canvas, r, c, p, "123", r.height() * 0.34f)
            Sp.TO_SYMBOLS -> drawTextKey(canvas, r, c, p, "#+=", r.height() * 0.34f)
            Sp.TO_ALPHA -> drawTextKey(canvas, r, c, p, "ABC", r.height() * 0.34f)
        }
    }

    // ---- Vector icons -------------------------------------------------------

    // Icons are the Lucide set (ISC-licensed), authored on a 24×24 grid as
    // stroked outlines, parsed once and scaled small into each key.
    private val matrix = Matrix()
    private val scaledIcon = Path()

    /** Stroke [icon] (a 24×24 path) into [r], centred, at [frac] of the key size. */
    private fun drawIcon(canvas: Canvas, icon: Path, r: RectF, frac: Float, paint: Paint) {
        val size = minOf(r.width(), r.height()) * frac
        val scale = size / 24f
        matrix.setScale(scale, scale)
        matrix.postTranslate(r.centerX() - size / 2f, r.centerY() - size / 2f)
        icon.transform(matrix, scaledIcon)
        canvas.drawPath(scaledIcon, paint)
    }

    private fun drawShift(canvas: Canvas, r: RectF, c: Palette) {
        if (shifted) {
            // Active: a solid arrow in the normal icon colour (not the accent).
            iconFill.color = c.icon
            drawIcon(canvas, ICON_SHIFT, r, ICON_FRAC, iconFill)
        } else {
            iconPaint.color = c.icon; iconPaint.strokeWidth = dp(1.7f)
            drawIcon(canvas, ICON_SHIFT, r, ICON_FRAC, iconPaint)
        }
    }

    private fun drawBackspace(canvas: Canvas, r: RectF, c: Palette) {
        iconPaint.color = c.icon; iconPaint.strokeWidth = dp(1.7f)
        drawIcon(canvas, ICON_BACKSPACE, r, ICON_FRAC, iconPaint)
    }

    private fun drawReturn(canvas: Canvas, r: RectF, c: Palette) {
        iconPaint.color = c.iconMuted; iconPaint.strokeWidth = dp(1.7f)
        drawIcon(canvas, ICON_RETURN, r, ICON_FRAC, iconPaint)
    }

    private fun drawGlobe(canvas: Canvas, r: RectF, c: Palette) {
        iconPaint.color = c.icon; iconPaint.strokeWidth = dp(1.6f)
        drawIcon(canvas, ICON_GLOBE, r, ICON_FRAC, iconPaint)
    }

    private fun drawMic(canvas: Canvas, r: RectF, c: Palette) {
        iconPaint.color = c.icon; iconPaint.strokeWidth = dp(1.6f)
        drawIcon(canvas, ICON_MIC, r, ICON_FRAC, iconPaint)
    }

    // ---- Touch --------------------------------------------------------------

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                val hit = cells.firstOrNull { it.rect.contains(event.x, event.y) }
                // A letter press may become a swipe: defer its commit to UP and
                // start tracking a path. Everything else fires immediately.
                if (page == Page.ALPHA && hit is Cell.Letter) {
                    gestureCell = hit
                    gesturing = false
                    trailLen = 0f
                    trail.clear(); trail.add(PointF(event.x, event.y))
                    pressed = hit; invalidate()
                } else {
                    gestureCell = null
                    pressed = hit; invalidate()
                    if (hit != null) fire(hit, event.x, event.y)
                }
                return true
            }
            MotionEvent.ACTION_MOVE -> {
                val g = gestureCell ?: return true
                val last = trail.lastOrNull()
                val p = PointF(event.x, event.y)
                if (last != null) trailLen += kotlin.math.hypot(p.x - last.x, p.y - last.y)
                trail.add(p)
                if (!gesturing && trailLen > gestureStartThreshold()) {
                    gesturing = true; pressed = null
                }
                if (gesturing) invalidate()
                return true
            }
            MotionEvent.ACTION_UP -> {
                val g = gestureCell
                if (g != null) {
                    if (gesturing && trail.size >= 3) {
                        onGesture?.invoke(ArrayList(trail), letterCenters())
                    } else {
                        // A tap after all: report the finger-down point so the
                        // core can learn this user's offset for the key.
                        val down = trail.firstOrNull()
                        fire(g, down?.x ?: g.rect.centerX(), down?.y ?: g.rect.centerY())
                    }
                }
                resetGesture()
                return true
            }
            MotionEvent.ACTION_CANCEL -> { resetGesture(); return true }
        }
        return super.onTouchEvent(event)
    }

    private fun gestureStartThreshold() = dp(26f)

    private fun resetGesture() {
        gestureCell = null; gesturing = false; trailLen = 0f
        trail.clear()
        if (pressed != null) pressed = null
        invalidate()
    }

    private fun fire(cell: Cell, tx: Float, ty: Float) {
        when (cell) {
            is Cell.Letter -> {
                // Map the finger to a *continuous* point in the core's logical
                // space so decode — not this pixel hit-test — picks the key. A
                // finger between two keys lands between their logical centres, so
                // the per-user tap model can pull a borderline touch to the key
                // the user meant. (A within-key mapping would snap to this key and
                // make the model inert.)
                val p = logicalTouch(cell, tx)
                onKeyTouch?.invoke(p.x, p.y)
            }
            is Cell.Char -> onCharKey?.invoke(cell.label)
            is Cell.Suggest -> onSuggestion?.invoke(cell.index)
            is Cell.Special -> when (cell.kind) {
                Sp.SHIFT -> shifted = !shifted
                Sp.BACKSPACE -> onFunctionKey?.invoke(FunctionKey.BACKSPACE)
                Sp.ENTER -> onFunctionKey?.invoke(FunctionKey.ENTER)
                Sp.SPACE -> onFunctionKey?.invoke(FunctionKey.SPACE)
                Sp.GLOBE -> onFunctionKey?.invoke(FunctionKey.GLOBE)
                Sp.MIC -> onFunctionKey?.invoke(FunctionKey.MIC)
                Sp.TO_NUMBERS -> { page = Page.NUMBERS; requestLayout(); invalidate() }
                Sp.TO_SYMBOLS -> { page = Page.SYMBOLS; requestLayout(); invalidate() }
                Sp.TO_ALPHA -> { page = Page.ALPHA; requestLayout(); invalidate() }
            }
        }
    }

    // ---- Theme --------------------------------------------------------------

    private class Palette(
        val bg: Int, val key: Int, val shadow: Int, val pressed: Int,
        val label: Int, val icon: Int, val iconMuted: Int, val accent: Int,
        val suggestion: Int, val divider: Int, val hint: Int,
    ) {
        companion object {
            val LIGHT = Palette(
                bg = Color.parseColor("#D1D5DB"), key = Color.parseColor("#FFFFFF"),
                shadow = Color.parseColor("#8D9099"), pressed = Color.parseColor("#E6E8EC"),
                label = Color.parseColor("#111114"), icon = Color.parseColor("#1A1A1D"),
                iconMuted = Color.parseColor("#8A8D93"), accent = Color.parseColor("#1177FF"),
                suggestion = Color.parseColor("#1A1A1D"), divider = Color.parseColor("#AEB2B9"),
                hint = Color.parseColor("#9A9DA3"),
            )
            val DARK = Palette(
                bg = Color.parseColor("#2B2B2E"), key = Color.parseColor("#6C6C70"),
                shadow = Color.parseColor("#141416"), pressed = Color.parseColor("#8A8A8E"),
                label = Color.parseColor("#FFFFFF"), icon = Color.parseColor("#F2F2F5"),
                iconMuted = Color.parseColor("#9A9A9E"), accent = Color.parseColor("#3B9BFF"),
                suggestion = Color.parseColor("#EDEDEF"), divider = Color.parseColor("#4A4A4E"),
                hint = Color.parseColor("#B8B8BC"),
            )
        }
    }

    private companion object {
        val NUMBERS_R1 = "1234567890".map { it.toString() }
        val NUMBERS_R2 = listOf("-", "/", ":", ";", "(", ")", "$", "&", "@", "\"")
        val SYMBOLS_R1 = listOf("[", "]", "{", "}", "#", "%", "^", "*", "+", "=")
        val SYMBOLS_R2 = listOf("_", "\\", "|", "~", "<", ">", "€", "£", "¥", "•")
        val PUNCT_R3 = listOf(".", ",", "?", "!", "'")

        // Special-key glyphs from the Lucide icon set (ISC-licensed), each on a
        // 24×24 grid; multi-part icons use absolute coordinates so the subpaths
        // concatenate safely. Rendered at half a key so they stay light and small.
        const val ICON_FRAC = 0.5f
        val ICON_SHIFT: Path = PathParser.createPathFromPathData(
            "M9 19a1 1 0 0 0 1 1h4a1 1 0 0 0 1-1v-6a1 1 0 0 1 1-1h3.293a.707.707 0 0 0 " +
                ".5-1.207l-7.086-7.086a1 1 0 0 0-1.414 0l-7.086 7.086a.707.707 0 0 0 .5 1.207H8a1 1 0 0 1 1 1z",
        )
        val ICON_BACKSPACE: Path = PathParser.createPathFromPathData(
            "M10 5a2 2 0 0 0-1.344.519l-6.328 5.74a1 1 0 0 0 0 1.481l6.328 5.741A2 2 0 0 0 " +
                "10 19h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2z M12 9L18 15 M18 9L12 15",
        )
        val ICON_RETURN: Path =
            PathParser.createPathFromPathData("M20 4v7a4 4 0 0 1-4 4H4 M9 10L4 15L9 20")
        val ICON_GLOBE: Path = PathParser.createPathFromPathData(
            "M2 12A10 10 0 1 1 22 12A10 10 0 1 1 2 12Z " +
                "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20 M2 12h20",
        )
        val ICON_MIC: Path = PathParser.createPathFromPathData(
            "M9 5A3 3 0 0 1 15 5V12A3 3 0 0 1 9 12Z M19 10v2a7 7 0 0 1-14 0v-2 M12 19v3",
        )

        /** Built-in QWERTY in the core's 1000×360 logical space, for decode fallback. */
        val FALLBACK_QWERTY: List<RenderKey> = buildList {
            fun row(chars: String, rowIndex: Int, x0: Float) {
                chars.forEachIndexed { i, ch -> add(RenderKey(ch.toString(), x0 + i * 100f, rowIndex * 120f, 100f, 120f)) }
            }
            row("qwertyuiop", 0, 0f); row("asdfghjkl", 1, 50f); row("zxcvbnm", 2, 150f)
        }
    }
}
