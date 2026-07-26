package com.featherkey.platform

/*
 * BR-26 — classify the current editor field as sensitive so the core suppresses
 * learning/prediction. This is the shell-side input to the E-2 gate: the IME
 * builds it from `EditorInfo` and hands it to the core on every learn call.
 *
 * ⚠️ Authored, not device-verified.
 */

import android.text.InputType
import android.view.inputmethod.EditorInfo

object EditorInfoSensitivity {

    /**
     * True when the field must not be learned from: any password variation, or a
     * field that explicitly opts out of personalized learning
     * (`IME_FLAG_NO_PERSONALIZED_LEARNING`). Erring toward privacy — an unknown
     * or malformed field is treated as ordinary only when it is clearly so.
     */
    fun isSensitive(info: EditorInfo?): Boolean {
        if (info == null) return false
        val type = info.inputType
        val cls = type and InputType.TYPE_MASK_CLASS
        val variation = type and InputType.TYPE_MASK_VARIATION

        val isPassword = when (cls) {
            InputType.TYPE_CLASS_TEXT -> variation == InputType.TYPE_TEXT_VARIATION_PASSWORD ||
                variation == InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD ||
                variation == InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD
            InputType.TYPE_CLASS_NUMBER -> variation == InputType.TYPE_NUMBER_VARIATION_PASSWORD
            else -> false
        }

        val noLearning =
            (info.imeOptions and EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING) != 0

        return isPassword || noLearning
    }
}
