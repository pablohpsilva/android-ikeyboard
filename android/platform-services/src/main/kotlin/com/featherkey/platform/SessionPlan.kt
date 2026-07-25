package com.featherkey.platform

import java.util.Locale

/** A pure diff of spell-checker sessions: which languages to open/close and the
 * canonical active order. No Android session objects — just language codes, so
 * it is unit-testable off-device. */
data class SessionPlan(
    val open: List<String>,
    val close: List<String>,
    val order: List<String>,
) {
    companion object {
        fun of(openNow: Set<String>, desiredTags: List<String>): SessionPlan {
            val order = LinkedHashSet<String>()
            for (tag in desiredTags) {
                val lang = Locale.forLanguageTag(tag).language.ifEmpty { tag }
                if (lang.isNotEmpty()) order.add(lang)
            }
            val open = order.filter { it !in openNow }
            val close = openNow.filter { it !in order }
            return SessionPlan(open = open, close = close, order = order.toList())
        }
    }
}
