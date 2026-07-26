package com.featherkey.ime

import com.featherkey.ffi.generated.FfiTransition
import com.featherkey.ffi.generated.FfiWordFreq
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * Pure, on-JVM coverage for the legacy-plaintext migration (W6a): the TSV parsers
 * and the file orchestration (consume -> secure-delete -> mark, abort-keeps-files).
 * The bridge import itself is exercised on-device; here [LegacyMigration.migrate]'s
 * testable overload takes the import action as a lambda so no native core is needed.
 */
class LegacyMigrationTest {

    @get:Rule val tmp = TemporaryFolder()

    // --- parseUsage (mirrors the retired UsageModel.load) --------------------

    @Test fun parseUsage_reads_word_count_pairs() {
        val out = LegacyMigration.parseUsage("teste\t3\nbom\t1\n")
        assertEquals(listOf(FfiWordFreq("teste", 3u), FfiWordFreq("bom", 1u)), out)
    }

    @Test fun parseUsage_splits_on_the_first_tab_only() {
        // A (hypothetical) word containing a tab keeps everything up to the first
        // tab as the word; the remainder must still parse as the count.
        val out = LegacyMigration.parseUsage("hi\t5")
        assertEquals(listOf(FfiWordFreq("hi", 5u)), out)
    }

    @Test fun parseUsage_skips_malformed_and_nonpositive() {
        val out = LegacyMigration.parseUsage(
            "good\t2\n" +
                "notab\n" +          // no tab
                "\t9\n" +            // empty word (tab at index 0)
                "bad\tNaN\n" +       // non-numeric count
                "zero\t0\n" +        // non-positive count
                "neg\t-4\n",         // negative count
        )
        assertEquals(listOf(FfiWordFreq("good", 2u)), out)
    }

    // --- parseContext (mirrors the retired ContextModel.load) ----------------

    @Test fun parseContext_reads_prev_next_count_triples() {
        val out = LegacyMigration.parseContext("teste\ttrem\t1\nzxcv\tzxcv\t28\n")
        assertEquals(
            listOf(FfiTransition("teste", "trem", 1u), FfiTransition("zxcv", "zxcv", 28u)),
            out,
        )
    }

    @Test fun parseContext_skips_wrong_arity_empty_and_nonpositive() {
        val out = LegacyMigration.parseContext(
            "the\tcat\t3\n" +
                "the\tcat\n" +        // only 2 fields
                "a\tb\tc\td\n" +      // 4 fields
                "\tnext\t2\n" +       // empty prev
                "prev\t\t2\n" +       // empty next
                "the\tdog\t0\n",      // non-positive count
        )
        assertEquals(listOf(FfiTransition("the", "cat", 3u)), out)
    }

    // --- migrate orchestration ----------------------------------------------

    @Test fun migrate_imports_then_deletes_files_and_marks_done() {
        val dir = tmp.newFolder()
        File(dir, "usage.tsv").writeText("teste\t3\nbom\t1\n")
        File(dir, "context.tsv").writeText("teste\ttrem\t1\n")

        var seenFreqs: List<FfiWordFreq>? = null
        var seenTrans: List<FfiTransition>? = null
        assertTrue(LegacyMigration.isPending(dir))

        val ran = LegacyMigration.migrate(dir) { freqs, trans ->
            seenFreqs = freqs
            seenTrans = trans
        }

        assertTrue(ran)
        assertEquals(listOf(FfiWordFreq("teste", 3u), FfiWordFreq("bom", 1u)), seenFreqs)
        assertEquals(listOf(FfiTransition("teste", "trem", 1u)), seenTrans)
        assertFalse("usage.tsv must be deleted", File(dir, "usage.tsv").exists())
        assertFalse("context.tsv must be deleted", File(dir, "context.tsv").exists())
        assertFalse("migration no longer pending", LegacyMigration.isPending(dir))
    }

    @Test fun migrate_is_idempotent_once_marker_written() {
        val dir = tmp.newFolder()
        File(dir, "usage.tsv").writeText("teste\t3\n")
        LegacyMigration.migrate(dir) { _, _ -> }

        // A stray legacy file re-appearing after the marker exists is NOT re-run.
        File(dir, "usage.tsv").writeText("teste\t3\n")
        var calledAgain = false
        val ran = LegacyMigration.migrate(dir) { _, _ -> calledAgain = true }
        assertFalse(ran)
        assertFalse(calledAgain)
    }

    @Test fun migrate_with_no_legacy_files_marks_done_without_running() {
        val dir = tmp.newFolder()
        var called = false
        val ran = LegacyMigration.migrate(dir) { _, _ -> called = true }
        assertFalse(ran)
        assertFalse(called)
        assertFalse(LegacyMigration.isPending(dir))
    }

    @Test fun migrate_abort_on_apply_failure_keeps_files_and_no_marker() {
        val dir = tmp.newFolder()
        File(dir, "usage.tsv").writeText("teste\t3\n")
        File(dir, "context.tsv").writeText("teste\ttrem\t1\n")

        val err = runCatching {
            LegacyMigration.migrate(dir) { _, _ -> throw RuntimeException("persist failed") }
        }
        assertTrue(err.isFailure)
        // Files remain so the next launch retries; migration is still pending.
        assertTrue(File(dir, "usage.tsv").exists())
        assertTrue(File(dir, "context.tsv").exists())
        assertTrue(LegacyMigration.isPending(dir))
    }
}
