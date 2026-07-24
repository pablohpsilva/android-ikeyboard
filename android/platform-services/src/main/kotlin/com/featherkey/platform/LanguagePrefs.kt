package com.featherkey.platform

/*
 * The user's chosen active languages, ordered — first is the *primary* (shown
 * first in the space bar, used as the display subtype and prediction tie-break).
 * The globe key cycles the primary; settings edits the whole set.
 *
 * Plain SharedPreferences: a language choice is a preference, not personal data.
 * The IME service and the settings activity share the app process, so a write in
 * settings is visible to the IME on its next `onStartInput`.
 */

import android.content.Context

class LanguagePrefs(context: Context) {

    private val prefs = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    /** Ordered active tags; first is primary. Defaults to English. */
    fun activeTags(): List<String> {
        val tags = prefs.getString(KEY, null)
            ?.split(",")
            ?.map { it.trim() }
            ?.filter { it.isNotEmpty() }
            .orEmpty()
        return tags.ifEmpty { DEFAULT }
    }

    /** Replace the active set (order preserved; never empty). */
    fun setActiveTags(tags: List<String>) {
        val clean = tags.distinct().filter { it.isNotEmpty() }.ifEmpty { DEFAULT }
        prefs.edit().putString(KEY, clean.joinToString(",")).apply()
    }

    /** Rotate so the next language becomes primary; returns the new order. */
    fun cyclePrimary(): List<String> {
        val cur = activeTags()
        if (cur.size < 2) return cur
        val rotated = cur.drop(1) + cur.first()
        setActiveTags(rotated)
        return rotated
    }

    private companion object {
        const val FILE = "featherkey_languages"
        const val KEY = "active_tags"
        val DEFAULT = listOf("en")
    }
}
