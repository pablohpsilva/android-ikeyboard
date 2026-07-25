package com.featherkey.platform

/*
 * Luxembourgish is written by mixing German, French and English words. The Rust
 * core already code-switches between active languages, so when the user first
 * adds Luxembourgish we silently activate those companions too — once. This is
 * the pure decision (no Android dependencies); LanguagePrefs.setActiveTags wires
 * it in and persists the one-shot flag.
 */
object LanguageBundle {

    const val LB = "lb"

    /** Appended (in this order) when lb is first activated, if not already active. */
    val COMPANIONS = listOf("de", "fr", "en")

    data class Result(val tags: List<String>, val bundleApplied: Boolean)

    /**
     * If [requested] newly introduces `lb` (it is not in [current]) and the
     * bundle has not been applied before ([alreadyApplied] false), promote `lb`
     * to primary (first) — since the "Add" action appends it to the end — keep
     * the rest of the requested order, then append any missing [COMPANIONS].
     * Putting `lb` first is what makes its QWERTZ layout and momentum head-start
     * take effect. Otherwise return [requested] unchanged. Never re-applies once
     * applied.
     */
    fun withCompanions(
        current: List<String>,
        requested: List<String>,
        alreadyApplied: Boolean,
    ): Result {
        val lbNewlyAdded = requested.contains(LB) && !current.contains(LB)
        if (alreadyApplied || !lbNewlyAdded) return Result(requested, alreadyApplied)
        val merged = mutableListOf(LB)
        for (t in requested) if (t != LB && !merged.contains(t)) merged.add(t)
        for (c in COMPANIONS) if (!merged.contains(c)) merged.add(c)
        return Result(merged, true)
    }
}
