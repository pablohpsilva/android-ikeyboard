package com.featherkey.keyboard

import android.content.Context
import android.util.AttributeSet
import android.view.MotionEvent
import android.view.View

/**
 * The keyboard surface: renders keys and captures touch (SEDD §5.1).
 *
 * Single responsibility — it draws and it reports touches. It performs **no**
 * decoding, prediction, or text commitment; those cross the FFI into the Rust
 * core and return to [com.featherkey.ime.FeatherKeyImeService]. Keeping this
 * class Android-only and logic-free is what keeps the core host-testable
 * (SEDD §5.5 rule 2).
 *
 * NOTE: scaffold. Rendering (Canvas draw of the layout) is intentionally
 * omitted from the tracer bullet; only the touch → listener path is sketched.
 */
class KeyboardView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {

    /** Notified with surface-local coordinates for each key-down. */
    fun interface OnKeyTouchListener {
        fun onTouchAt(x: Float, y: Float)
    }

    var onKeyTouch: OnKeyTouchListener? = null

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (event.actionMasked == MotionEvent.ACTION_DOWN) {
            onKeyTouch?.onTouchAt(event.x, event.y)
            return true
        }
        return super.onTouchEvent(event)
    }
}
