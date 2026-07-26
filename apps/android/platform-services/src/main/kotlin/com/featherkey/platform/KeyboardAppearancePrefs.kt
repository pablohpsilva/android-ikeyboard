package com.featherkey.platform

/*
 * Keyboard appearance preferences — the "Typing" section of settings (a gap the
 * design brief called out: height, key outlines and haptics were all absent).
 *
 * Every value here actually drives the keyboard: the IME reads this store on
 * `onStartInput` and pushes the values into [com.featherkey.keyboard.KeyboardView]
 * (see FeatherKeyImeService.applyAppearance), the same read-on-next-field pattern
 * [LanguagePrefs] uses. Nothing here is personal data — it is display preference —
 * so it is plain, synchronous SharedPreferences shared across the app process.
 *
 * ⚠️ Authored, not compiled.
 */

import android.content.Context

/**
 * Keyboard height presets. The scale multiplies the view's row/strip/function/bar
 * bands (not the gaps or corner radius), so the whole board grows or shrinks
 * proportionally while key spacing stays constant.
 */
enum class KeyboardHeight(val tag: String, val scale: Float) {
    COMPACT("compact", 0.76f),
    STANDARD("standard", 0.88f),
    TALL("tall", 1.0f);

    companion object {
        fun fromTag(tag: String?): KeyboardHeight =
            entries.firstOrNull { it.tag == tag } ?: STANDARD
    }
}

/** A snapshot of the appearance preferences, read together for one field. */
data class KeyboardAppearance(
    val height: KeyboardHeight,
    val keyOutlines: Boolean,
    val haptics: Boolean,
)

class KeyboardAppearancePrefs(context: Context) {

    private val prefs = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    fun height(): KeyboardHeight = KeyboardHeight.fromTag(prefs.getString(KEY_HEIGHT, null))

    fun setHeight(height: KeyboardHeight) {
        prefs.edit().putString(KEY_HEIGHT, height.tag).apply()
    }

    /** Draw a hairline outline around each key. Off by default (iOS-style flat keys). */
    fun keyOutlines(): Boolean = prefs.getBoolean(KEY_OUTLINES, false)

    fun setKeyOutlines(enabled: Boolean) {
        prefs.edit().putBoolean(KEY_OUTLINES, enabled).apply()
    }

    /** Haptic tick on each key press. On by default. */
    fun haptics(): Boolean = prefs.getBoolean(KEY_HAPTICS, true)

    fun setHaptics(enabled: Boolean) {
        prefs.edit().putBoolean(KEY_HAPTICS, enabled).apply()
    }

    /** Read all three at once — what the IME hands to the keyboard view per field. */
    fun snapshot(): KeyboardAppearance =
        KeyboardAppearance(height(), keyOutlines(), haptics())

    private companion object {
        const val FILE = "featherkey_appearance"
        const val KEY_HEIGHT = "height"
        const val KEY_OUTLINES = "key_outlines"
        const val KEY_HAPTICS = "haptics"
    }
}
