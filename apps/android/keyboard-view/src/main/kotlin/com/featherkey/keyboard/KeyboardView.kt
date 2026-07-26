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

import android.animation.ValueAnimator
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
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.View
import android.view.animation.PathInterpolator
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

    /** An emoji was tapped on the emoji page: commit this exact string verbatim. */
    var onEmoji: ((String) -> Unit)? = null

    /** A long-press accent variant (or base letter) was chosen: commit it verbatim. */
    var onAccentKey: ((String) -> Unit)? = null

    private var keysVersion = 0

    /** The active alpha layout's keys (from the core). */
    var keys: List<RenderKey> = emptyList()
        set(value) { field = value; keysVersion++; requestLayout(); invalidate() }

    /**
     * Predictive suggestions shown in the reserved strip. The strip band is a
     * constant height while a standard page is shown (see [stripBand]): its
     * contents come and go as words are typed, but the band itself never resizes,
     * so the keys never shift under the finger mid-word. Changing the list only
     * repaints the strip text.
     */
    var suggestions: List<String> = emptyList()
        set(value) { field = value; invalidate() }

    /** Shift state (next letter uppercase; highlights the shift key). */
    var shifted: Boolean = false
        set(value) { field = value; invalidate() }

    /** Active-language hint shown on the space bar, e.g. "EN" or "EN PT". */
    var spaceHint: String = "EN"
        set(value) { field = value; invalidate() }

    /** Active-language tags (preference order, primary first) that order the
     *  long-press accent variants for the primary accented language. */
    var accentLangs: List<String> = emptyList()

    /** Recently-used emoji (most-recent-first); shown on the emoji page's recents tab. */
    var recents: List<String> = emptyList()
        set(value) { field = value; if (page == Page.EMOJI) invalidate() }

    // --- Appearance (driven by KeyboardAppearancePrefs via the IME; see
    // FeatherKeyImeService.applyAppearance). Defaults reproduce the original look,
    // so an unset view renders exactly as before. ---

    /**
     * Target height multiplier for the vertical bands (rows/strip/function/bar);
     * gaps stay constant. Set from the "Keyboard height" setting (compact /
     * standard / tall). The *rendered* scale is [animatedHeightScale], which eases
     * toward this: instantly on first creation (before the view is attached, so
     * the keyboard simply opens at the chosen size), but with a short ease when
     * the setting is changed while a keyboard is already on screen.
     */
    var heightScale: Float = 1f
        set(value) {
            val v = value.coerceIn(0.7f, 1.4f)
            if (v == field) return
            field = v
            if (isAttachedToWindow) animateHeightScale(v)
            else { animatedHeightScale = v; requestLayout(); invalidate() }
        }

    /** Draw a hairline outline around each key (off = flat, iOS-style). */
    var keyOutlines: Boolean = false
        set(value) { if (value != field) { field = value; invalidate() } }

    /** Emit a haptic tick when a key is pressed. */
    var hapticsEnabled: Boolean = true

    /** Apply all appearance prefs at once (the IME calls this per field). */
    fun applyAppearance(heightScale: Float, keyOutlines: Boolean, haptics: Boolean) {
        this.hapticsEnabled = haptics
        this.keyOutlines = keyOutlines
        this.heightScale = heightScale // triggers relayout/invalidate if changed
    }

    /** A key was pressed: a light haptic tick, if enabled and the device supports it. */
    private fun keyPressFeedback() {
        if (hapticsEnabled) {
            performHapticFeedback(
                HapticFeedbackConstants.KEYBOARD_TAP,
                HapticFeedbackConstants.FLAG_IGNORE_GLOBAL_SETTING,
            )
        }
    }

    private enum class Page { ALPHA, NUMBERS, SYMBOLS, EMOJI }
    private var page = Page.ALPHA

    // --- Emoji-page state (own draw + scroll path, independent of the cell grid). ---
    /** Active emoji tab: 0 = recents, 1..N = [EmojiData.categories] in order. */
    private var emojiTab = 0
    /** Vertical scroll of the active tab's grid, in px; clamped to its content. */
    private var emojiScrollY = 0f
    private var emojiDownX = 0f
    private var emojiDownY = 0f
    private var emojiStartScroll = 0f
    private var emojiDragging = false
    private var emojiDownInGrid = false

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

    // --- Height animation state ---
    // [animatedHeightScale] is the scale actually rendered; it eases toward the
    // [heightScale] target (a live compact/standard/tall change). It drives
    // onMeasure, so the IME window height follows it frame by frame. The
    // suggestion band, by contrast, is a constant height (see [stripBand]) — it
    // is never animated, so typing never resizes the window.
    private var animatedHeightScale = 1f
    private var heightAnimator: ValueAnimator? = null
    // Material "standard" ease (fast-out, slow-in) for the height-scale change.
    private val easing = PathInterpolator(0.4f, 0f, 0.2f, 1f)

    // --- Geometry ---
    // The vertical bands scale with [animatedHeightScale] (the eased "Keyboard
    // height" setting); horizontal spacing (margins, gaps, radius) is left fixed
    // so the board grows in height without the keys drifting apart.
    private val stripHeight get() = dp(42f) * animatedHeightScale
    private val rowHeight get() = dp(52f) * animatedHeightScale
    private val funcRowHeight get() = dp(54f) * animatedHeightScale
    private val bottomBarHeight get() = dp(46f) * animatedHeightScale
    /** The reserved suggestion band's height — constant while a standard page is
     *  shown, so the keys stay put as suggestions come and go (they only repaint). */
    private val stripBand get() = stripHeight
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
    private val borderPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
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

    // --- Long-press accent popup state ---
    private val accentSession = AccentSession()
    private var accentPopup: RectF? = null       // popup band in view pixels
    private val longPressRunnable = Runnable { startAccentMode() }
    private fun longPressTimeoutMs() = 300L
    private fun accentActive() = accentSession.active

    // --- Backspace auto-repeat (press-and-hold to delete continuously) ---
    // The first delete fires on touch-down like any key; while the finger stays
    // down this runnable keeps firing backspaces on an accelerating cadence
    // ([KeyRepeat]) until UP/CANCEL removes it.
    private var backspaceDelay = KeyRepeat.START_MS
    private val backspaceRepeat = object : Runnable {
        override fun run() {
            onFunctionKey?.invoke(FunctionKey.BACKSPACE)
            backspaceDelay = KeyRepeat.next(backspaceDelay)
            postDelayed(this, backspaceDelay)
        }
    }
    private fun startBackspaceRepeat() {
        backspaceDelay = KeyRepeat.START_MS
        postDelayed(backspaceRepeat, KeyRepeat.INITIAL_MS)
    }
    private fun stopBackspaceRepeat() = removeCallbacks(backspaceRepeat)
    /** True while a hold on the emoji page's backspace is repeating. */
    private var emojiBackspaceHeld = false

    private enum class Sp { SHIFT, BACKSPACE, ENTER, SPACE, GLOBE, MIC, TO_NUMBERS, TO_SYMBOLS, TO_ALPHA, TO_EMOJI }

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

    private var cachedCells: List<Cell>? = null
    private var cachedKey: CellLayoutKey? = null

    private fun layoutCells(): List<Cell> {
        val key = CellLayoutKey(width, height, page.ordinal, keysVersion)
        val hit = cachedCells
        if (hit != null && key == cachedKey) return hit
        val built = buildCells(width, height)
        cachedCells = built; cachedKey = key
        return built
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val w = MeasureSpec.getSize(widthMeasureSpec)
        val stripReserved = page != Page.EMOJI
        val h = KeyboardGeometry.totalHeightPx(
            stripReserved = stripReserved,
            rowPx = rowHeight, funcPx = funcRowHeight, barPx = bottomBarHeight,
            insetPx = bottomInset.toFloat(), stripPx = stripBand,
        )
        setMeasuredDimension(w, h.toInt())
    }

    // ---- Layout -------------------------------------------------------------

    private fun buildCells(w: Int, h: Int): List<Cell> {
        // The emoji page draws and hit-tests itself (its grid is far too many keys
        // to model as [Cell]s), so it contributes no cells to the standard grid.
        if (page == Page.EMOJI) return emptyList()
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

        // The suggestion band occupies the top [stripBand] px (a constant height)
        // and the key grid starts just below it. The three Suggest cells span the
        // band; they take taps only where a suggestion is actually drawn.
        val band = stripBand
        val cw = w / 3f
        for (i in 0..2) out += Cell.Suggest(RectF(i * cw, 0f, (i + 1) * cw, band), i)
        var top = KeyboardGeometry.contentTopPx(stripReserved = true, stripPx = band)

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
            Page.EMOJI -> Unit // handled by the early return above; builds no cells
        }

        // Function row: [123|ABC] [emoji] [ space ] [ return ]. The emoji key sits
        // between the page-switch key and the space bar (iOS-style), so the emoji
        // page is reachable from every standard page; the space bar takes the rest.
        run {
            val kt = top + rowGap / 2f; val kb = top + funcRowHeight - rowGap / 2f
            val fSideW = baseKeyW * 2f
            val leftKind = if (page == Page.ALPHA) Sp.TO_NUMBERS else Sp.TO_ALPHA
            out += Cell.Special(RectF(sideMargin, kt, sideMargin + fSideW, kb), leftKind)
            val retLeft = w - sideMargin - fSideW
            out += Cell.Special(RectF(retLeft, kt, w - sideMargin, kb), Sp.ENTER)
            val emojiLeft = sideMargin + fSideW + keyGap
            val emojiW = baseKeyW * 1.2f
            out += Cell.Special(RectF(emojiLeft, kt, emojiLeft + emojiW, kb), Sp.TO_EMOJI)
            out += Cell.Special(RectF(emojiLeft + emojiW + keyGap, kt, retLeft - keyGap, kb), Sp.SPACE)
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

        // The emoji page renders itself (tabs + scrollable grid + control bar) and
        // takes no part in the cell grid or the prediction strip below.
        if (page == Page.EMOJI) { cells = emptyList(); drawEmojiPage(canvas, c); return }

        cells = layoutCells()

        // Suggestion strip. The band is a constant height; only its contents (the
        // words and the dividers between them) come and go, so nothing here resizes
        // the view.
        if (suggestions.isNotEmpty()) {
            val band = stripBand
            labelPaint.color = c.suggestion
            labelPaint.textSize = stripHeight * 0.42f
            for (cell in cells.filterIsInstance<Cell.Suggest>()) {
                val word = suggestions.getOrNull(cell.index) ?: continue
                val cy = cell.rect.centerY() - (labelPaint.ascent() + labelPaint.descent()) / 2
                canvas.drawText(word, cell.rect.centerX(), cy, labelPaint)
            }
            for (i in 1 until 3) if (i < suggestions.size) {
                val x = width / 3f * i
                canvas.drawLine(x, band * 0.28f, x, band * 0.72f, dividerPaint)
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

        if (accentActive()) drawAccentPopup(canvas, c)
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
        // "Key outlines" setting: a hairline border for users who want defined keys.
        if (keyOutlines) {
            borderPaint.color = c.border
            borderPaint.strokeWidth = dp(1f)
            canvas.drawRoundRect(r, keyRadius, keyRadius, borderPaint)
        }
    }

    private fun drawTextKey(canvas: Canvas, r: RectF, c: Palette, isPressed: Boolean, text: String, size: Float) {
        keyBg(canvas, r, c, isPressed)
        labelPaint.textSize = size
        val cy = r.centerY() - (labelPaint.ascent() + labelPaint.descent()) / 2
        canvas.drawText(text, r.centerX(), cy, labelPaint)
    }

    private fun drawAccentPopup(canvas: Canvas, c: Palette) {
        val rect = accentPopup ?: return
        val variants = accentSession.variants
        val n = variants.size
        if (n == 0) return
        val cellW = rect.width() / n
        // Shadow + base plate.
        canvas.drawRoundRect(rect.left, rect.top + dp(1f), rect.right, rect.bottom + dp(1.5f),
            keyRadius, keyRadius, shadowPaint)
        canvas.drawRoundRect(rect, keyRadius, keyRadius, keyPaint)
        labelPaint.textSize = rowHeight * 0.44f
        for (i in 0 until n) {
            val cell = RectF(rect.left + i * cellW, rect.top, rect.left + (i + 1) * cellW, rect.bottom)
            if (i == accentSession.index) {
                iconFill.color = c.accent
                canvas.drawRoundRect(cell, keyRadius, keyRadius, iconFill)
                labelPaint.color = Color.WHITE
            } else {
                labelPaint.color = c.label
            }
            val text = if (shifted) variants[i].uppercase() else variants[i]
            val cy = cell.centerY() - (labelPaint.ascent() + labelPaint.descent()) / 2
            canvas.drawText(text, cell.centerX(), cy, labelPaint)
        }
        labelPaint.color = c.label
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
            Sp.TO_EMOJI -> {
                keyBg(canvas, r, c, p)
                iconPaint.color = c.icon; iconPaint.strokeWidth = dp(1.6f)
                drawIcon(canvas, ICON_SMILE, r, ICON_FRAC, iconPaint)
            }
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
        // Same colour as the other key icons (backspace/shift/globe) — the return
        // key is not de-emphasised.
        iconPaint.color = c.icon; iconPaint.strokeWidth = dp(1.7f)
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
        // The emoji page has its own tap-vs-drag scroll model; keep it off the
        // letter/gesture path entirely.
        if (page == Page.EMOJI) return onEmojiTouch(event)
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                // A finger between keys lands in the small gap between two rects and
                // used to hit nothing — the tap vanished ("I typed but got nothing").
                // Fall back to the nearest cell so no in-area tap is ever dropped;
                // for letters, decode + the per-user tap model then resolve which
                // key was meant. The threshold keeps a tap on the empty strip (a
                // whole row away) from snapping to a letter.
                val hit = cells.firstOrNull { it.rect.contains(event.x, event.y) }
                    ?: nearestCell(event.x, event.y)
                if (hit != null) keyPressFeedback()
                // A letter press may become a swipe: defer its commit to UP and
                // start tracking a path. Everything else fires immediately.
                if (page == Page.ALPHA && hit is Cell.Letter) {
                    gestureCell = hit
                    gesturing = false
                    trailLen = 0f
                    trail.clear(); trail.add(PointF(event.x, event.y))
                    pressed = hit; invalidate()
                    if (Accents.hasVariants(hit.label.firstOrNull() ?: ' ')) {
                        postDelayed(longPressRunnable, longPressTimeoutMs())
                    }
                } else {
                    gestureCell = null
                    pressed = hit; invalidate()
                    if (hit != null) fire(hit, event.x, event.y)
                    // Holding backspace repeats the delete until the finger lifts.
                    if (hit is Cell.Special && hit.kind == Sp.BACKSPACE) startBackspaceRepeat()
                }
                return true
            }
            MotionEvent.ACTION_MOVE -> {
                if (accentActive()) { updateAccentSelection(event.x); return true }
                val g = gestureCell ?: return true
                val last = trail.lastOrNull()
                val p = PointF(event.x, event.y)
                if (last != null) trailLen += kotlin.math.hypot(p.x - last.x, p.y - last.y)
                trail.add(p)
                if (!gesturing && trailLen > gestureStartThreshold()) {
                    gesturing = true; pressed = null
                    removeCallbacks(longPressRunnable) // finger moved: it's a swipe
                }
                if (gesturing) invalidate()
                return true
            }
            MotionEvent.ACTION_UP -> {
                removeCallbacks(longPressRunnable)
                stopBackspaceRepeat()
                if (accentActive()) {
                    val chosen = accentSession.release()
                    if (chosen != null) onAccentKey?.invoke(chosen)
                    resetAccent(); resetGesture()
                    return true
                }
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
            MotionEvent.ACTION_CANCEL -> {
                removeCallbacks(longPressRunnable); stopBackspaceRepeat()
                resetAccent(); resetGesture(); return true
            }
        }
        return super.onTouchEvent(event)
    }

    private fun gestureStartThreshold() = dp(26f)

    /** The cell nearest to (x,y) if it is within a key's reach — used to rescue a
     *  tap that fell in an inter-key gap. Returns null when the point is far from
     *  every cell (e.g. deep in the empty suggestion strip), so such taps stay
     *  no-ops rather than typing a stray letter. */
    private fun nearestCell(x: Float, y: Float): Cell? {
        var best: Cell? = null
        var bestD = Float.MAX_VALUE
        for (c in cells) {
            val d = distanceToRect(c.rect, x, y)
            if (d < bestD) { bestD = d; best = c }
        }
        return if (bestD <= rowHeight * 0.6f) best else null
    }

    /** Euclidean distance from a point to a rectangle (0 when inside). */
    private fun distanceToRect(r: RectF, x: Float, y: Float): Float {
        val dx = when { x < r.left -> r.left - x; x > r.right -> x - r.right; else -> 0f }
        val dy = when { y < r.top -> r.top - y; y > r.bottom -> y - r.bottom; else -> 0f }
        return kotlin.math.hypot(dx, dy)
    }

    private fun resetGesture() {
        gestureCell = null; gesturing = false; trailLen = 0f
        trail.clear()
        if (pressed != null) pressed = null
        invalidate()
    }

    /** Fired by the long-press timer: if the held letter has accents, open the popup. */
    private fun startAccentMode() {
        val base = gestureCell ?: return
        val ch = base.label.firstOrNull() ?: return
        if (!accentSession.open(ch, accentLangs)) return
        gesturing = false                        // long-press wins over swipe
        accentPopup = accentPopupRect(base, accentSession.variants.size)
        pressed = base
        invalidate()
    }

    /** The popup band above [base]: one key-width cell per variant, centred over the
     *  key and clamped into the view; if it would clip the top, it is pinned to y=0. */
    private fun accentPopupRect(base: Cell.Letter, count: Int): RectF {
        // One key-width per cell, but never wider than the view: a large variant
        // set (some vowels have six) shrinks its cells to fit instead of running
        // off-screen. Downstream geometry reads cell width as totalW/count, so the
        // hit-test stays consistent with what is drawn.
        val usable = (width - 2 * sideMargin).coerceAtLeast(base.rect.width())
        val totalW = (base.rect.width() * count).coerceAtMost(usable)
        val left = (base.rect.centerX() - totalW / 2f)
            .coerceIn(sideMargin, (width - sideMargin - totalW).coerceAtLeast(sideMargin))
        val h = rowHeight
        val top = (base.rect.top - h - dp(6f)).coerceAtLeast(0f)
        return RectF(left, top, left + totalW, top + h)
    }

    private fun updateAccentSelection(x: Float) {
        val rect = accentPopup ?: return
        accentSession.moveTo(x, rect.left, rect.width() / accentSession.variants.size)
        invalidate()
    }

    private fun resetAccent() {
        accentSession.reset()
        accentPopup = null
        pressed = null
    }

    // ---- Height animation ---------------------------------------------------

    /** Ease the rendered height scale toward [target] (a live compact/standard/
     *  tall change), resizing the IME window frame by frame. */
    private fun animateHeightScale(target: Float) {
        if (animatedHeightScale == target) return
        heightAnimator?.cancel()
        heightAnimator = ValueAnimator.ofFloat(animatedHeightScale, target).apply {
            duration = HEIGHT_ANIM_MS
            interpolator = easing
            addUpdateListener {
                animatedHeightScale = it.animatedValue as Float
                requestLayout(); invalidate()
            }
            start()
        }
    }

    override fun onDetachedFromWindow() {
        super.onDetachedFromWindow()
        // Don't let the height animation or the repeat timer outlive the view.
        heightAnimator?.cancel()
        stopBackspaceRepeat()
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
                Sp.TO_EMOJI -> { page = Page.EMOJI; emojiScrollY = 0f; requestLayout(); invalidate() }
            }
        }
    }

    // ---- Emoji page ---------------------------------------------------------

    /**
     * The emoji page's geometry, computed once and shared by its draw and touch
     * paths so both agree on where the tabs, grid cells and control bar sit.
     */
    private class EmojiLayout(
        val tabCount: Int, val tabBottom: Float,
        val gridTop: Float, val gridBottom: Float,
        val cols: Int, val cellW: Float, val cellH: Float, val contentH: Float,
        val abc: RectF, val recents: RectF, val backspace: RectF,
    )

    private val emojiTabHeight get() = dp(44f)
    private val emojiDragThreshold get() = dp(8f)

    /** The active tab's emoji: recents for tab 0, else the category's list. */
    private fun emojiList(): List<String> =
        if (emojiTab == 0) recents else EmojiData.categories[emojiTab - 1].emojis

    /** The glyph for tab [i] — a clock for recents (0), else the category's tab glyph. */
    private fun emojiTabGlyph(i: Int): String =
        if (i == 0) RECENTS_GLYPH else EmojiData.categories[i - 1].tabGlyph

    private fun emojiLayout(): EmojiLayout {
        val w = width.toFloat(); val h = height.toFloat()
        val tabCount = 1 + EmojiData.categories.size
        val tabBottom = emojiTabHeight
        // A single control bar sits at the bottom (over the nav inset); the grid
        // fills everything between it and the tab band.
        val barTop = h - bottomInset - funcRowHeight
        val gridW = w - sideMargin * 2
        val cols = maxOf(1, (gridW / dp(46f)).toInt())
        val cellW = gridW / cols
        val cellH = cellW
        val rows = (emojiList().size + cols - 1) / cols
        val contentH = rows * cellH
        // Control bar: [ABC] [recents] … [backspace], keys sized like a func row.
        val baseKeyW = (gridW - keyGap * 9) / 10f
        val fSideW = baseKeyW * 2f
        val barKt = barTop + rowGap / 2f
        val barKb = h - bottomInset - rowGap / 2f
        val abc = RectF(sideMargin, barKt, sideMargin + fSideW, barKb)
        val recLeft = sideMargin + fSideW + keyGap
        val recents = RectF(recLeft, barKt, recLeft + fSideW, barKb)
        val backspace = RectF(w - sideMargin - fSideW, barKt, w - sideMargin, barKb)
        return EmojiLayout(tabCount, tabBottom, tabBottom, barTop, cols, cellW, cellH, contentH, abc, recents, backspace)
    }

    private fun drawEmojiPage(canvas: Canvas, c: Palette) {
        val L = emojiLayout()
        val maxScroll = maxOf(0f, L.contentH - (L.gridBottom - L.gridTop))
        emojiScrollY = emojiScrollY.coerceIn(0f, maxScroll)

        // Category tab band + its baseline divider, active tab underlined in accent.
        canvas.drawLine(0f, L.tabBottom, width.toFloat(), L.tabBottom, dividerPaint)
        labelPaint.color = c.label
        labelPaint.textSize = emojiTabHeight * 0.5f
        val tabW = width / L.tabCount.toFloat()
        val tabCy = L.tabBottom / 2f - (labelPaint.ascent() + labelPaint.descent()) / 2f
        for (i in 0 until L.tabCount) {
            canvas.drawText(emojiTabGlyph(i), tabW * (i + 0.5f), tabCy, labelPaint)
        }
        iconFill.color = c.accent
        canvas.drawRect(tabW * emojiTab + tabW * 0.3f, L.tabBottom - dp(3f),
            tabW * emojiTab + tabW * 0.7f, L.tabBottom - dp(1f), iconFill)

        // The scrollable grid, clipped to its band; skip rows scrolled out of view.
        val emojis = emojiList()
        canvas.save()
        canvas.clipRect(0f, L.gridTop, width.toFloat(), L.gridBottom)
        if (emojis.isEmpty()) {
            labelPaint.color = c.hint
            labelPaint.textSize = dp(15f)
            val my = (L.gridTop + L.gridBottom) / 2f - (labelPaint.ascent() + labelPaint.descent()) / 2f
            canvas.drawText("No recent emoji yet", width / 2f, my, labelPaint)
        } else {
            labelPaint.color = c.label
            labelPaint.textSize = minOf(L.cellW, L.cellH) * 0.62f
            for (i in emojis.indices) {
                val cx = sideMargin + (i % L.cols) * L.cellW + L.cellW / 2f
                val cy = L.gridTop - emojiScrollY + (i / L.cols) * L.cellH + L.cellH / 2f
                if (cy + L.cellH / 2f < L.gridTop || cy - L.cellH / 2f > L.gridBottom) continue
                val by = cy - (labelPaint.ascent() + labelPaint.descent()) / 2f
                canvas.drawText(emojis[i], cx, by, labelPaint)
            }
        }
        canvas.restore()

        // Control bar: return-to-letters, jump-to-recents, backspace.
        labelPaint.color = c.label
        drawTextKey(canvas, L.abc, c, false, "ABC", L.abc.height() * 0.34f)
        keyBg(canvas, L.recents, c, emojiTab == 0)
        iconPaint.color = c.icon; iconPaint.strokeWidth = dp(1.6f)
        drawIcon(canvas, ICON_CLOCK, L.recents, ICON_FRAC, iconPaint)
        keyBg(canvas, L.backspace, c, false)
        drawBackspace(canvas, L.backspace, c)
    }

    /**
     * The emoji page's touch model: a press that moves past [emojiDragThreshold]
     * inside the grid scrolls it; anything else is a tap, dispatched on UP to a
     * tab, an emoji cell, or a control-bar key.
     */
    private fun onEmojiTouch(event: MotionEvent): Boolean {
        val L = emojiLayout()
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                emojiDownX = event.x; emojiDownY = event.y
                emojiStartScroll = emojiScrollY
                emojiDragging = false
                emojiDownInGrid = event.y >= L.gridTop && event.y < L.gridBottom
                // Backspace on the emoji control bar holds-to-repeat like the main page.
                if (L.backspace.contains(event.x, event.y)) {
                    keyPressFeedback()
                    onFunctionKey?.invoke(FunctionKey.BACKSPACE)
                    startBackspaceRepeat()
                    emojiBackspaceHeld = true
                }
            }
            MotionEvent.ACTION_MOVE -> {
                if (emojiBackspaceHeld || !emojiDownInGrid) return true
                val dy = event.y - emojiDownY
                if (!emojiDragging &&
                    kotlin.math.hypot(event.x - emojiDownX, dy) > emojiDragThreshold) {
                    emojiDragging = true
                }
                if (emojiDragging) {
                    val maxScroll = maxOf(0f, L.contentH - (L.gridBottom - L.gridTop))
                    emojiScrollY = (emojiStartScroll - dy).coerceIn(0f, maxScroll)
                    invalidate()
                }
            }
            MotionEvent.ACTION_UP -> {
                if (emojiBackspaceHeld) {
                    stopBackspaceRepeat(); emojiBackspaceHeld = false
                } else if (!emojiDragging) {
                    keyPressFeedback(); onEmojiTap(L, event.x, event.y)
                }
                emojiDragging = false
            }
            MotionEvent.ACTION_CANCEL -> {
                stopBackspaceRepeat(); emojiBackspaceHeld = false; emojiDragging = false
            }
        }
        return true
    }

    private fun onEmojiTap(L: EmojiLayout, x: Float, y: Float) {
        when {
            y < L.tabBottom -> {
                val idx = ((x / width.toFloat()) * L.tabCount).toInt().coerceIn(0, L.tabCount - 1)
                if (idx != emojiTab) { emojiTab = idx; emojiScrollY = 0f; invalidate() }
            }
            y < L.gridBottom -> {
                val col = ((x - sideMargin) / L.cellW).toInt()
                if (col in 0 until L.cols) {
                    val row = ((y - L.gridTop + emojiScrollY) / L.cellH).toInt()
                    emojiList().getOrNull(row * L.cols + col)?.let { onEmoji?.invoke(it) }
                }
            }
            L.abc.contains(x, y) -> { page = Page.ALPHA; requestLayout(); invalidate() }
            L.recents.contains(x, y) -> if (emojiTab != 0) { emojiTab = 0; emojiScrollY = 0f; invalidate() }
            L.backspace.contains(x, y) -> onFunctionKey?.invoke(FunctionKey.BACKSPACE)
        }
    }

    // ---- Theme --------------------------------------------------------------

    private class Palette(
        val bg: Int, val key: Int, val shadow: Int, val pressed: Int,
        val label: Int, val icon: Int, val iconMuted: Int, val accent: Int,
        val suggestion: Int, val divider: Int, val hint: Int, val border: Int,
    ) {
        companion object {
            val LIGHT = Palette(
                bg = Color.parseColor("#D1D5DB"), key = Color.parseColor("#FFFFFF"),
                shadow = Color.parseColor("#8D9099"), pressed = Color.parseColor("#E6E8EC"),
                label = Color.parseColor("#111114"), icon = Color.parseColor("#1A1A1D"),
                iconMuted = Color.parseColor("#8A8D93"), accent = Color.parseColor("#1177FF"),
                suggestion = Color.parseColor("#1A1A1D"), divider = Color.parseColor("#AEB2B9"),
                hint = Color.parseColor("#9A9DA3"), border = Color.parseColor("#B7BBC2"),
            )
            val DARK = Palette(
                bg = Color.parseColor("#2B2B2E"), key = Color.parseColor("#6C6C70"),
                shadow = Color.parseColor("#141416"), pressed = Color.parseColor("#8A8A8E"),
                label = Color.parseColor("#FFFFFF"), icon = Color.parseColor("#F2F2F5"),
                iconMuted = Color.parseColor("#9A9A9E"), accent = Color.parseColor("#3B9BFF"),
                suggestion = Color.parseColor("#EDEDEF"), divider = Color.parseColor("#4A4A4E"),
                hint = Color.parseColor("#B8B8BC"), border = Color.parseColor("#54545A"),
            )
        }
    }

    private companion object {
        /** Duration for a live compact/standard/tall height-scale ease (ms). */
        const val HEIGHT_ANIM_MS = 200L

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
        // Emoji-entry (a smile) and the recents (clock) key on the emoji control
        // bar. The eyes are round-capped zero-length strokes, so they draw as dots.
        val ICON_SMILE: Path = PathParser.createPathFromPathData(
            "M2 12A10 10 0 1 1 22 12A10 10 0 1 1 2 12Z " +
                "M8 14c1.333 1.333 6.667 1.333 8 0 M9 9L9.01 9 M15 9L15.01 9",
        )
        val ICON_CLOCK: Path = PathParser.createPathFromPathData(
            "M2 12A10 10 0 1 1 22 12A10 10 0 1 1 2 12Z M12 6v6l4 2",
        )

        /** Tab glyph for the emoji page's recents tab (system emoji font). */
        const val RECENTS_GLYPH = "🕙" // 🕙

        /** Built-in QWERTY in the core's 1000×360 logical space, for decode fallback. */
        val FALLBACK_QWERTY: List<RenderKey> = buildList {
            fun row(chars: String, rowIndex: Int, x0: Float) {
                chars.forEachIndexed { i, ch -> add(RenderKey(ch.toString(), x0 + i * 100f, rowIndex * 120f, 100f, 120f)) }
            }
            row("qwertyuiop", 0, 0f); row("asdfghjkl", 1, 50f); row("zxcvbnm", 2, 150f)
        }
    }
}
