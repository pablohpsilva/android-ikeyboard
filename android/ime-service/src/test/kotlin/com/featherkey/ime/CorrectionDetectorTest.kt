package com.featherkey.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Pure state-machine logic for the correction signals, unit-testable without a
 *  live InputConnection or the native bridge (like [TypingRulesTest]). */
class CorrectionDetectorTest {

    // --- Revert after autocorrect --------------------------------------------

    @Test fun revert_after_autocorrect_is_detected() {
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        val sig = d.onBackspaceUndo()
        assertEquals(CorrectionSignal.RevertAfterAutocorrect("teh"), sig)
    }

    @Test fun backspace_without_a_preceding_autocorrect_emits_nothing() {
        val d = CorrectionDetector()
        assertNull(d.onBackspaceUndo())
    }

    @Test fun revert_fires_only_immediately_after_the_autocorrect() {
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        // Any intervening event clears the 1-slot lookback.
        d.onSuggestionPicked(prefix = "ca", index = 0, picked = "cat")
        assertNull(d.onBackspaceUndo())
    }

    @Test fun revert_is_a_one_shot_signal() {
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        assertEquals(CorrectionSignal.RevertAfterAutocorrect("teh"), d.onBackspaceUndo())
        // A second backspace has no pending autocorrect to undo.
        assertNull(d.onBackspaceUndo())
    }

    // --- Lower-ranked pick ----------------------------------------------------

    @Test fun lower_ranked_pick_reports_prefix_and_word() {
        val d = CorrectionDetector()
        assertEquals(
            CorrectionSignal.LowerRankedPick("te", "teh"),
            d.onSuggestionPicked(prefix = "te", index = 1, picked = "teh"),
        )
    }

    @Test fun top_pick_emits_no_signal() {
        val d = CorrectionDetector()
        assertNull(d.onSuggestionPicked(prefix = "te", index = 0, picked = "the"))
    }

    @Test fun deeper_picks_still_report() {
        val d = CorrectionDetector()
        assertEquals(
            CorrectionSignal.LowerRankedPick("ca", "casa"),
            d.onSuggestionPicked(prefix = "ca", index = 3, picked = "casa"),
        )
    }

    // --- Delete-retype --------------------------------------------------------

    @Test fun delete_retype_reports_the_old_word() {
        val d = CorrectionDetector()
        assertEquals(
            CorrectionSignal.DeleteRetype("recieve"),
            d.onDeleteRetype(old = "recieve"),
        )
    }

    // --- The lookback is exactly one slot ------------------------------------

    @Test fun a_pick_clears_a_pending_autocorrect() {
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        d.onSuggestionPicked(prefix = "te", index = 1, picked = "teh")
        assertNull(d.onBackspaceUndo())
    }

    @Test fun a_delete_retype_clears_a_pending_autocorrect() {
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        d.onDeleteRetype(old = "the")
        assertNull(d.onBackspaceUndo())
    }
}
