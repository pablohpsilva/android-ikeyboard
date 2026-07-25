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
import android.app.AlertDialog
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
import android.view.WindowManager
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.widget.Toast
import com.featherkey.ffi.FeatherKeyBridge
import com.featherkey.ffi.FieldSensitivity
import com.featherkey.ffi.Language
import com.featherkey.ffi.generated.FfiDecode
import com.featherkey.ffi.generated.FfiRankCandidate
import com.featherkey.ffi.generated.FfiSource
import com.featherkey.keyboard.FunctionKey
import com.featherkey.keyboard.KeyboardView
import com.featherkey.keyboard.RenderKey
import com.featherkey.onboarding.ConsentStore
import com.featherkey.platform.DeviceDictionary
import com.featherkey.platform.EditorInfoSensitivity
import com.featherkey.platform.EmojiRecents
import com.featherkey.platform.KeystoreKeyProvider
import com.featherkey.platform.LanguageCatalog
import com.featherkey.platform.LanguagePrefs
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

class FeatherKeyImeService : InputMethodService() {

    private lateinit var bridge: FeatherKeyBridge
    private val ioScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private var field: FieldSensitivity = FieldSensitivity { false }
    private val pending = StringBuilder()
    /** Per-tap key-probability distributions for the word in [pending], kept in
     *  lockstep with it, so the word can be re-read probabilistically (BR-10). */
    private val tapDists = ArrayList<Map<Char, Float>>()
    private var persistJob: Job? = null

    private var keyboard: KeyboardView? = null
    private var recognizer: SpeechRecognizer? = null

    private lateinit var langPrefs: LanguagePrefs
    /** Persisted most-recently-used emoji for the emoji page's recents tab. */
    private lateinit var emojiRecents: EmojiRecents
    /** The active languages currently loaded into the core (order = preference). */
    private var currentTags: List<String> = emptyList()
    /** Frequency-ranked vocabulary for suggestions + swipe (loaded off the input path). */
    @Volatile private var vocab: Vocabulary = Vocabulary.empty()
    /**
     * The device's own dictionary (Android TextServices) for the primary
     * language — the base vocabulary for scripts we bundle no word list for
     * (Russian, Greek, …). Its lookups are async; the callback refreshes the
     * strip. Never queried in a sensitive field (E-2/BR-26).
     */
    private lateinit var deviceDict: DeviceDictionary
    /** On-device usage learning that biases ranking toward the user's own words. */
    private lateinit var usage: UsageModel
    /** On-device next-word (bigram) learning for context-aware prediction. */
    private lateinit var bigrams: ContextModel
    /** The last committed word, lowercased — the context for the next prediction. */
    @Volatile private var lastWord: String? = null
    /** Logical-space centre of each letter key, for computing tap offsets to learn. */
    @Volatile private var keyCenters: Map<Char, PointF> = emptyMap()
    /** The learning-consent toggle (BR-22); learning is off until opted in. */
    @Volatile private var learningEnabled = false

    override fun onCreate() {
        super.onCreate()
        langPrefs = LanguagePrefs(this)
        emojiRecents = EmojiRecents(this)
        currentTags = langPrefs.activeTags()
        usage = UsageModel(this).also { it.load() }
        bigrams = ContextModel(this).also { it.load() }
        // The device dictionary's async lookups refresh the strip on completion.
        deviceDict = DeviceDictionary(this) { keyboard?.post { updateSuggestions() } }
        deviceDict.setLanguages(currentTags)
        loadVocab(currentTags)
        ConsentStore(applicationContext).learningEnabled
            .onEach { learningEnabled = it }
            .launchIn(ioScope)
        val key = KeystoreKeyProvider(this).provisionDataKey()
        val dbPath = File(filesDir, "featherkey.redb").absolutePath
        bridge = FeatherKeyBridge.open(dbPath, key, Lexicons.load(this, currentTags))
        key.fill(0) // wipe the shell's copy; the native side holds it zeroizing
    }

    override fun onCreateInputView(): View {
        val view = KeyboardView(this)
        view.keys = renderKeys()
        view.spaceHint = spaceHint(currentTags)
        view.onKeyTouch = { x, y -> handleTouch(x, y) }
        view.onCharKey = { ch -> handleChar(ch) }
        view.onFunctionKey = { fk -> handleFunction(fk) }
        view.onSuggestion = { i -> commitSuggestion(i) }
        view.onGesture = { pathPts, centers -> handleGesture(pathPts, centers) }
        view.onEmoji = { emoji -> handleEmoji(emoji) }
        view.recents = emojiRecents.list()
        keyboard = view
        return view
    }

    override fun onStartInput(info: EditorInfo?, restarting: Boolean) {
        super.onStartInput(info, restarting)
        val sensitive = EditorInfoSensitivity.isSensitive(info)
        field = FieldSensitivity { sensitive } // captured once per field (E-2)
        pending.clear(); tapDists.clear()
        lastWord = null // a new field starts with no preceding-word context
        keyboard?.suggestions = emptyList()
        keyboard?.resetPage()
        // Pick up any language changes made in settings since the last field.
        applyLanguages(langPrefs.activeTags())
    }

    /** Push [tags] to the core (if changed) and reflect them on the space bar. */
    private fun applyLanguages(tags: List<String>) {
        if (tags != currentTags) {
            runCatching { bridge.setActiveLanguages(Lexicons.load(this, tags)) }
            currentTags = tags
            deviceDict.setLanguages(tags)
            loadVocab(tags)
            // The primary language may have changed the core's alpha script
            // (e.g. Latin → Cyrillic), so re-pull the rendered keys; renderKeys()
            // also refreshes keyCenters for the tap model via its `.also`.
            keyboard?.keys = renderKeys()
        }
        keyboard?.spaceHint = spaceHint(tags)
    }

    /** Build the frequency vocabulary off the input thread; swap it in when ready. */
    private fun loadVocab(tags: List<String>) {
        ioScope.launch {
            vocab = Vocabulary.load(applicationContext, tags)
            // On a cold start the strip may be empty for the word already being
            // typed (the load is async); refresh it now that the data is ready.
            keyboard?.post { updateSuggestions() }
        }
    }

    /** A swipe over the letters: decode to a word, commit it, offer alternatives. */
    private fun handleGesture(pathPts: List<PointF>, centers: Map<Char, PointF>) {
        val ic = currentInputConnection ?: return
        val words = GestureDecoder.decode(pathPts, centers, vocab.words, vocab::rankOf, usage.map, limit = 4)
        if (words.isEmpty()) return
        // Tag each decoded word by the languages that recognise it (fallback: the
        // primary language) and let the core ranker blend in language momentum.
        val fallback = currentTags.firstOrNull() ?: "en"
        val cands = ArrayList<FfiRankCandidate>()
        words.forEachIndexed { i, w ->
            val langs = vocab.languagesOf(w).ifEmpty { setOf(fallback) }
            for (lang in langs) cands.add(FfiRankCandidate(w, lang, FfiSource.LEXICON, i.toUInt()))
        }
        val ranked = runCatching { bridge.rank(cands, SUGGESTIONS.toUInt()).map { it.word } }
            .getOrDefault(emptyList())
        val best = ranked.firstOrNull() ?: words.firstOrNull() ?: return
        if (pending.isNotEmpty()) { // finalise a half-typed word with a space
            learnWord(pending.toString())
            ic.commitText(" ", 1)
            pending.clear()
        }
        ic.commitText(best, 1)
        // Swipe result has no per-tap data; drop any so suggestions use prefixes.
        tapDists.clear()
        pending.clear(); pending.append(best) // treat as the current word: alts replace it
        keyboard?.suggestions = ranked.take(3).ifEmpty { words.take(3) }
        schedulePersist()
    }

    /**
     * Learn a committed word into both the core (autocorrect protection) and the
     * shell usage model (ranking) — gated by consent (BR-22) and field
     * sensitivity (E-2/BR-26), so password/secure fields are never learned.
     */
    private fun learnWord(word: String) {
        if (word.isEmpty() || field.isSensitive() || !learningEnabled) return
        runCatching { bridge.learnWord(word, field) }
        usage.record(word)
        // Record the transition from the previous word, then advance the context.
        val w = word.lowercase()
        // Fold the committing word's recogniser languages into core momentum. The
        // device dictionary is only consulted when the field is not sensitive
        // (E-2/BR-26); this whole method is already gated on consent + sensitivity.
        val recognizers = (vocab.languagesOf(w) +
            (if (!field.isSensitive()) deviceDict.knownLanguages(w) else emptySet())).toList()
        runCatching { bridge.observeLanguage(recognizers) }
        lastWord?.let { bigrams.record(it, w) }
        lastWord = w
    }

    /** The space-bar language hint, e.g. "EN" or "EN PT" (primary first). */
    private fun spaceHint(tags: List<String>): String =
        tags.take(3).joinToString(" ") { it.uppercase() }

    override fun onFinishInput() {
        super.onFinishInput()
        pending.clear()
        keyboard?.suggestions = emptyList()
        schedulePersist(immediate = true)
    }

    private fun renderKeys(): List<RenderKey> =
        runCatching {
            bridge.layoutKeys().map { RenderKey(it.label, it.x, it.y, it.width, it.height) }
        }.getOrDefault(emptyList())
            .also { keys ->
                keyCenters = keys.filter { it.label.length == 1 }
                    .associate { it.label.first().lowercaseChar() to PointF(it.x + it.width / 2f, it.y + it.height / 2f) }
            }

    /** A letter touch, already mapped to the Rust layout's logical space. */
    private fun handleTouch(x: Float, y: Float) {
        val ic = currentInputConnection ?: return
        val result = runCatching { bridge.decode(x, y) }.getOrNull() ?: return
        val decoded = result.best ?: return
        observeTap(decoded, x, y)
        tapDists.add(distributionOf(result)) // remember the tap as a distribution
        val kb = keyboard
        val ch = if (kb?.shifted == true) decoded.uppercase() else decoded
        pending.append(ch)
        ic.commitText(ch, 1)
        if (kb?.shifted == true) kb.shifted = false
        updateSuggestions()
    }

    /** A tap's key -> probability map (lowercased), from the decoder's candidates. */
    private fun distributionOf(result: FfiDecode): Map<Char, Float> {
        val m = HashMap<Char, Float>(result.candidates.size)
        for (c in result.candidates) {
            val ch = c.key.firstOrNull()?.lowercaseChar() ?: continue
            val cur = m[ch]
            if (cur == null || c.confidence > cur) m[ch] = c.confidence
        }
        return m
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
        runCatching { bridge.observeTap(ch.toString(), x - center.x, y - center.y, field) }
    }

    private fun handleFunction(fk: FunctionKey) {
        val ic = currentInputConnection ?: return
        when (fk) {
            FunctionKey.SPACE -> boundary(ic)
            FunctionKey.BACKSPACE -> { backspace(ic); updateSuggestions() }
            FunctionKey.ENTER -> {
                flushWord(ic)
                sendDefaultEditorAction(true)
                keyboard?.suggestions = emptyList()
            }
            FunctionKey.GLOBE -> cycleLanguage()
            FunctionKey.MIC -> startVoiceInput()
        }
    }

    /** A number/symbol key: commit verbatim; it ends the current learnable word. */
    private fun handleChar(ch: String) {
        val ic = currentInputConnection ?: return
        ic.commitText(ch, 1)
        pending.clear(); tapDists.clear()
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
        pending.clear(); tapDists.clear()
        lastWord = null
        keyboard?.suggestions = emptyList()
        keyboard?.recents = emojiRecents.record(emoji)
    }

    /**
     * Live predictions. While typing a word: words that usually follow the
     * previous one (context) and the user's own learned words first, then
     * frequency across languages. On an empty prefix just after a word: the
     * next-word predictions for the previous word (BR-10 next-word ranking).
     */
    private fun updateSuggestions() {
        keyboard?.suggestions = rankForStrip(pending.toString().lowercase())
    }

    /**
     * The suggestion strip for [prefix]: on an empty prefix, the next-word
     * predictions for the previous word; otherwise the bundled per-language
     * completions plus (in a non-sensitive field) the device dictionary's
     * completions for scripts we ship no list for, all blended by the core ranker
     * so language momentum decides the order. The device lookup is async
     * ([DeviceDictionary.refresh]) and its callback re-runs this method when
     * results land. Skipped entirely in a sensitive field, so a password is never
     * sent to the system spell checker (E-2/BR-26). If the core ranker throws or
     * comes back empty, falls back to the bundled per-language candidates in the
     * order gathered, so the strip still shows completions (same
     * degrade-don't-crash pattern as [handleGesture]).
     */
    private fun rankForStrip(prefix: String): List<String> {
        if (prefix.isEmpty()) return lastWord?.let { bigrams.nextWords(it, SUGGESTIONS) } ?: emptyList()
        val bundled = vocab.candidatesByLanguage(prefix, usage.map, bigrams.nextCounts(lastWord), SUGGESTIONS + 2)
        val cands = ArrayList<FfiRankCandidate>()
        for (c in bundled) cands.add(FfiRankCandidate(c.word, c.lang, FfiSource.LEXICON, c.sourceRank.toUInt()))
        if (!field.isSensitive()) {
            deviceDict.refresh(prefix)
            for ((lang, words) in deviceDict.candidatesByLanguage())
                words.forEachIndexed { i, w ->
                    if (w.lowercase() != prefix) cands.add(FfiRankCandidate(w, lang, FfiSource.DEVICE, i.toUInt()))
                }
        }
        val ranked = runCatching { bridge.rank(cands, SUGGESTIONS.toUInt()).map { it.word } }.getOrDefault(emptyList())
        return ranked.ifEmpty { bundled.map { it.word }.distinct().take(SUGGESTIONS) }
    }

    /** Commit a tapped suggestion, replacing the pending word. */
    private fun commitSuggestion(index: Int) {
        val ic = currentInputConnection ?: return
        val word = keyboard?.suggestions?.getOrNull(index) ?: return
        val cur = pending.toString()
        if (cur.isNotEmpty()) ic.deleteSurroundingText(cur.length, 0)
        ic.commitText("$word ", 1)
        learnWord(word)
        pending.clear(); tapDists.clear()
        updateSuggestions() // now show next-word predictions for this word
        schedulePersist()
    }

    /** Word boundary: correct the pending word, learn it (gated), then space. */
    private fun boundary(ic: InputConnection) {
        val word = pending.toString()
        if (word.isNotEmpty()) {
            val corrected = correctedWord(word)
            if (corrected != null && corrected != word) {
                ic.deleteSurroundingText(word.length, 0)
                ic.commitText(corrected, 1)
            }
            learnWord(corrected ?: word)
        }
        ic.commitText(" ", 1)
        pending.clear(); tapDists.clear()
        updateSuggestions() // next-word predictions for the word just committed
        schedulePersist()
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
        val c = runCatching { bridge.chooseCorrection(word, deviceKnown, deviceCands) }.getOrNull() ?: return null
        return if (c.applied && c.primary != word) c.primary else null
    }

    private fun backspace(ic: InputConnection) {
        if (pending.isNotEmpty()) pending.deleteCharAt(pending.length - 1)
        if (tapDists.isNotEmpty()) tapDists.removeAt(tapDists.size - 1)
        ic.deleteSurroundingText(1, 0)
    }

    private fun flushWord(ic: InputConnection) {
        learnWord(pending.toString())
        pending.clear(); tapDists.clear()
        lastWord = null // Enter ends the line: start the next with no context
    }

    /**
     * Globe: cycle the primary language among the active set by rotating the
     * first tag to the end (which swaps the alpha script/layout and the space-bar
     * hint in place). With fewer than two languages active there is nothing to
     * cycle, so fall back to the picker so single-language users can add more.
     */
    private fun cycleLanguage() {
        val active = langPrefs.activeTags()
        if (active.size < 2) { showLanguageDialog(); return }
        val rotated = active.drop(1) + active.first()
        langPrefs.setActiveTags(rotated)
        applyLanguages(rotated)
    }

    /**
     * Globe long-form: choose the active languages right from the keyboard. Several may be
     * active at once (checked); the core keeps them all active and the space bar
     * shows the set. Rendered as a dialog attached to the input view's window
     * (the standard way an IME shows a dialog).
     */
    private fun showLanguageDialog() {
        val token = keyboard?.windowToken ?: return
        val langs = LanguageCatalog.all(this)
        val active = langPrefs.activeTags().toMutableSet()
        val names = langs.map { it.displayName }.toTypedArray()
        val checked = BooleanArray(langs.size) { active.contains(langs[it].tag) }
        val dialog = AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
            .setTitle("Languages")
            .setMultiChoiceItems(names, checked) { _, which, isChecked ->
                if (isChecked) active.add(langs[which].tag) else active.remove(langs[which].tag)
            }
            .setPositiveButton("OK") { _, _ ->
                val ordered = langs.map { it.tag }.filter { active.contains(it) }
                if (ordered.isNotEmpty()) {
                    langPrefs.setActiveTags(ordered)
                    applyLanguages(ordered)
                }
            }
            .setNegativeButton("Cancel", null)
            .create()
        dialog.window?.apply {
            attributes = attributes.also {
                it.token = token
                it.type = WindowManager.LayoutParams.TYPE_APPLICATION_ATTACHED_DIALOG
            }
            addFlags(WindowManager.LayoutParams.FLAG_ALT_FOCUSABLE_IM)
        }
        dialog.show()
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
            runCatching { bridge.persist() }
            usage.persist()
            bigrams.persist()
        }
    }

    override fun onDestroy() {
        recognizer?.destroy()
        deviceDict.close()
        usage.persist()
        bigrams.persist()
        ioScope.cancel()
        runCatching { bridge.persist() }
        runCatching { bridge.close() }
        super.onDestroy()
    }

    private companion object {
        const val PERSIST_DEBOUNCE_MS = 3_000L
        const val SUGGESTIONS = 3 // strip capacity
    }
}

/**
 * Loads the active languages' lexicons from `assets/lexicons/<tag>.txt` (one word
 * per line, pre-sorted by byte order — the core rejects an unsorted list) for the
 * core's correction/autocorrect. Suggestion and swipe ranking use the
 * frequency-ordered lists via [Vocabulary].
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
