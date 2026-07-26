package com.featherkey.platform

/*
 * The user's chosen active languages, ordered — first is the *primary* (shown
 * first in the space bar, used as the display subtype and prediction tie-break).
 * Settings edits the whole set; the primary is simply the first entry.
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

    /**
     * Replace the active set (order preserved; never empty). The first time
     * Luxembourgish is added, silently also activate its German/French/English
     * companions (see [LanguageBundle]) — once, tracked by a one-shot flag, so a
     * user who later removes a companion is not fought.
     */
    fun setActiveTags(tags: List<String>) {
        val requested = tags.distinct().filter { it.isNotEmpty() }.ifEmpty { DEFAULT }
        val result = LanguageBundle.withCompanions(
            current = activeTags(),
            requested = requested,
            alreadyApplied = prefs.getBoolean(KEY_BUNDLE_APPLIED, false),
        )
        prefs.edit()
            .putString(KEY, result.tags.joinToString(","))
            .putBoolean(KEY_BUNDLE_APPLIED, result.bundleApplied)
            .apply()
    }

    /**
     * Rotate so the next language becomes primary; returns the new order.
     * Currently unused (the globe key opens the IME picker rather than cycling);
     * kept for a future in-keyboard language-switch affordance.
     */
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
        const val KEY_BUNDLE_APPLIED = "lb_bundle_applied"
        val DEFAULT = listOf("en")
    }
}
