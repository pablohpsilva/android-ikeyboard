package com.featherkey.platform

/*
 * The user's chosen Latin key arrangement, independent of the selected
 * language(s) (BR-68). Like [LanguagePrefs], this preference flows to the *core*
 * (the IME pushes it via bridge.setLatinLayout on onStartInput), not to the view.
 * Plain SharedPreferences: a layout choice is a display preference, not personal
 * data, and the settings activity + IME share the app process.
 */

import android.content.Context

/** The pickable Latin arrangements. AUTO = match the system, else per-language default. */
enum class KeyboardLayoutChoice(val tag: String) {
    AUTO("auto"),
    QWERTY("qwerty"),
    QWERTZ("qwertz"),
    AZERTY("azerty");

    companion object {
        fun fromTag(tag: String?): KeyboardLayoutChoice =
            entries.firstOrNull { it.tag == tag } ?: AUTO
    }
}

class KeyboardLayoutPrefs(context: Context) {

    private val prefs = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    /** The chosen arrangement; defaults to AUTO. */
    fun choice(): KeyboardLayoutChoice = KeyboardLayoutChoice.fromTag(prefs.getString(KEY, null))

    fun setChoice(choice: KeyboardLayoutChoice) {
        prefs.edit().putString(KEY, choice.tag).apply()
    }

    private companion object {
        const val FILE = "featherkey_layout"
        const val KEY = "latin_layout"
    }
}
