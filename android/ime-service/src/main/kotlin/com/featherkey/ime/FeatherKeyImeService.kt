package com.featherkey.ime

import android.inputmethodservice.InputMethodService
import android.view.View
import com.featherkey.keyboard.KeyboardView
import uniffi.featherkey.decodeTouch

/**
 * The IME entry point: owns the [InputMethodService] lifecycle and commits text
 * (SEDD §5.1 ime-service). It wires the keyboard surface's touches through the
 * FFI bridge to the Rust decoder and commits the returned character — the
 * Kotlin end of the keystroke tracer bullet.
 *
 * It holds no typing logic of its own: decoding lives in the Rust core
 * (`featherkey-input-decoder`). This class is a thin adapter (ARCH §9).
 *
 * NOTE: scaffold. `uniffi.featherkey.decodeTouch` is generated from
 * `ffi-bridge/src/featherkey.udl` once the Android/NDK build is wired up; this
 * file will not compile until then. See `android/README.md`.
 */
class FeatherKeyImeService : InputMethodService() {

    override fun onCreateInputView(): View {
        return KeyboardView(this).apply {
            onKeyTouch = KeyboardView.OnKeyTouchListener { x, y -> commitDecoded(x, y) }
        }
    }

    /** Decode a touch via the Rust core and commit the resulting character. */
    private fun commitDecoded(x: Float, y: Float) {
        val ch = decodeTouch(x, y) ?: return
        currentInputConnection?.commitText(ch, 1)
    }
}
