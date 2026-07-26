package com.featherkey.ime

/*
 * One-time migration of the legacy plaintext learning files into the encrypted
 * core (W6a). Earlier builds persisted learned data as two cleartext TSVs in the
 * app's private files dir — `usage.tsv` (word -> use-count) and `context.tsv`
 * (prev -> next -> count). Those are exactly the on-device secrets this project's
 * privacy posture requires to live only in the SecureStore (BR-13/BR-62), so on
 * the first launch of the new build we fold them into the core and remove them.
 *
 * Crash-safety: the import uses the core's SET-semantics (`importFrequencies` /
 * `importContext` replace counts, they do not accumulate), so re-running after a
 * crash mid-migration is idempotent. The steps run in the only safe order —
 * parse -> import -> persist -> delete -> mark — and each later step only runs if
 * the previous one succeeded (persist throws on failure, aborting before any file
 * is deleted). A one-shot marker means a device with no legacy files is scanned at
 * most once.
 *
 * The migration is NOT consent-gated. The consent toggle (BR-22) governs *new*
 * learning; this data was already recorded under consent by the old models, and
 * moving it into the encrypted store — the same data the core already reads for
 * ranking — is strictly a security improvement, not new learning. Gating it on
 * the async consent flow (which defaults to `false` before it emits) would risk
 * silently discarding the user's own history. Matches the un-gated core imports.
 */

import com.featherkey.ffi.FeatherKeyBridge
import com.featherkey.ffi.generated.FfiTransition
import com.featherkey.ffi.generated.FfiWordFreq
import java.io.File
import java.io.RandomAccessFile

object LegacyMigration {

    private const val USAGE_FILE = "usage.tsv"
    private const val CONTEXT_FILE = "context.tsv"
    /** Versioned so a future migration can define its own marker without clashing. */
    private const val MARKER_FILE = "legacy_migrated.v1"

    /**
     * Parse legacy `usage.tsv` (`word\tcount` per line) into set-import frequency
     * records. Mirrors the retired `UsageModel.load()`: split at the FIRST tab
     * (the count is the tail), keep only a non-empty word and a positive integer
     * count. Malformed lines are skipped, exactly as the old loader did.
     */
    fun parseUsage(text: String): List<FfiWordFreq> {
        val out = ArrayList<FfiWordFreq>()
        for (line in text.lineSequence()) {
            val tab = line.indexOf('\t')
            if (tab <= 0) continue
            val word = line.substring(0, tab)
            val count = line.substring(tab + 1).toIntOrNull() ?: continue
            if (word.isNotEmpty() && count > 0) out.add(FfiWordFreq(word, count.toUInt()))
        }
        return out
    }

    /**
     * Parse legacy `context.tsv` (`prev\tnext\tcount` per line) into set-import
     * transitions. Mirrors the retired `ContextModel.load()`: exactly three
     * tab-separated fields, non-empty `prev`/`next`, positive integer count.
     * Malformed lines are skipped.
     */
    fun parseContext(text: String): List<FfiTransition> {
        val out = ArrayList<FfiTransition>()
        for (line in text.lineSequence()) {
            val p = line.split('\t')
            if (p.size != 3) continue
            val prev = p[0]
            val next = p[1]
            val count = p[2].toIntOrNull() ?: continue
            if (prev.isEmpty() || next.isEmpty() || count <= 0) continue
            out.add(FfiTransition(prev, next, count.toUInt()))
        }
        return out
    }

    /** True if a migration still needs to run for [filesDir] (marker absent and at
     *  least one legacy file present). Cheap enough to call on every launch. */
    fun isPending(filesDir: File): Boolean {
        if (File(filesDir, MARKER_FILE).exists()) return false
        return File(filesDir, USAGE_FILE).exists() || File(filesDir, CONTEXT_FILE).exists()
    }

    /**
     * Run the one-time migration into [bridge], then secure-delete the legacy
     * files. Returns true if a migration actually ran (legacy files were present),
     * false if there was nothing to do. Any failure (parse aside — malformed lines
     * are skipped, not fatal) propagates so the caller leaves the files in place to
     * retry on the next launch.
     */
    fun migrate(filesDir: File, bridge: FeatherKeyBridge): Boolean =
        migrate(filesDir) { freqs, trans ->
            if (freqs.isNotEmpty()) bridge.importFrequencies(freqs)
            if (trans.isNotEmpty()) bridge.importContext(trans)
            bridge.persist() // throws on store failure -> aborts before any delete
        }

    /**
     * Testable core of [migrate]. [apply] receives the parsed records and MUST
     * import them and persist; it must throw if persistence fails so the files are
     * kept for a retry. Only after [apply] returns are the plaintext files
     * securely deleted and the marker written.
     */
    internal fun migrate(
        filesDir: File,
        apply: (List<FfiWordFreq>, List<FfiTransition>) -> Unit,
    ): Boolean {
        val marker = File(filesDir, MARKER_FILE)
        if (marker.exists()) return false
        val usage = File(filesDir, USAGE_FILE)
        val context = File(filesDir, CONTEXT_FILE)
        if (!usage.exists() && !context.exists()) {
            // Nothing to migrate on this device: record that we've checked so the
            // scan never runs again.
            runCatching { marker.createNewFile() }
            return false
        }

        val freqs = if (usage.exists()) parseUsage(usage.readText()) else emptyList()
        val trans = if (context.exists()) parseContext(context.readText()) else emptyList()

        // Import + persist. If this throws, we return without deleting or marking,
        // so the next launch retries (set-semantics makes the retry idempotent).
        apply(freqs, trans)

        secureDelete(usage)
        secureDelete(context)
        runCatching { marker.createNewFile() }
        return true
    }

    /**
     * Overwrite a plaintext learning file with zeros and fsync before unlinking, so
     * the raw words are not trivially recoverable from the freed blocks. Best-effort
     * — flash wear-levelling cannot be defeated from user space — but strictly
     * better than a bare `delete()` for the cleartext secrets this migration retires.
     */
    private fun secureDelete(file: File) {
        runCatching {
            if (!file.exists()) return
            val len = file.length()
            if (len > 0) {
                RandomAccessFile(file, "rw").use { raf ->
                    val zeros = ByteArray(4096)
                    var written = 0L
                    raf.seek(0)
                    while (written < len) {
                        val n = minOf(zeros.size.toLong(), len - written).toInt()
                        raf.write(zeros, 0, n)
                        written += n
                    }
                    raf.fd.sync()
                }
            }
            file.delete()
        }
    }
}
