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
import com.featherkey.keyboard.FunctionKey
import com.featherkey.keyboard.KeyboardView
import com.featherkey.keyboard.RenderKey
import com.featherkey.onboarding.ConsentStore
import com.featherkey.platform.EditorInfoSensitivity
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
    /** The active languages currently loaded into the core (order = preference). */
    private var currentTags: List<String> = emptyList()
    /** Frequency-ranked vocabulary for suggestions + swipe (loaded off the input path). */
    @Volatile private var vocab: Vocabulary = Vocabulary.empty()
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
        currentTags = langPrefs.activeTags()
        usage = UsageModel(this).also { it.load() }
        bigrams = ContextModel(this).also { it.load() }
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
            loadVocab(tags)
        }
        keyboard?.spaceHint = spaceHint(tags)
    }

    /** Build the frequency vocabulary off the input thread; swap it in when ready. */
    private fun loadVocab(tags: List<String>) {
        ioScope.launch { vocab = Vocabulary.load(applicationContext, tags) }
    }

    /** A swipe over the letters: decode to a word, commit it, offer alternatives. */
    private fun handleGesture(pathPts: List<PointF>, centers: Map<Char, PointF>) {
        val ic = currentInputConnection ?: return
        val words = GestureDecoder.decode(pathPts, centers, vocab.words, vocab::rankOf, usage.map, limit = 4)
        val best = words.firstOrNull() ?: return
        if (pending.isNotEmpty()) { // finalise a half-typed word with a space
            learnWord(pending.toString())
            ic.commitText(" ", 1)
            pending.clear()
        }
        ic.commitText(best, 1)
        // Swipe result has no per-tap data; drop any so suggestions use prefixes.
        tapDists.clear()
        pending.clear(); pending.append(best) // treat as the current word: alts replace it
        keyboard?.suggestions = words.take(3)
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
            FunctionKey.GLOBE -> showLanguageDialog()
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
     * Live predictions. While typing a word: words that usually follow the
     * previous one (context) and the user's own learned words first, then
     * frequency across languages. On an empty prefix just after a word: the
     * next-word predictions for the previous word (BR-10 next-word ranking).
     */
    private fun updateSuggestions() {
        val prefix = pending.toString().lowercase()
        keyboard?.suggestions = when {
            prefix.isEmpty() -> lastWord?.let { bigrams.nextWords(it, 3) } ?: emptyList()
            // Taps in lockstep with the word: read them probabilistically against
            // the language model (fat-finger tolerant) instead of as hard keys.
            tapDists.size == prefix.length ->
                vocab.probableWords(tapDists, usage.map, bigrams.nextCounts(lastWord), 3)
            // Word came from a path without per-tap data (swipe, suggestion): fall
            // back to exact-prefix completion.
            else -> vocab.suggestions(prefix, usage.map, 3, bigrams.nextCounts(lastWord))
        }
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
     * The word to commit at a boundary, or `null` to keep what was typed. Uses
     * the core autocorrect first; if that declines and the typed string is not
     * itself a known word, it trusts the noisy-channel reading of the taps — so a
     * fat-fingered "rhe" commits as "the" — but never second-guesses a real word
     * or a mixed-case token.
     */
    private fun correctedWord(word: String): String? {
        val core = runCatching { bridge.correct(word, "", word) }.getOrNull()
        if (core != null && core.applied) return core.primary
        if (word != word.lowercase()) return null // don't mangle Caps/ALLCAPS
        if (vocab.rankOf(word) != Int.MAX_VALUE) return null // already a real word
        if (tapDists.size != word.length) return null // no per-tap data to trust
        return vocab.probableWords(tapDists, usage.map, bigrams.nextCounts(lastWord), 1)
            .firstOrNull()
            ?.takeIf { it != word }
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
     * Globe: choose the active languages right from the keyboard. Several may be
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
        usage.persist()
        bigrams.persist()
        ioScope.cancel()
        runCatching { bridge.persist() }
        runCatching { bridge.close() }
        super.onDestroy()
    }

    private companion object {
        const val PERSIST_DEBOUNCE_MS = 3_000L
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
