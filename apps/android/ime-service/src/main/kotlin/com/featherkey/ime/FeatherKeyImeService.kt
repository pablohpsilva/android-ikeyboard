package com.featherkey.ime

/*
 * The FeatherKey IME. Shell-side composition of the input path: touch → native
 * decode → commit, with word-boundary correct + gated learning, live prediction
 * strip, shift, page switching, plus globe (IME switch) and mic (system voice).
 *
 * Privacy invariants:
 *  - E-2 / BR-26: the field's sensitivity is captured on every onStartInput and
 *    passed to every learn call, so a password field is never learned from.
 *  - BR-29/30/31: native calls are wrapped so a fault at the seam degrades to a
 *    dropped keystroke, never a crash of the host editor.
 */

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.PointF
import android.inputmethodservice.InputMethodService
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.widget.Toast
import com.featherkey.ffi.AutocorrectOutcome
import com.featherkey.ffi.FeatherKeyBridge
import com.featherkey.ffi.FieldSensitivity
import com.featherkey.ffi.Language
import com.featherkey.ffi.LatinLayout
import com.featherkey.ffi.generated.FfiRankCandidate
import com.featherkey.ffi.generated.FfiSource
import com.featherkey.keyboard.FunctionKey
import com.featherkey.keyboard.KeyboardView
import com.featherkey.keyboard.RenderKey
import com.featherkey.onboarding.ConsentStore
import com.featherkey.platform.DeviceDictionary
import com.featherkey.platform.EditorInfoSensitivity
import com.featherkey.platform.EmojiRecents
import com.featherkey.platform.KeyboardAppearancePrefs
import com.featherkey.platform.KeyboardLayoutChoice
import com.featherkey.platform.KeyboardLayoutPrefs
import com.featherkey.platform.KeystoreKeyProvider
import com.featherkey.platform.LanguagePrefs
import com.featherkey.platform.PhysicalKeyboardLayout
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext

class FeatherKeyImeService : InputMethodService() {

    /** The native bridge. Opened off the main thread ([bridgeJob]) so the heavy
     *  keystore + lexicon + FST work never blocks the keyboard from appearing;
     *  null until that completes. Input-path callers gate on [ensureBridgeReady]. */
    @Volatile private var bridge: FeatherKeyBridge? = null
    /** The in-flight [bridge] open, joined by [ensureBridgeReady] on first use. */
    private var bridgeJob: Job? = null
    private val ioScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private var field: FieldSensitivity = FieldSensitivity { false }
    private val pending = StringBuilder()
    private var persistJob: Job? = null

    private var keyboard: KeyboardView? = null
    private var recognizer: SpeechRecognizer? = null

    private lateinit var langPrefs: LanguagePrefs
    /** Keyboard height / outlines / haptics prefs, re-read per field like languages. */
    private lateinit var appearancePrefs: KeyboardAppearancePrefs
    /** The user's chosen Latin key arrangement (BR-68), re-read per field. */
    private lateinit var layoutPrefs: KeyboardLayoutPrefs
    /** Persisted most-recently-used emoji for the emoji page's recents tab. */
    private lateinit var emojiRecents: EmojiRecents
    /** The active languages currently loaded into the core (order = preference). */
    private var currentTags: List<String> = emptyList()
    /** Frequency-ranked vocabulary for suggestions + swipe (loaded off the input path). */
    @Volatile private var vocab: Vocabulary = Vocabulary.empty()
    /** Swipe candidates bucketed by first key, rebuilt with [vocab]; scanned per
     *  gesture instead of re-deriving every word's key path each swipe. */
    @Volatile private var gestureIndex: GestureDecoder.Index = GestureDecoder.Index.EMPTY
    /** The in-flight [vocab] load, joined by [ensureVocabReady] on a cold-start gesture. */
    private var vocabJob: Job? = null
    /**
     * The device's own dictionary (Android TextServices) for the primary
     * language — the base vocabulary for scripts we bundle no word list for
     * (Russian, Greek, …). Its lookups are async; the callback refreshes the
     * strip. Never queried in a sensitive field (E-2/BR-26).
     */
    private lateinit var deviceDict: DeviceDictionary
    /** Derives correction learning signals (revert, lower-ranked pick, delete-retype)
     *  from the raw editing events; the core consumes the resulting observations. */
    private val corrections = CorrectionDetector()
    /** The last committed word, lowercased — the context for the next prediction. */
    @Volatile private var lastWord: String? = null
    /** The word last committed as a single atomic unit (swipe / picked suggestion),
     *  so a whole-word backspace can report the rejected form as a delete-retype. */
    private var lastAtomicWord: String? = null
    /**
     * Length of the text last committed as a single unit — a picked suggestion
     * (word + trailing space) or a swipe result — or 0 if the last edit was not
     * such a unit. While non-zero, the next single backspace deletes that whole
     * span instead of one character, so a wrong swipe or suggestion clears in one
     * tap. Any other edit (typing, space, emoji, a new field) disarms it.
     */
    private var atomicSpan: Int = 0
    /** The active field's inputType — drives [getCursorCapsMode] auto-capitalization. */
    private var editorInputType: Int = 0
    /** The active field's raw imeOptions — Enter routing (see [EnterKey]). */
    private var editorImeOptions: Int = 0
    /** The active field's requested IME action (Send/Search/Next/…), masked out. */
    private var editorAction: Int = EditorInfo.IME_ACTION_UNSPECIFIED
    /** Logical-space centre of each letter key, for computing tap offsets to learn. */
    @Volatile private var keyCenters: Map<Char, PointF> = emptyMap()
    /** The learning-consent toggle (BR-22); learning is off until opted in. */
    @Volatile private var learningEnabled = false

    // Coalesces multiple device-dictionary results that land within a frame into a
    // single strip refresh (each language answers separately; without this a
    // keystroke re-runs rankForStrip once per answering language).
    private val refreshStrip = Runnable { updateSuggestions() }

    override fun onCreate() {
        super.onCreate()
        langPrefs = LanguagePrefs(this)
        appearancePrefs = KeyboardAppearancePrefs(this)
        layoutPrefs = KeyboardLayoutPrefs(this)
        emojiRecents = EmojiRecents(this)
        currentTags = langPrefs.activeTags()
        // The device dictionary's async lookups refresh the strip on completion.
        deviceDict = DeviceDictionary(this) {
            keyboard?.removeCallbacks(refreshStrip)
            keyboard?.postDelayed(refreshStrip, DEVICE_REFRESH_COALESCE_MS)
        }
        deviceDict.setLanguages(currentTags)
        loadVocab(currentTags)
        ConsentStore(applicationContext).learningEnabled
            .onEach { learningEnabled = it }
            .launchIn(ioScope)
        // Open the native bridge off the main thread. Provisioning the keystore
        // key, parsing the lexicon word lists and building the core's FSTs together
        // cost ~1s+ on older devices; doing them here (in onCreate, on the main
        // thread) would block the keyboard from appearing that whole time. Instead
        // the keyboard shows immediately with the view's fallback QWERTY, letter
        // taps still decode (the tap model is geometry-only), and the first call
        // that truly needs the core waits for this via [ensureBridgeReady]. The
        // real layout swaps in when the open completes.
        bridgeJob = ioScope.launch {
            // Degrade-don't-crash (BR-29/30/31): a keystore, lexicon-parse or
            // store-open fault here must NOT crash the IME. An uncaught throw in
            // this coroutine kills the service — the keyboard then shows only the
            // suggestion strip and never reopens (a locked/corrupt redb after the
            // process is killed mid-write can make open() throw on every start). On
            // failure the bridge stays null: the view still renders its fallback
            // QWERTY and every bridge call site is null-safe, so typing survives.
            runCatching {
                val key = KeystoreKeyProvider(this@FeatherKeyImeService).provisionDataKey()
                try {
                    val dbPath = File(filesDir, "featherkey.redb").absolutePath
                    bridge = FeatherKeyBridge.open(dbPath, key, Lexicons.load(applicationContext, currentTags))
                } finally {
                    key.fill(0) // wipe the shell's copy whether or not open succeeded
                }
            }
            // One-time migration of legacy plaintext learning (usage.tsv/context.tsv)
            // into the now-open encrypted core, then secure-delete the cleartext.
            // Off the main thread, after open; failure leaves the files for a retry
            // on the next launch (set-semantics makes that idempotent). BR-13/BR-62.
            bridge?.let { b -> runCatching { LegacyMigration.migrate(filesDir, b) } }
            withContext(Dispatchers.Main) { keyboard?.let { it.keys = renderKeys() } }
        }
        // Ease the whole keyboard up when it shows and down when it hides.
        window?.window?.setWindowAnimations(R.style.ImeWindowAnimation)
    }

    /**
     * Block until the native bridge has finished opening, if it hasn't already.
     * The open runs off the main thread ([bridgeJob]); the input path can't decode
     * or rank without it, so the first keystroke/gesture joins the in-flight open
     * (reusing its work, never reloading). Only ever blocks once, and only for the
     * open's remaining time — which has been running since onCreate, so much of it
     * overlaps the user reaching for the first key. Mirrors [ensureVocabReady].
     */
    private fun ensureBridgeReady() {
        if (bridge != null) return
        runCatching { runBlocking { bridgeJob?.join() } }
    }

    override fun onCreateInputView(): View {
        val view = KeyboardView(this)
        view.keys = renderKeys() // fallback QWERTY until the bridge finishes opening
        view.spaceHint = spaceHint(currentTags)
        view.accentLangs = currentTags // orders long-press accents for the primary language
        view.onKeyTouch = { x, y -> handleTouch(x, y) }
        view.onCharKey = { ch -> handleChar(ch) }
        view.onFunctionKey = { fk -> handleFunction(fk) }
        view.onSuggestion = { i -> commitSuggestion(i) }
        view.onGesture = { pathPts, centers -> handleGesture(pathPts, centers) }
        view.onEmoji = { emoji -> handleEmoji(emoji) }
        view.onAccentKey = { ch -> handleAccent(ch) }
        view.recents = emojiRecents.list()
        keyboard = view
        applyAppearance()
        return view
    }

    /**
     * Push the current appearance preferences (height, key outlines, haptics) into
     * the keyboard view. Read synchronously and re-applied on each [onStartInput],
     * so a change made in settings takes effect on the next field — the same
     * pattern [applyLanguages] uses.
     */
    private fun applyAppearance() {
        val a = appearancePrefs.snapshot()
        keyboard?.applyAppearance(
            heightScale = a.height.scale,
            keyOutlines = a.keyOutlines,
            haptics = a.haptics,
        )
    }

    /**
     * Push the chosen Latin layout to the core, then re-pull the rendered keys so
     * render and decode stay in lockstep. Read-on-next-field, like [applyLanguages]
     * and [applyAppearance]. AUTO resolves to a probed physical-keyboard layout if
     * one is attached, else stays AUTO so the core uses the per-language default.
     */
    private fun applyLayout() {
        val choice = layoutPrefs.choice()
        val resolved = if (choice == KeyboardLayoutChoice.AUTO) {
            runCatching { PhysicalKeyboardLayout.detect() }.getOrNull() ?: KeyboardLayoutChoice.AUTO
        } else {
            choice
        }
        val kind = when (resolved) {
            KeyboardLayoutChoice.AUTO -> LatinLayout.AUTO
            KeyboardLayoutChoice.QWERTY -> LatinLayout.QWERTY
            KeyboardLayoutChoice.QWERTZ -> LatinLayout.QWERTZ
            KeyboardLayoutChoice.AZERTY -> LatinLayout.AZERTY
        }
        runCatching { bridge?.setLatinLayout(kind) }
        // The alpha page may have changed; re-pull keys (also refreshes keyCenters
        // for the tap model via renderKeys()'s `.also`, same as applyLanguages).
        keyboard?.let { it.keys = renderKeys() }
    }

    override fun onStartInput(info: EditorInfo?, restarting: Boolean) {
        super.onStartInput(info, restarting)
        val sensitive = EditorInfoSensitivity.isSensitive(info)
        field = FieldSensitivity { sensitive } // captured once per field (E-2)
        pending.clear()
        atomicSpan = 0
        lastWord = null // a new field starts with no preceding-word context
        corrections.clear() // a new field is a hard boundary: no correction
        // judgment (revert lookback or withheld note) leaks across fields
        editorInputType = info?.inputType ?: 0
        editorImeOptions = info?.imeOptions ?: 0
        editorAction = editorImeOptions and EditorInfo.IME_MASK_ACTION
        keyboard?.suggestions = emptyList()
        applyFieldLayout()
        // Pick up any language or appearance changes made in settings since the
        // last field (both are read synchronously and take effect from here on).
        applyLanguages(langPrefs.activeTags())
        applyLayout()
        applyAppearance()
        applyAutoCaps() // start a sentence/name field already shifted
    }

    /** Once the input view is actually up, arm auto-caps for the field. onStartInput
     *  can run before the view exists (keyboard == null there), so the first field's
     *  capitalization must be set here or it is lost. */
    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        applyFieldLayout()
        applyAutoCaps()
    }

    /** Apply the field-appropriate initial page + affix keys. Called from both
     *  onStartInput and onStartInputView because onStartInput can run before the
     *  view exists (keyboard == null there), so the first field's layout would
     *  otherwise be lost — the same reason applyAutoCaps is re-applied there. */
    private fun applyFieldLayout() {
        keyboard?.resetPage(FieldLayout.initialPage(editorInputType))
        keyboard?.affixKeys = FieldLayout.affixKeys(editorInputType)
    }

    /**
     * Auto-capitalization (precise but flexible). Arms the shift as a one-shot when
     * the caret sits where a capital belongs. It first honours the field's own
     * request via [InputConnection.getCursorCapsMode] (CAP_SENTENCES / CAP_WORDS /
     * CAP_CHARACTERS); for ordinary text editors that set no such flag — some note
     * apps among them — it falls back to detecting a sentence start ourselves
     * (start of field, after a newline, or after ". "/"! "/"? "), so capitalization
     * "just works". Never fires in password/email/URL/number fields, where a
     * capital would be wrong. One-shot: the next letter is upper-cased and shift
     * clears, and tapping shift first always wins, so a deliberate lowercase is
     * never fought. Called at every context change so caps track the caret.
     *
     * Goes through [KeyboardView.armShift], which is inert under caps lock — a
     * mid-word or post-space call must not cancel a lock the user asked for.
     */
    private fun applyAutoCaps() {
        val kb = keyboard ?: return
        val ic = currentInputConnection ?: return
        kb.armShift(
            AutoCaps.shouldCapitalize(
                editorInputType,
                ic.getCursorCapsMode(editorInputType),
                ic.getTextBeforeCursor(2, 0),
            ),
        )
    }

    /** Push [tags] to the core (if changed) and reflect them on the space bar. */
    private fun applyLanguages(tags: List<String>) {
        if (tags != currentTags) {
            runCatching { bridge?.setActiveLanguages(Lexicons.load(this, tags)) }
            currentTags = tags
            deviceDict.setLanguages(tags)
            loadVocab(tags)
            // The primary language may have changed the core's alpha script
            // (e.g. Latin → Cyrillic), so re-pull the rendered keys; renderKeys()
            // also refreshes keyCenters for the tap model via its `.also`.
            keyboard?.keys = renderKeys()
            keyboard?.accentLangs = tags // re-order long-press accents for the new primary
        }
        keyboard?.spaceHint = spaceHint(tags)
    }

    /** Build the frequency vocabulary off the input thread; swap it in when ready.
     *  The [Job] is kept so a gesture arriving before the load lands can wait for
     *  it (see [ensureVocabReady]) rather than decoding against an empty list. */
    private fun loadVocab(tags: List<String>) {
        vocabJob?.cancel()
        vocabJob = ioScope.launch {
            val loaded = Vocabulary.load(applicationContext, tags)
            gestureIndex = GestureDecoder.Index.build(loaded.words)
            vocab = loaded
            // On a cold start the strip may be empty for the word already being
            // typed (the load is async); refresh it now that the data is ready.
            keyboard?.post { updateSuggestions() }
        }
    }

    /**
     * Swipe decoding needs the full word list synchronously, but the vocabulary
     * loads asynchronously ([loadVocab]). On a cold start the very first gesture
     * can beat that load and — with an empty word set — decode to nothing (the
     * "first swipe does nothing, second works" bug). If the list isn't ready yet,
     * briefly join the in-flight load (reusing its work, not reloading) so the
     * first swipe decodes against a real vocabulary. Only ever blocks once, and
     * only for the load's remaining time.
     */
    private fun ensureVocabReady() {
        if (vocab.words.isNotEmpty()) return
        runCatching { runBlocking { vocabJob?.join() } }
    }

    /** A swipe over the letters: decode to a word, commit it, offer alternatives. */
    private fun handleGesture(pathPts: List<PointF>, centers: Map<Char, PointF>) {
        val ic = currentInputConnection ?: return
        atomicSpan = 0 // a fresh gesture; re-armed below only if it commits a word
        // A gesture is intervening input: clear the autocorrect revert lookback so a
        // later backspace can't spuriously whitelist a typo, even when this swipe
        // decodes to nothing (early return below). A swipe never checks the withheld
        // note (it doesn't call onManualWord), so also bound its reach here.
        clearCorrectionLookback(boundWithheld = true)
        ensureBridgeReady() // cold start: the first swipe waits for the core to open
        ensureVocabReady() // cold start: don't decode the first swipe against an empty list
        // Re-centre the keys by the core's learned per-key tap offsets and bias the
        // decode by the user's own learned word frequencies (both owned by the core).
        val shifted = shiftedCenters(centers)
        val learned = runCatching { bridge?.learnedFrequencies() }.getOrNull()
            ?.associate { it.word to it.freq.toInt() } ?: emptyMap()
        val words = GestureDecoder.decode(pathPts, shifted, gestureIndex, vocab::rankOf, learned, limit = 4)
        if (words.isEmpty()) return
        // Tag each decoded word by the languages that recognise it (fallback: the
        // primary language) and let the core ranker blend in language momentum.
        val fallback = currentTags.firstOrNull() ?: "en"
        val cands = ArrayList<FfiRankCandidate>()
        words.forEachIndexed { i, w ->
            val langs = vocab.languagesOf(w).ifEmpty { setOf(fallback) }
            for (lang in langs) cands.add(FfiRankCandidate(w, lang, FfiSource.LEXICON, i.toUInt()))
        }
        val ranked = runCatching { bridge?.rank(cands, SUGGESTIONS.toUInt())?.map { it.word } ?: emptyList() }
            .getOrDefault(emptyList())
        val best = ranked.firstOrNull() ?: words.firstOrNull() ?: return
        if (pending.isNotEmpty()) { // finalise a half-typed word with a space
            learnWord(pending.toString())
            ic.commitText(" ", 1)
            pending.clear()
        }
        // Honor auto-caps for swipe too: a sentence-initial glide capitalizes its
        // word just as typing would (the shift is armed by applyAutoCaps). Under
        // caps lock the whole glided word is upper-cased, not just its first
        // letter — a locked keyboard would have typed it that way.
        val kb = keyboard
        val out = when {
            kb?.capsLocked == true -> best.uppercase()
            kb?.shifted == true -> CaseMatch.matchLeading("A", best)
            else -> best
        }
        kb?.consumeShift()
        ic.commitText(out, 1)
        pending.clear(); pending.append(out) // treat as the current word: alts replace it
        atomicSpan = out.length // a wrong swipe clears whole on the next backspace
        lastAtomicWord = out
        val alts = ranked.take(3).ifEmpty { words.take(3) }
        // Match the strip to what a pick would commit (see updateSuggestions).
        keyboard?.suggestions = if (kb?.capsLocked == true) alts.map { it.uppercase() } else alts
        schedulePersist()
    }

    /**
     * Learn a committed word into the core (frequency + next-word context +
     * autocorrect protection) — gated by consent (BR-22) and field sensitivity
     * (E-2/BR-26), so password/secure fields are never learned.
     */
    private fun learnWord(word: String) {
        if (word.isEmpty() || field.isSensitive() || !learningEnabled) return
        // The core owns frequency + next-word (bigram) learning: pass the preceding
        // committed word so it records the transition. `preceding` must be read
        // BEFORE lastWord is advanced below.
        val preceding = lastWord ?: ""
        runCatching { bridge?.learnWord(preceding, word, field) }
        val w = word.lowercase()
        // Fold the committing word's recogniser languages into core momentum. The
        // device dictionary is only consulted when the field is not sensitive
        // (E-2/BR-26); this whole method is already gated on consent + sensitivity.
        val recognizers = (vocab.languagesOf(w) +
            (if (!field.isSensitive()) deviceDict.knownLanguages(w) else emptySet())).toList()
        runCatching { bridge?.observeLanguage(recognizers) }
        lastWord = w
    }

    /** True when learning may be recorded: consent is on and the field is not
     *  sensitive (E-2/BR-26). Gates the correction observations, mirroring
     *  [learnWord]; the core also gates internally, so this only avoids needless FFI. */
    private fun observeGate(): Boolean = learningEnabled && !field.isSensitive()

    /** Clear the autocorrect revert lookback (the service's per-intervening-edit
     *  hook, [CorrectionDetector.reset]). When the cleared slot means a correction
     *  survived to this edit without being reverted, feed the core a KEPT training
     *  signal (gated like all learning).
     *
     *  [boundWithheld] additionally bounds the withheld-reach counterfactual
     *  ([CorrectionDetector.expireWithheld]) for event kinds that never call
     *  [CorrectionDetector.onManualWord] — a swipe, a symbol/emoji, a whole-word
     *  delete-retype, or a newline — so a withheld note can't survive across them
     *  indefinitely and later fire a stale Reached for an unrelated later
     *  occurrence of the same word. Left false for a plain typed character/letter
     *  pick (mid-word, must not cut off a same-window delete-and-retype) and for
     *  the suggestion-pick/boundary commits that already self-check the note via
     *  `onManualWord` right after this call. */
    private fun clearCorrectionLookback(boundWithheld: Boolean = false) {
        val kept = corrections.reset()
        if (kept != null) emitOutcome(kept)
        if (boundWithheld) corrections.expireWithheld()
    }

    /** Feed the core's neural autocorrect gate the real-world [outcome] of the last
     *  gated correction, gated exactly like [learnWord] (consent + non-sensitive);
     *  the core also gates internally, so this only avoids a needless FFI call. */
    private fun emitOutcome(outcome: Outcome) {
        if (!observeGate()) return
        val ffi = when (outcome) {
            Outcome.REVERTED -> AutocorrectOutcome.REVERTED
            Outcome.KEPT -> AutocorrectOutcome.KEPT
            Outcome.REACHED -> AutocorrectOutcome.REACHED
        }
        runCatching { bridge?.observeAutocorrectOutcome(ffi, field) }
    }

    /**
     * [centers] re-centred by the core's learned per-key tap offsets (BR-7): the
     * gesture decoder should trace ideal paths through where this user actually
     * lands, not the nominal key centres. Falls back to the raw centres when the
     * bridge isn't ready or no offsets have been learned. Bridges the PointF/Pair
     * gap around the PointF-free [GestureGeometry].
     */
    private fun shiftedCenters(centers: Map<Char, PointF>): Map<Char, PointF> {
        val offsets = runCatching { bridge?.tapOffsets() }.getOrNull()
            ?.mapNotNull { o -> o.key.firstOrNull()?.let { it to (o.dx to o.dy) } }
            ?.toMap()
            ?: return centers
        if (offsets.isEmpty()) return centers
        val asPairs = centers.mapValues { it.value.x to it.value.y }
        return GestureGeometry.shiftCenters(asPairs, offsets)
            .mapValues { PointF(it.value.first, it.value.second) }
    }

    /** The space-bar language hint, e.g. "EN" or "EN PT" (primary first). */
    private fun spaceHint(tags: List<String>): String =
        tags.take(3).joinToString(" ") { it.uppercase() }

    override fun onFinishInput() {
        super.onFinishInput()
        pending.clear()
        atomicSpan = 0
        keyboard?.suggestions = emptyList()
        keyboard?.removeCallbacks(refreshStrip)
        schedulePersist(immediate = true)
    }

    private fun renderKeys(): List<RenderKey> =
        runCatching {
            bridge?.layoutKeys()?.map { RenderKey(it.label, it.x, it.y, it.width, it.height) } ?: emptyList()
        }.getOrDefault(emptyList())
            .also { keys ->
                keyCenters = keys.filter { it.label.length == 1 }
                    .associate { it.label.first().lowercaseChar() to PointF(it.x + it.width / 2f, it.y + it.height / 2f) }
            }

    /** A letter touch, already mapped to the Rust layout's logical space. */
    private fun handleTouch(x: Float, y: Float) {
        val ic = currentInputConnection ?: return
        atomicSpan = 0 // typing edits the word char-by-char, so backspace is char-wise
        ensureBridgeReady() // cold start: the first tap waits for the core to open
        val result = runCatching { bridge?.decode(x, y) }.getOrNull() ?: return
        val decoded = chooseKey(result) ?: return
        observeTap(decoded, x, y)
        val kb = keyboard
        val ch = if (kb?.shifted == true) decoded.uppercase() else decoded
        // A typed character is an intervening event: it clears the autocorrect
        // revert lookback so a later backspace isn't misread as undoing a correction
        // that happened words ago (CorrectionDetector's one-slot contract).
        clearCorrectionLookback()
        pending.append(ch)
        ic.commitText(ch, 1)
        kb?.consumeShift() // spends a one-shot shift; caps lock holds
        updateSuggestions()
    }

    /**
     * Pick which key a touch meant. Starts from the decoder's geometric best (the
     * per-user tap model already folded in), but when a word is in progress and
     * that best would kill the word — no dictionary word continues "prefix+best" —
     * it looks at the decoder's other near-confidence candidates and, if one keeps
     * a real word alive, prefers it. This is the lightweight fat-finger / low-vision
     * rescue: a tap landing between two keys resolves to the letter that spells a
     * word, not the fractionally-closer one that spells nothing. Purely additive —
     * with no pending word, an unambiguous tap, or no live alternative, it returns
     * the geometric best unchanged.
     */
    private fun chooseKey(result: com.featherkey.ffi.generated.FfiDecode): String? {
        val best = result.best ?: result.candidates.firstOrNull()?.key ?: return null
        return TapDisambiguator.choose(
            best = best,
            candidates = result.candidates.map { it.key to it.confidence },
            prefix = pending.toString(),
            ratio = AMBIGUOUS_TAP_RATIO,
            isLivePrefix = vocab::hasWordPrefix,
        )
    }

    /**
     * A long-press accent (or its base letter) chosen from the popup. Unlike a
     * normal tap this is an explicit pick, so it skips decode and tap-learning:
     * it is appended to the pending word exactly (upper-cased when shifted) and
     * committed, so it participates in the current word, autocorrect and learning
     * just like a decoded letter would.
     */
    private fun handleAccent(ch: String) {
        val ic = currentInputConnection ?: return
        atomicSpan = 0 // an explicit letter pick edits the word char-by-char
        val kb = keyboard
        val out = if (kb?.shifted == true) ch.uppercase() else ch
        // An explicit letter pick is an intervening event, like a decoded tap: it
        // clears the autocorrect revert lookback (CorrectionDetector's contract).
        clearCorrectionLookback()
        pending.append(out)
        ic.commitText(out, 1)
        kb?.consumeShift() // spends a one-shot shift; caps lock holds
        updateSuggestions()
    }

    /**
     * Teach the core's adaptive tap model where this user actually landed
     * relative to the key the decoder chose: the running-mean offset lets future
     * decodes correct for a systematic finger bias (BR-7). Same gates as
     * [learnWord] — consent (BR-22) and field sensitivity (E-2/BR-26) — so
     * password/secure fields never feed the model. The core update is O(1) and
     * allocation-free (BR-46), so it is safe on the input path.
     */
    private fun observeTap(decoded: String, x: Float, y: Float) {
        if (field.isSensitive() || !learningEnabled) return
        val ch = decoded.firstOrNull()?.lowercaseChar() ?: return
        val center = keyCenters[ch] ?: return
        runCatching { bridge?.observeTap(ch.toString(), x - center.x, y - center.y, field) }
    }

    private fun handleFunction(fk: FunctionKey) {
        val ic = currentInputConnection ?: return
        when (fk) {
            FunctionKey.SPACE -> boundary(ic)
            FunctionKey.BACKSPACE -> { backspace(ic); updateSuggestions(); applyAutoCaps() }
            FunctionKey.ENTER -> {
                flushWord(ic)
                // A multi-line field (Notes, message bodies) always wants a real
                // newline; a single-line field with a requested action (Search/
                // Send/Next/…) wants that action. Otherwise fall back to a newline
                // so Enter is never a dead key (the bug: sendDefaultEditorAction did
                // nothing for multi-line / no-action fields).
                // A real requested action (Search/Send/Go/Next/Done) fires that
                // action; a multi-line or no-action field (note bodies) gets a
                // literal newline — so Enter is never a dead key (the bug:
                // sendDefaultEditorAction did nothing for a no-action field).
                if (EnterKey.insertsNewline(editorInputType, editorImeOptions)) ic.commitText("\n", 1)
                else ic.performEditorAction(editorAction)
                keyboard?.suggestions = emptyList()
                applyAutoCaps()
            }
            FunctionKey.GLOBE -> openKeyboardPreferences()
            FunctionKey.MIC -> startVoiceInput()
        }
    }

    /** A number/symbol key: commit verbatim; it ends the current learnable word. */
    private fun handleChar(ch: String) {
        val ic = currentInputConnection ?: return
        ic.commitText(ch, 1)
        pending.clear()
        atomicSpan = 0
        // A symbol/number clears the revert lookback and, never checking the
        // withheld note itself, also bounds its reach.
        clearCorrectionLookback(boundWithheld = true)
        keyboard?.suggestions = emptyList()
    }

    /**
     * An emoji tapped on the emoji page: commit it verbatim and record it as
     * recent. Emoji never go through the decoder or the tap-learning path — they
     * are picked, not typed — so this just ends the current word's tracking like
     * any non-letter and drops the preceding-word context. Committing text is
     * always allowed; only *learning* is gated, and we learn nothing here.
     */
    private fun handleEmoji(emoji: String) {
        val ic = currentInputConnection ?: return
        ic.commitText(emoji, 1)
        pending.clear()
        atomicSpan = 0
        lastWord = null
        // An emoji clears the revert lookback and, never checking the withheld
        // note itself, also bounds its reach.
        clearCorrectionLookback(boundWithheld = true)
        keyboard?.suggestions = emptyList()
        keyboard?.recents = emojiRecents.record(emoji)
    }

    /**
     * Live predictions. While typing a word: words that usually follow the
     * previous one (context) and the user's own learned words first, then
     * frequency across languages. On an empty prefix just after a word: the
     * next-word predictions for the previous word (BR-10 next-word ranking).
     *
     * Ranking always runs on the lower-cased prefix (that is how the lexicons are
     * keyed), but under caps lock the strip is shown — and therefore committed —
     * upper-cased, so picking a suggestion mid-sentence can't silently drop the
     * user out of capitals.
     */
    private fun updateSuggestions() {
        val ranked = rankForStrip(pending.toString().lowercase())
        keyboard?.suggestions =
            if (keyboard?.capsLocked == true) ranked.map { it.uppercase() } else ranked
    }

    /**
     * The suggestion strip for [prefix]: on an empty prefix, the next-word
     * predictions for the previous word; otherwise the bundled per-language
     * completions plus (in a non-sensitive field) the device dictionary's
     * completions for scripts we ship no list for, all blended by the core ranker
     * so language momentum decides the order. The device lookup is async
     * ([DeviceDictionary.refresh]) and its callback re-runs this method when
     * results land. Skipped entirely in a sensitive field, so a password is never
     * sent to the system spell checker (E-2/BR-26). The core is the sole ranking
     * source now (W5): if it throws or returns empty the strip degrades to empty
     * rather than to a bundled per-language list, so [ensureBridgeReady] is called
     * first to close the cold-start window before the core has opened.
     */
    private fun rankForStrip(prefix: String): List<String> {
        ensureBridgeReady() // cold start: don't rank against a not-yet-open core (empty strip)
        val preceding = lastWord ?: ""
        // Device-dictionary completions (scripts we ship no list for) are gathered
        // in the shell — the only source the core can't see — and blended by the
        // core ranker. Skipped on an empty prefix (next-word prediction) and in a
        // sensitive field, where the word is never sent to the spell checker (E-2).
        val deviceCands = ArrayList<FfiRankCandidate>()
        if (prefix.isNotEmpty() && !field.isSensitive()) {
            deviceDict.refresh(prefix)
            for ((lang, words) in deviceDict.candidatesByLanguage())
                words.forEachIndexed { i, w ->
                    if (w.lowercase() != prefix) deviceCands.add(FfiRankCandidate(w, lang, FfiSource.DEVICE, i.toUInt()))
                }
        }
        // ONE core call handles both cases: non-empty prefix -> completions + device
        // + momentum + the dictionary fold-group variant guarantee; empty prefix ->
        // next-word predictions from the bigram context of `preceding`.
        val ranked = runCatching {
            bridge?.rankSuggestions(preceding, prefix, deviceCands)?.map { it.word } ?: emptyList()
        }.getOrDefault(emptyList())
        if (prefix.isEmpty()) return ranked // next-word predictions: no typed variant to guarantee
        // The core already guarantees the DICTIONARY fold-group variant; keep only the
        // DEVICE-derived variant as a thin post-step so a device-only accented/
        // apostrophe form still gets a slot over a commoner plain twin.
        return SuggestionStrip.withGuaranteedVariant(ranked, accentVariants(prefix), SUGGESTIONS)
    }

    /**
     * The DEVICE-dictionary accent/apostrophe variants of the typed [prefix] to
     * guarantee a strip slot — apostrophe/accent corrections the OS spell-checker
     * returned for the same base letters. The shipped-dictionary fold-group variant
     * is now guaranteed by the core, so it is no longer added here. Derived from the
     * OS dictionary, never a hand-authored table. Empty in a sensitive field, where
     * the word is never sent to the spell checker (E-2/BR-26).
     */
    private fun accentVariants(prefix: String): List<String> {
        if (field.isSensitive()) return emptyList()
        val f = Diacritics.fold(prefix)
        return deviceDict.candidatesByLanguage().values.asSequence().flatten()
            .filter { Diacritics.fold(it) == f && !it.equals(prefix, ignoreCase = true) }
            .distinct()
            .toList()
    }

    /** Commit a tapped suggestion, replacing the pending word. */
    private fun commitSuggestion(index: Int) {
        val ic = currentInputConnection ?: return
        val word = keyboard?.suggestions?.getOrNull(index) ?: return
        val cur = pending.toString()
        // Keep the case the word was heading toward: if the pending word was
        // capitalized (sentence start / auto-caps), the picked suggestion is too,
        // so tapping a completion for "Hel" gives "Hello", not "hello" — and an
        // all-caps prefix from caps lock ("HEL") completes to "HELLO".
        val out = CaseMatch.matchCase(cur, word)
        if (cur.isNotEmpty()) ic.deleteSurroundingText(cur.length, 0)
        ic.commitText("$out ", 1)
        // Picking a suggestion is an intervening edit: if it lands right after a
        // boundary autocorrect (the BR-10 next-word flow — nothing typed since),
        // that correction survived, so emit KEPT before consuming the lookback.
        clearCorrectionLookback()
        // A non-top pick is a signal the ranking was off for this prefix; feed it to
        // the core (gated exactly like learning) before advancing the context.
        val pick = corrections.onSuggestionPicked(cur.lowercase(), index, out)
        if (pick is CorrectionSignal.LowerRankedPick && observeGate())
            runCatching { bridge?.observeStripPick(pick.prefix, pick.picked, field) }
        // Picking the exact word a correction was withheld in favour of confirms the
        // gate should have applied it (the counterfactual REACHED signal).
        val reached = corrections.onManualWord(out.lowercase())
        if (reached != null) emitOutcome(reached)
        learnWord(out)
        pending.clear()
        atomicSpan = out.length + 1 // word + its space clears whole on next backspace
        lastAtomicWord = out
        updateSuggestions() // now show next-word predictions for this word
        applyAutoCaps()
        schedulePersist()
    }

    /** Word boundary: correct + accent the pending word, learn it (gated), then
     *  space — or, on a bare second space, end the sentence with ". ". */
    private fun boundary(ic: InputConnection) {
        val word = pending.toString()
        // A word boundary is an intervening event: clear the autocorrect revert
        // lookback up front so an uncorrected commit (or a double-space period) can't
        // leave a stale slot that a later backspace would misread as a revert. When
        // this very boundary IS an autocorrect, onAutocorrect below re-arms it.
        clearCorrectionLookback()
        if (word.isEmpty()) {
            // No word in progress: a space right after "<word> " becomes ". ".
            if (maybeDoubleSpacePeriod(ic)) return
        }
        if (word.isNotEmpty()) {
            // Restore accents/apostrophes on what was typed first (tambem → também,
            // im → I'm); only if that finds nothing do we fall back to the core's
            // edit-distance typo fix — otherwise a contraction like "ive" would be
            // "corrected" to a near lexicon word ("ice") before it could become "I've".
            val out = accentUpgrade(word) ?: correctedWord(word) ?: word
            if (out != word) {
                ic.deleteSurroundingText(word.length, 0)
                ic.commitText(out, 1)
                // Arm the one-slot revert lookback: an immediate backspace after this
                // means the user rejected the correction (keep their original word).
                corrections.onAutocorrect(word, out)
                // This word's OWN autocorrect never checks a withheld note (that only
                // happens on the uncorrected branch below) — so it can't be a match for
                // a note left over from an earlier word either; bound its reach here.
                corrections.expireWithheld()
            } else {
                // The user's typed word stood uncorrected: if it is the exact word a
                // correction was earlier WITHHELD in favour of (e.g. deleted a weak
                // "xat" and retyped "cat"), that reach confirms the gate was too shy.
                val reached = corrections.onManualWord(word.lowercase())
                if (reached != null) emitOutcome(reached)
            }
            learnWord(out)
        }
        ic.commitText(" ", 1)
        pending.clear()
        atomicSpan = 0 // after a manual space, backspace deletes char-by-char again
        updateSuggestions() // next-word predictions for the word just committed
        applyAutoCaps() // CAP_WORDS fields (names) re-capitalize after each space
        schedulePersist()
    }

    /**
     * Double-space → ". ": if the cursor sits just after "<letter-or-digit><space>",
     * replace that lone trailing space with a period and space, so a second tap of
     * the space bar ends the sentence (the universal Gboard/iOS convention). Reads
     * the actual text before the cursor rather than tracking state, so it fires only
     * when there really is a word-then-space to punctuate (never after "..", "  ",
     * or at the start of a field). Returns true when it fired — the caller must then
     * NOT also commit its own space.
     */
    private fun maybeDoubleSpacePeriod(ic: InputConnection): Boolean {
        if (!PunctuationRules.doubleSpaceMakesPeriod(ic.getTextBeforeCursor(2, 0))) return false
        ic.deleteSurroundingText(1, 0) // drop the lone trailing space
        ic.commitText(". ", 1)
        atomicSpan = 0
        lastWord = null // "." ends the sentence: no next-word context carries across it
        keyboard?.suggestions = emptyList()
        applyAutoCaps() // capitalize the first letter of the new sentence
        return true
    }

    /**
     * The canonical accented spelling to auto-apply for a fully-typed word at a
     * boundary (the user opted into auto-accent on space), or null to leave it as
     * typed. Only lowercase words of length ≥ 3 are upgraded: this restores the
     * common cases (tambem → também, voce → você) while leaving very short,
     * meaning-ambiguous tokens (e/é, a/à, da/dá) exactly as the user typed them.
     */
    private fun accentUpgrade(word: String): String? {
        val lower = word.lowercase()
        val allLower = word == lower
        // Title-case (only the first letter upper) is what auto-caps produces at a
        // sentence start — still eligible, re-capitalized after lookup so "Tambem"
        // → "Também". ALLCAPS or interior caps are left exactly as deliberately typed.
        val titleCase = !allLower && word[0].isUpperCase() && word.substring(1) == word.substring(1).lowercase()
        if (!allLower && !titleCase) return null
        val canon = vocab.accentedCanonical(lower) ?: return null
        // Skip very short words (e→é, a→à traps) — UNLESS the canonical is a
        // contraction (im→I'm, ive→I've), which is unambiguous even at length 2.
        val isContraction = canon.any { it == '\'' || it == '’' }
        if (word.length < 3 && !isContraction) return null
        return CaseMatch.matchLeading(word, canon)
    }

    /**
     * The word to commit at a boundary, or `null` to keep what was typed.
     * Delegates to the core's `chooseCorrection` — an all-active-language,
     * momentum-aware edit-distance fix — passing along what the device
     * dictionary knows so a word recognised there is treated as known too.
     * It never clobbers a word already known in any active language, in the
     * device dictionary, or a mixed-case token (guarded above); otherwise it
     * picks the momentum-weighted correction the core judges most likely.
     */
    private fun correctedWord(word: String): String? {
        if (word != word.lowercase()) return null // don't mangle Caps/ALLCAPS
        // The device dictionary is the base for languages we bundle no list for.
        // Skipped in a sensitive field (the word would go to the spell checker).
        val deviceOn = !field.isSensitive()
        val deviceKnown = if (deviceOn) {
            if (deviceDict.knownLanguages(word).isNotEmpty()) listOf(word) else emptyList()
        } else emptyList()
        val deviceCands = ArrayList<FfiRankCandidate>()
        if (deviceOn) for ((lang, words) in deviceDict.candidatesByLanguage())
            words.forEachIndexed { i, w -> deviceCands.add(FfiRankCandidate(w, lang, FfiSource.DEVICE, i.toUInt())) }
        val c = runCatching { bridge?.chooseCorrection(word, deviceKnown, deviceCands) }.getOrNull() ?: return null
        // The gate withheld an otherwise-available correction: remember the word it
        // would have applied, so a later manual landing on it is a REACHED signal
        // (the counterfactual that tells the gate it was too cautious).
        c.withheld?.let { corrections.noteWithheld(it) }
        return if (c.applied && c.primary != word) c.primary else null
    }

    private fun backspace(ic: InputConnection) {
        // A word committed as one unit (swipe or picked suggestion) clears whole on
        // the first backspace after it; the arming is one-shot.
        if (atomicSpan > 0) {
            val rejected = lastAtomicWord
            ic.deleteSurroundingText(atomicSpan, 0)
            atomicSpan = 0
            pending.clear()
            lastWord = null // the deleted word is gone: no next-word context
            lastAtomicWord = null
            // Deleting a just-committed word whole is an intervening edit: surface a
            // KEPT for any autocorrect it displaced before recording the delete. This
            // delete-retype never checks the withheld note via onManualWord itself, so
            // also bound its reach here.
            clearCorrectionLookback(boundWithheld = true)
            // Deleting a just-committed word whole is a low-weight negative signal.
            if (rejected != null && observeGate()) {
                val sig = corrections.onDeleteRetype(rejected)
                runCatching { bridge?.observeDeleteRetype(sig.oldWord, field) }
            }
            return
        }
        // A backspace immediately after an autocorrect reverts it: the user wants
        // their original word kept, so protect it from being re-corrected.
        val undo = corrections.onBackspaceUndo()
        if (undo is CorrectionSignal.RevertAfterAutocorrect && observeGate()) {
            runCatching { bridge?.addToDictionary(undo.word) }
            // The user rejected the correction: push the gate toward withholding it.
            emitOutcome(Outcome.REVERTED)
        }
        if (pending.isNotEmpty()) {
            // Mid-word: each pending char is a single BMP letter, so char-wise.
            pending.deleteCharAt(pending.length - 1)
            ic.deleteSurroundingText(1, 0)
            return
        }
        // Nothing composing: delete a whole grapheme cluster so an emoji (surrogate
        // pair, ZWJ/skin-tone/flag sequence) or a base+combining pair clears on one
        // press instead of leaving an orphaned half-character.
        val before = ic.getTextBeforeCursor(GRAPHEME_LOOKBEHIND, 0)
        val n = GraphemeDeletion.lastClusterLength(before)
        ic.deleteSurroundingText(if (n > 0) n else 1, 0)
    }

    private fun flushWord(ic: InputConnection) {
        learnWord(pending.toString())
        pending.clear()
        atomicSpan = 0
        lastWord = null // Enter ends the line: start the next with no context
        // A newline is an intervening event: drop the revert lookback and, never
        // checking the withheld note itself, also bound its reach.
        clearCorrectionLookback(boundWithheld = true)
    }

    /**
     * Globe: open FeatherKey's own preferences — the keyboard's settings screen,
     * where languages (several at once), on-device learning, and learned data are
     * managed. Launched as a new task (the standard way an IME hands off to an
     * Activity, mirroring [startVoiceInput]'s permission escape hatch). Addressed
     * by class name so this module needs no compile dependency on settings-ui.
     */
    private fun openKeyboardPreferences() {
        val intent = Intent()
            .setClassName(packageName, "com.featherkey.settings.SettingsActivity")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        runCatching { startActivity(intent) }
    }

    /** Mic: system voice typing via [SpeechRecognizer], committed into the field. */
    private fun startVoiceInput() {
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            Toast.makeText(this, "Enable microphone access for FeatherKey to use voice typing", Toast.LENGTH_LONG).show()
            val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
                .setData(Uri.fromParts("package", packageName, null))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            runCatching { startActivity(intent) }
            return
        }
        if (!SpeechRecognizer.isRecognitionAvailable(this)) {
            Toast.makeText(this, "Voice typing isn't available on this device", Toast.LENGTH_SHORT).show()
            return
        }
        recognizer?.destroy()
        recognizer = SpeechRecognizer.createSpeechRecognizer(this).also { sr ->
            sr.setRecognitionListener(voiceListener())
            val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
                .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, false)
            runCatching { sr.startListening(intent) }
        }
    }

    private fun voiceListener() = object : RecognitionListener {
        override fun onResults(results: Bundle?) {
            val text = results
                ?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                ?.firstOrNull()
                ?.takeIf { it.isNotBlank() }
            if (text != null) currentInputConnection?.commitText("$text ", 1)
            recognizer?.destroy(); recognizer = null
        }

        override fun onError(error: Int) { recognizer?.destroy(); recognizer = null }
        override fun onReadyForSpeech(params: Bundle?) {}
        override fun onBeginningOfSpeech() {}
        override fun onRmsChanged(rmsdB: Float) {}
        override fun onBufferReceived(buffer: ByteArray?) {}
        override fun onEndOfSpeech() {}
        override fun onPartialResults(partialResults: Bundle?) {}
        override fun onEvent(eventType: Int, params: Bundle?) {}
    }

    private fun schedulePersist(immediate: Boolean = false) {
        persistJob?.cancel()
        persistJob = ioScope.launch {
            if (!immediate) delay(PERSIST_DEBOUNCE_MS)
            runCatching { bridge?.persist() }
        }
    }

    override fun onDestroy() {
        recognizer?.destroy()
        deviceDict.close()
        ioScope.cancel()
        runCatching { bridge?.persist() }
        runCatching { bridge?.close() }
        super.onDestroy()
    }

    private companion object {
        const val PERSIST_DEBOUNCE_MS = 3_000L
        const val SUGGESTIONS = 3 // strip capacity
        // A rival key must carry at least this share of the best key's confidence
        // to override it on word-continuation grounds — so only a genuinely close
        // (between-keys) tap is rescued, never a far, clearly-different key.
        private const val AMBIGUOUS_TAP_RATIO = 0.5f
        private const val DEVICE_REFRESH_COALESCE_MS = 16L // ~one frame; batch same-keystroke device results
        // Chars to fetch before the cursor to find the last grapheme cluster on
        // backspace — comfortably longer than the widest emoji cluster (a tag-flag
        // like the England flag is 14 UTF-16 units; a 4-person ZWJ family is 11).
        private const val GRAPHEME_LOOKBEHIND = 32
    }
}

/**
 * Loads the active languages' lexicons from `assets/lexicons/<tag>.txt` (one word
 * per line) for the core's correction/autocorrect. The words are passed in asset
 * order and NOT re-sorted here: the core records that input position as each word's
 * bundled rank (frequency-carry, option A) before it byte-sorts them internally, so
 * a commoner word outranks a rarer one across languages. Suggestion and swipe
 * ranking also use the frequency-ordered lists via [Vocabulary].
 *
 * That makes the assets' LINE ORDER a load-bearing signal, so it is generated and
 * gated rather than trusted: `core/tools/order_lexicons.py` orders each lexicon by
 * its word's position in `assets/freq/<tag>.txt`, and `--check` fails CI if one
 * drifts. It had drifted — the files shipped alphabetically for several waves, so
 * every consumer read alphabetical position as if it were commonness.
 */
object Lexicons {
    fun load(context: Context, tags: List<String>): List<Language> =
        tags.map { tag ->
            val words = runCatching {
                context.assets.open("lexicons/$tag.txt").bufferedReader().useLines { lines ->
                    lines.map { it.trim() }.filter { it.isNotEmpty() }.toList()
                }
            }.getOrDefault(emptyList())
            Language(tag, words)
        }
}
