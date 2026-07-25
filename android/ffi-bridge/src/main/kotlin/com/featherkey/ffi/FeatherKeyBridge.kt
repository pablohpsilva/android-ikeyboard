package com.featherkey.ffi

/*
 * Thin, curated wrapper over the UniFFI-generated `KeyboardCore` bindings.
 *
 * ⚠️ AUTHORED, NOT COMPILED. The `com.featherkey.ffi.generated.*` symbols below
 * are produced by `uniffi-bindgen` from crates/featherkey-core/src/ffi.rs (see
 * android/ffi-bridge/rust-overlay/APPLY.md). Their exact names (e.g. whether the
 * error type is `FfiException` and the constructor is `KeyboardCore.open`) are
 * UniFFI-version dependent — reconcile this file against the actually-generated
 * code the first time you build. The wrapper exists so the rest of the app codes
 * against a stable, non-generated surface.
 */

import com.featherkey.ffi.generated.FfiCorrection
import com.featherkey.ffi.generated.FfiDecode
import com.featherkey.ffi.generated.FfiRankCandidate
import com.featherkey.ffi.generated.FfiRanked
import com.featherkey.ffi.generated.FfiSource
import com.featherkey.ffi.generated.FfiSuggestion
import com.featherkey.ffi.generated.KeyboardCore
import com.featherkey.ffi.generated.LanguagePack
import com.featherkey.ffi.generated.SensitiveField

/** A language and its sorted lexicon, shell-side. */
data class Language(val tag: String, val words: List<String>)

/** One layout key for the shell to render, in the layout's logical space. */
data class LayoutKeyDto(
    val label: String,
    val x: Float,
    val y: Float,
    val width: Float,
    val height: Float,
)

/** The active layout page. */
enum class LayoutPage { ALPHA, NUMERIC, SYMBOLS }

/** The current field's sensitivity, supplied by the IME from `EditorInfo`. */
fun interface FieldSensitivity {
    fun isSensitive(): Boolean
}

/**
 * Owns the single native core handle for the process. Construct once (per active
 * IME session) via [open]; call [close] to release the native object.
 */
class FeatherKeyBridge private constructor(private val core: KeyboardCore) : AutoCloseable {

    companion object {
        /**
         * Open the core over [languages], backed by an encrypted DB at [dbPath]
         * keyed by [deviceKey] (32 bytes from the Android Keystore, BR-62).
         */
        fun open(dbPath: String, deviceKey: ByteArray, languages: List<Language>): FeatherKeyBridge {
            require(deviceKey.size == 32) { "device key must be 32 bytes" }
            val packs = languages.map { LanguagePack(it.tag, it.words) }
            return FeatherKeyBridge(KeyboardCore.open(dbPath, deviceKey, packs))
        }
    }

    fun decode(x: Float, y: Float): FfiDecode = core.decode(x, y)

    fun suggest(preceding: String, prefix: String): List<FfiSuggestion> =
        core.suggest(preceding, prefix)

    fun correct(text: String, preceding: String, prefix: String): FfiCorrection =
        core.correct(text, preceding, prefix)

    /** Rank shell-gathered candidates with current language momentum. */
    fun rank(candidates: List<FfiRankCandidate>, k: UInt): List<FfiRanked> = core.rank(candidates, k)

    /** Multilingual momentum-aware correction (never clobbers a known word). */
    fun chooseCorrection(
        text: String,
        deviceKnown: List<String>,
        deviceCands: List<FfiRankCandidate>,
    ): FfiCorrection = core.chooseCorrection(text, deviceKnown, deviceCands)

    /**
     * Fold a committed word's recogniser languages into momentum. The caller must
     * only invoke this when consent is on and the field is not sensitive.
     */
    fun observeLanguage(recognizers: List<String>) = core.observeLanguage(recognizers)

    /** Learn a committed word unless the field is sensitive (E-2 / BR-26). */
    fun learnWord(word: String, field: FieldSensitivity) =
        core.learnWord(word, field.asForeign())

    /** Fold a tap offset unless the field is sensitive (E-2 / BR-26). */
    fun observeTap(key: String, dx: Float, dy: Float, field: FieldSensitivity) =
        core.observeTap(key, dx, dy, field.asForeign())

    fun addToDictionary(word: String) = core.addToDictionary(word)

    /** The keys of the active layout for the view to render. */
    fun layoutKeys(): List<LayoutKeyDto> =
        core.layoutKeys().map { LayoutKeyDto(it.label, it.x, it.y, it.width, it.height) }

    /** Switch layout page; fetch [layoutKeys] again afterwards. */
    fun setPage(page: LayoutPage) = when (page) {
        LayoutPage.ALPHA -> core.useAlphaLayout()
        LayoutPage.NUMERIC -> core.useNumericLayout()
        LayoutPage.SYMBOLS -> core.useSymbolsLayout()
    }

    fun activeLanguages(): List<String> = core.activeLanguages()

    fun setActiveLanguages(languages: List<Language>) =
        core.setActiveLanguages(languages.map { LanguagePack(it.tag, it.words) })

    /** Persist learned vocabulary. Call off the input thread (debounced). */
    fun persist() = core.persist()

    override fun close() = core.destroy() // UniFFI object disposal

    private fun FieldSensitivity.asForeign(): SensitiveField =
        object : SensitiveField {
            override fun isSensitive(): Boolean = this@asForeign.isSensitive()
        }
}
