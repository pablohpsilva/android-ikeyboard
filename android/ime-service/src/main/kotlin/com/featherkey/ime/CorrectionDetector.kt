package com.featherkey.ime

/*
 * Pure correction-signal logic for the input path, split out so it is
 * unit-testable without a live InputConnection or the native bridge (mirrors the
 * TypingRules pattern). The IME service feeds it the raw editing events it already
 * observes (an autocorrect commit, a backspace-undo, a suggestion pick, a
 * delete-then-retype); this turns them into the learning signals the core wants,
 * with a fixed one-slot lookback and no Android types.
 */

/** A learning signal derived from the user's correction behaviour. */
sealed class CorrectionSignal {
    /** The user immediately undid an autocorrect — [word] is what they had typed
     *  (the "from" the autocorrect replaced), a strong "leave my word alone" hint. */
    data class RevertAfterAutocorrect(val word: String) : CorrectionSignal()

    /** The user picked a suggestion that was not the top-ranked one for [prefix];
     *  [picked] is what they chose (only emitted when the pick's index > 0). */
    data class LowerRankedPick(val prefix: String, val picked: String) : CorrectionSignal()

    /** The user deleted a just-committed word and retyped — [oldWord] is the form
     *  they rejected (a low-weight negative signal). */
    data class DeleteRetype(val oldWord: String) : CorrectionSignal()
}

/**
 * A fixed one-slot lookback state machine over the IME's editing events. The only
 * retained state is the "from" word of the most recent autocorrect; any other
 * event clears it, so [onBackspaceUndo] emits [CorrectionSignal.RevertAfterAutocorrect]
 * only when the backspace *immediately* follows an autocorrect. The pick and
 * delete-retype events are stateless and report directly.
 *
 * Not thread-safe: intended to be driven from the single input thread, like the
 * rest of the service's per-keystroke logic.
 */
class CorrectionDetector {
    /** The "from" word of the last autocorrect, or null once consumed/cleared. */
    private var pendingAutocorrect: String? = null

    /** Record that the field autocorrected [from] → [to]. Arms the one-slot
     *  lookback so a following [onBackspaceUndo] is read as a revert. */
    fun onAutocorrect(from: String, to: String) {
        pendingAutocorrect = from
    }

    /** A backspace that undoes the last edit. Emits
     *  [CorrectionSignal.RevertAfterAutocorrect] iff it immediately follows an
     *  autocorrect; the signal is one-shot (the slot is cleared). */
    fun onBackspaceUndo(): CorrectionSignal? {
        val from = pendingAutocorrect ?: return null
        pendingAutocorrect = null
        return CorrectionSignal.RevertAfterAutocorrect(from)
    }

    /** The user picked suggestion [picked] at [index] for [prefix]. Emits
     *  [CorrectionSignal.LowerRankedPick] only when [index] > 0 (a non-top pick).
     *  Clears any pending autocorrect (this is a distinct event). */
    fun onSuggestionPicked(prefix: String, index: Int, picked: String): CorrectionSignal? {
        pendingAutocorrect = null
        return if (index > 0) CorrectionSignal.LowerRankedPick(prefix, picked) else null
    }

    /** The user deleted a committed word and retyped; [old] is the rejected form.
     *  Clears any pending autocorrect (this is a distinct event). */
    fun onDeleteRetype(old: String): CorrectionSignal {
        pendingAutocorrect = null
        return CorrectionSignal.DeleteRetype(old)
    }
}
