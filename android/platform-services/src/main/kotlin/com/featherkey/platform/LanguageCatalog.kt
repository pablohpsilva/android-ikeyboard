package com.featherkey.platform

/*
 * The set of keyboard languages the app can offer, and which ship a word list
 * today. Multiple languages can be active at once (the Rust core's locale-manager
 * identifies each committed word's language); this catalog is what the settings
 * UI lists and the IME resolves tags against.
 *
 * A language with no bundled lexicon is still selectable — it becomes active but
 * contributes no predictions until `assets/lexicons/<tag>.txt` is added.
 */

import android.content.Context

/** A keyboard language: its tag, a human display name, and whether a word list ships. */
data class KeyboardLanguage(val tag: String, val displayName: String, val hasLexicon: Boolean)

object LanguageCatalog {

    /** Known languages in display order (tag → display name). */
    private val KNOWN = listOf(
        "en" to "English",
        "pt" to "Português",
        "es" to "Español",
        "fr" to "Français",
        "de" to "Deutsch",
        "it" to "Italiano",
        // Non-Latin scripts: selectable and typeable (the Rust core swaps the alpha
        // page to Cyrillic/Greek), but with no bundled word list yet — hasLexicon
        // resolves false until `assets/lexicons/<tag>.txt` ships, so no predictions.
        "ru" to "Русский",
        "el" to "Ελληνικά",
        // Luxembourgish: QWERTZ layout (shared with German) + a bundled lexicon.
        // hasLexicon flips true automatically once assets/lexicons/lb.txt ships.
        "lb" to "Lëtzebuergesch",
    )

    /** All selectable languages; [KeyboardLanguage.hasLexicon] = a bundled asset exists. */
    fun all(context: Context): List<KeyboardLanguage> {
        val assets = runCatching { context.assets.list("lexicons")?.toSet() }.getOrNull().orEmpty()
        return KNOWN.map { (tag, name) -> KeyboardLanguage(tag, name, assets.contains("$tag.txt")) }
    }

    fun displayName(tag: String): String = KNOWN.firstOrNull { it.first == tag }?.second ?: tag
}
