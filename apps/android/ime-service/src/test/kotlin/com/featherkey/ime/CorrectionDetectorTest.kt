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

    @Test fun reset_clears_a_pending_autocorrect() {
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        // The service calls reset() on any intervening event (a typed character, an
        // uncorrected word commit, a newline) so a later backspace is not a revert.
        d.reset()
        assertNull(d.onBackspaceUndo())
    }

    @Test fun reset_is_idempotent_with_no_pending_autocorrect() {
        val d = CorrectionDetector()
        d.reset()
        assertNull(d.onBackspaceUndo())
    }

    // --- Kept: a boundary/intervening edit after an unreverted autocorrect ----

    @Test fun an_intervening_edit_after_an_autocorrect_is_a_kept_signal() {
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        // The user typed on / hit a boundary instead of reverting: the correction
        // survived — reset (the intervening-edit hook) reports it as Kept.
        assertEquals(Outcome.KEPT, d.reset())
    }

    @Test fun reset_without_a_pending_autocorrect_reports_nothing() {
        val d = CorrectionDetector()
        assertNull(d.reset())
    }

    @Test fun a_reverted_autocorrect_does_not_also_report_kept() {
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        assertEquals(CorrectionSignal.RevertAfterAutocorrect("teh"), d.onBackspaceUndo())
        // The slot was consumed by the revert, so a following intervening edit is
        // not a second (contradictory) Kept signal for the same autocorrect.
        assertNull(d.reset())
    }

    @Test fun kept_is_a_one_shot_signal() {
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        assertEquals(Outcome.KEPT, d.reset())
        assertNull(d.reset())
    }

    @Test fun a_next_word_pick_after_an_autocorrect_still_surfaces_kept() {
        // BR-10 next-word flow: "teh " autocorrects to "the " (arms the lookback),
        // then the user taps a next-word suggestion before typing anything. The
        // service routes that pick's intervening edit through reset() FIRST, so the
        // survived autocorrect is reported KEPT and is not swallowed by the pick.
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        assertEquals(Outcome.KEPT, d.reset()) // the service's pre-pick hook
        // onSuggestionPicked then finds no pending autocorrect left to consume.
        assertNull(d.onSuggestionPicked(prefix = "", index = 0, picked = "cat"))
    }

    // --- Reached: the user manually lands on a withheld correction -----------

    @Test fun reaching_the_withheld_word_is_a_reached_signal() {
        val d = CorrectionDetector()
        d.noteWithheld("cat") // core withheld "cat" for a weak "xat"
        assertEquals(Outcome.REACHED, d.onManualWord("cat"))
        d.noteWithheld("cat")
        assertNull(d.onManualWord("dog")) // different word -> no signal
    }

    @Test fun a_manual_word_with_no_withheld_note_is_not_reached() {
        val d = CorrectionDetector()
        assertNull(d.onManualWord("cat"))
    }

    @Test fun reached_is_a_one_shot_signal() {
        val d = CorrectionDetector()
        d.noteWithheld("cat")
        assertEquals(Outcome.REACHED, d.onManualWord("cat"))
        // The counterfactual is consumed: a second landing is not a second signal.
        assertNull(d.onManualWord("cat"))
    }

    @Test fun a_non_matching_manual_word_leaves_the_withheld_note_intact() {
        val d = CorrectionDetector()
        d.noteWithheld("cat")
        // The weak word's own boundary self-check (shielded) does not expire the
        // note, so the immediately-following delete-then-retype reach still fires.
        assertNull(d.onManualWord("xat"))
        assertEquals(Outcome.REACHED, d.onManualWord("cat"))
    }

    @Test fun a_withheld_note_does_not_fire_reached_after_an_intervening_word() {
        // Mirrors the service: the boundary that typed the weak word "xat" both
        // notes the withheld "cat" AND self-checks it (shielded, so the note
        // survives). But if the user then commits an UNRELATED word instead of
        // reaching "cat", the note expires — so typing "cat" a sentence later is
        // NOT a false REACHED (bounded lifetime: BR-26/gate-quality safety).
        val d = CorrectionDetector()
        d.noteWithheld("cat")
        assertNull(d.onManualWord("xat")) // same-commit self-check, note kept
        assertNull(d.onManualWord("dog")) // an intervening word — expires the note
        assertNull(d.onManualWord("cat")) // reached late: must NOT be REACHED
    }

    @Test fun clear_drops_a_pending_autocorrect_and_a_withheld_note() {
        // A hard field boundary (onStartInput) must leak no correction judgment.
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        d.noteWithheld("cat")
        d.clear()
        assertNull(d.onBackspaceUndo())     // no revert leaks into the new field
        assertNull(d.onManualWord("cat"))   // no reach leaks into the new field
    }

    // --- Bounding the withheld note beyond the immediately-following commit --
    //
    // onManualWord already expires a stale note when the NEXT event IS a checked
    // manual word (an uncorrected boundary commit or a suggestion pick — see
    // a_withheld_note_does_not_fire_reached_after_an_intervening_word above). But
    // a swipe, a symbol/emoji, a whole-word delete-retype, a newline, or a
    // DIFFERENT word's own autocorrect never call onManualWord at all, so none of
    // them used to expire the note — it could survive indefinitely and fire a
    // stale Reached when the user later, unrelatedly, typed the withheld word.
    // expireWithheld() is the seam the service routes those events through
    // (alongside its existing reset()/clearCorrectionLookback call).

    @Test fun expire_withheld_drops_a_stale_note_without_a_signal() {
        val d = CorrectionDetector()
        d.noteWithheld("cat") // core withheld "cat" for a weak "xat"
        assertNull(d.onManualWord("xat")) // same-commit self-check (shielded): note kept
        // An intervening event that is not a checked manual word (a swipe, a
        // symbol/emoji, a delete-retype, a newline, another word's own autocorrect)
        // bounds the note's reach.
        d.expireWithheld()
        assertNull(d.onManualWord("cat")) // reached late, unrelated: must NOT be Reached
    }

    @Test fun expire_withheld_does_not_block_an_immediate_same_window_reach() {
        val d = CorrectionDetector()
        d.noteWithheld("cat")
        assertNull(d.onManualWord("xat")) // same-commit self-check (shielded): note kept
        // No intervening non-manual-word event happened: the immediately-following
        // manual landing on "cat" (delete-then-retype in the same window) still
        // reaches — expireWithheld is never called on this path.
        assertEquals(Outcome.REACHED, d.onManualWord("cat"))
    }

    @Test fun expire_withheld_is_idempotent_with_no_pending_note() {
        val d = CorrectionDetector()
        d.expireWithheld() // nothing to drop
        assertNull(d.onManualWord("cat"))
    }

    @Test fun expire_withheld_does_not_touch_a_pending_autocorrect() {
        // expireWithheld is the withheld-note-only half of clear(); it must not
        // also drop the unrelated one-slot revert lookback.
        val d = CorrectionDetector()
        d.onAutocorrect(from = "teh", to = "the")
        d.expireWithheld()
        assertEquals(CorrectionSignal.RevertAfterAutocorrect("teh"), d.onBackspaceUndo())
    }
}
