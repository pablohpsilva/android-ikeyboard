package com.featherkey.platform

/*
 * The user's recently-used emoji, most-recent-first and deduped, for the emoji
 * page's "recents" tab. A capped ordered list is all we need, so it lives in
 * plain SharedPreferences: which emoji you picked is a convenience preference,
 * not personal text — it never goes through the tap-learning path.
 *
 * Emoji are multi-codepoint Strings, so the list is stored newline-joined (no
 * emoji contains a newline) rather than comma-joined, which a flag pair would
 * survive but a future separator choice might not.
 */

import android.content.Context

class EmojiRecents(context: Context) {

    private val prefs = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    /** The recents, most-recent-first (possibly empty). */
    fun list(): List<String> =
        prefs.getString(KEY, null)
            ?.split("\n")
            ?.filter { it.isNotEmpty() }
            .orEmpty()

    /** Record [emoji] as the most recent; dedupes and caps at [MAX]. Returns the new list. */
    fun record(emoji: String): List<String> {
        if (emoji.isEmpty()) return list()
        val next = (listOf(emoji) + list().filter { it != emoji }).take(MAX)
        prefs.edit().putString(KEY, next.joinToString("\n")).apply()
        return next
    }

    private companion object {
        const val FILE = "featherkey_emoji"
        const val KEY = "recents"
        const val MAX = 30
    }
}
