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
import android.content.Intent
import android.content.pm.PackageManager
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
import android.view.inputmethod.InputMethodManager
import android.widget.Toast
import com.featherkey.ffi.FeatherKeyBridge
import com.featherkey.ffi.FieldSensitivity
import com.featherkey.ffi.Language
import com.featherkey.keyboard.FunctionKey
import com.featherkey.keyboard.KeyboardView
import com.featherkey.keyboard.RenderKey
import com.featherkey.platform.EditorInfoSensitivity
import com.featherkey.platform.KeystoreKeyProvider
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class FeatherKeyImeService : InputMethodService() {

    private lateinit var bridge: FeatherKeyBridge
    private val ioScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private var field: FieldSensitivity = FieldSensitivity { false }
    private val pending = StringBuilder()
    private var persistJob: Job? = null

    private var keyboard: KeyboardView? = null
    private var recognizer: SpeechRecognizer? = null

    override fun onCreate() {
        super.onCreate()
        val key = KeystoreKeyProvider(this).provisionDataKey()
        val dbPath = File(filesDir, "featherkey.redb").absolutePath
        bridge = FeatherKeyBridge.open(dbPath, key, Lexicons.bundled(this))
        key.fill(0) // wipe the shell's copy; the native side holds it zeroizing
    }

    override fun onCreateInputView(): View {
        val view = KeyboardView(this)
        view.keys = renderKeys()
        view.onKeyTouch = { x, y -> handleTouch(x, y) }
        view.onCharKey = { ch -> handleChar(ch) }
        view.onFunctionKey = { fk -> handleFunction(fk) }
        view.onSuggestion = { i -> commitSuggestion(i) }
        keyboard = view
        return view
    }

    override fun onStartInput(info: EditorInfo?, restarting: Boolean) {
        super.onStartInput(info, restarting)
        val sensitive = EditorInfoSensitivity.isSensitive(info)
        field = FieldSensitivity { sensitive } // captured once per field (E-2)
        pending.clear()
        keyboard?.suggestions = emptyList()
        keyboard?.resetPage()
    }

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

    /** A letter touch, already mapped to the Rust layout's logical space. */
    private fun handleTouch(x: Float, y: Float) {
        val ic = currentInputConnection ?: return
        val decoded = runCatching { bridge.decode(x, y).best }.getOrNull() ?: return
        val kb = keyboard
        val ch = if (kb?.shifted == true) decoded.uppercase() else decoded
        pending.append(ch)
        ic.commitText(ch, 1)
        if (kb?.shifted == true) kb.shifted = false
        updateSuggestions()
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
            FunctionKey.GLOBE -> showImePicker()
            FunctionKey.MIC -> startVoiceInput()
        }
    }

    /** A number/symbol key: commit verbatim; it ends the current learnable word. */
    private fun handleChar(ch: String) {
        val ic = currentInputConnection ?: return
        ic.commitText(ch, 1)
        pending.clear()
        keyboard?.suggestions = emptyList()
    }

    /** Live predictions for the current pending word (lexicon is lower-case). */
    private fun updateSuggestions() {
        val prefix = pending.toString()
        keyboard?.suggestions = if (prefix.isEmpty()) {
            emptyList()
        } else {
            runCatching { bridge.suggest("", prefix.lowercase()).map { it.word } }
                .getOrDefault(emptyList())
                .take(3)
        }
    }

    /** Commit a tapped suggestion, replacing the pending word. */
    private fun commitSuggestion(index: Int) {
        val ic = currentInputConnection ?: return
        val word = keyboard?.suggestions?.getOrNull(index) ?: return
        val cur = pending.toString()
        if (cur.isNotEmpty()) ic.deleteSurroundingText(cur.length, 0)
        ic.commitText("$word ", 1)
        runCatching { bridge.learnWord(word, field) } // gated (BR-26)
        pending.clear()
        keyboard?.suggestions = emptyList()
        schedulePersist()
    }

    /** Word boundary: correct the pending word, learn it (gated), then space. */
    private fun boundary(ic: InputConnection) {
        val word = pending.toString()
        if (word.isNotEmpty()) {
            val correction = runCatching { bridge.correct(word, "", word) }.getOrNull()
            if (correction != null && correction.applied) {
                ic.deleteSurroundingText(word.length, 0)
                ic.commitText(correction.primary, 1)
            }
            runCatching { bridge.learnWord(correction?.primary ?: word, field) } // gated (BR-26)
        }
        ic.commitText(" ", 1)
        pending.clear()
        keyboard?.suggestions = emptyList()
        schedulePersist()
    }

    private fun backspace(ic: InputConnection) {
        if (pending.isNotEmpty()) pending.deleteCharAt(pending.length - 1)
        ic.deleteSurroundingText(1, 0)
    }

    private fun flushWord(ic: InputConnection) {
        val word = pending.toString()
        if (word.isNotEmpty()) runCatching { bridge.learnWord(word, field) }
        pending.clear()
    }

    /** Globe: let the user switch to another keyboard. */
    private fun showImePicker() {
        (getSystemService(INPUT_METHOD_SERVICE) as? InputMethodManager)?.showInputMethodPicker()
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
        }
    }

    override fun onDestroy() {
        recognizer?.destroy()
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
 * Bundled MVP lexicons. Ships a tiny placeholder; replace with real per-language
 * word lists loaded from `assets/lexicons/<tag>.txt` (sorted, one word per line).
 */
object Lexicons {
    fun bundled(service: InputMethodService): List<Language> {
        val en = listOf("a", "an", "and", "hello", "the", "world").sorted()
        return listOf(Language("en", en))
    }
}
