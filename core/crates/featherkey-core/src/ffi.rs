//! UniFFI export layer for `featherkey-core` — the Wave 5 activation of ADR-18.
//!
//! ⚠️ AUTHORED, NOT COMPILED. This file was written without an Android
//! NDK/UniFFI toolchain in reach. Treat every macro path and generated-binding
//! name as *intended* until `cargo build --features uniffi` and
//! `cargo run --bin uniffi-bindgen generate` succeed on your machine. See
//! `android/BUILD_AND_RUN.md` §3 for the exact steps and the small edits this
//! overlay requires in `crates/featherkey-core/{Cargo.toml,src/lib.rs}`.
//!
//! Design: a single UniFFI **object** `KeyboardCore` wraps the (non-Sync) core
//! behind a `Mutex`, and owns the `RedbSecureStore` adapter opened from the key
//! the shell provisions via the Android Keystore (BR-62). The Kotlin side never
//! sees the raw key beyond handing us its bytes; the key lives in Rust in a
//! zeroizing buffer inside `secure-store`.

use std::sync::Mutex;

use featherkey_secure_store::RedbSecureStore;

use crate::FeatherKeyCore;

mod ffi_types;
use ffi_types::*;

// ---- Foreign trait: the current field's sensitivity (BR-26 gate input) -----

/// Implemented in Kotlin from `EditorInfo`; Rust consults it to decide whether a
/// field is sensitive. Marshalled as a UniFFI foreign (callback) trait.
#[uniffi::export(with_foreign)]
pub trait SensitiveField: Send + Sync {
    fn is_sensitive(&self) -> bool;
}

/// Adapt the foreign trait object to the core's `SensitiveContextSource` port.
struct FieldSource<'a>(&'a dyn SensitiveField);
impl featherkey_contracts::SensitiveContextSource for FieldSource<'_> {
    fn is_sensitive(&self) -> bool {
        self.0.is_sensitive()
    }
}

// ---- The exported object ---------------------------------------------------

/// The one handle the shell holds for the whole Rust core. Thread-safe: the
/// non-`Sync` core sits behind a `Mutex`; the input path is single-threaded on
/// the IME side, so contention is nil in practice.
#[derive(uniffi::Object)]
pub struct KeyboardCore {
    inner: Mutex<FeatherKeyCore>,
    store: RedbSecureStore,
}

#[uniffi::export]
impl KeyboardCore {
    /// Open the core over `languages`, backed by an encrypted store at
    /// `db_path`, keyed by the 32-byte `device_key` the shell provisioned from
    /// the Android Keystore (BR-62). Learned vocabulary from a previous session
    /// is reloaded immediately.
    #[uniffi::constructor]
    pub fn open(
        db_path: String,
        device_key: Vec<u8>,
        languages: Vec<LanguagePack>,
    ) -> Result<std::sync::Arc<Self>, FfiError> {
        let key: [u8; 32] = device_key.try_into().map_err(|_| FfiError::BadKeyLength)?;
        let store = RedbSecureStore::open(&db_path, key).map_err(|_| FfiError::StoreOpen)?;
        let langs = languages.into_iter().map(|p| (p.tag, p.words)).collect();
        let mut core = FeatherKeyCore::new(langs)?;
        // Best-effort reload; an absent blob is a clean first run (returns Ok).
        core.restore(&store)?;
        Ok(std::sync::Arc::new(Self {
            inner: Mutex::new(core),
            store,
        }))
    }

    /// Decode a touch at surface-local pixel `(x, y)`.
    pub fn decode(&self, x: f32, y: f32) -> Result<FfiDecode, FfiError> {
        let mut core = self.lock();
        Ok(core.decode(x, y)?.into())
    }

    /// Ranked completions for `prefix` in `preceding` context.
    pub fn suggest(&self, preceding: String, prefix: String) -> Vec<FfiSuggestion> {
        let core = self.lock();
        core.suggest(&preceding, &prefix)
            .items
            .into_iter()
            .map(|s| FfiSuggestion {
                word: s.word,
                score: s.score,
            })
            .collect()
    }

    /// Uncalled alias of [`choose_correction`](Self::choose_correction), kept
    /// only because the committed UniFFI bindings cannot be regenerated offline
    /// (ADR-21). Signature frozen — argument names included.
    pub fn correct(
        &self,
        text: String,
        preceding: String,
        prefix: String,
    ) -> Result<FfiCorrection, FfiError> {
        let _ = (&preceding, &prefix); // unused; names frozen for the bindings
        let core = self.lock();
        let c = core.choose_correction(&text, &[], Vec::new())?;
        Ok(FfiCorrection {
            primary: c.primary,
            alternatives: c.alternatives,
            applied: c.applied,
        })
    }

    /// Learn `word` typed after `preceding` — updating both the learned
    /// vocabulary and the next-word model — unless `field` is sensitive
    /// (E-2 / BR-26), in which case both are left untouched.
    pub fn learn_word(
        &self,
        preceding: String,
        word: String,
        field: std::sync::Arc<dyn SensitiveField>,
    ) {
        let mut core = self.lock();
        core.learn_word(&preceding, &word, &FieldSource(field.as_ref()));
    }

    /// The whole suggestion-strip blend, core-owned: predictor completions +
    /// `device` candidates → momentum ranking → dictionary fold-group variant
    /// guarantee. The shell renders the returned words in order.
    pub fn rank_suggestions(
        &self,
        preceding: String,
        prefix: String,
        device: Vec<FfiRankCandidate>,
    ) -> Vec<FfiRanked> {
        let cands = device.into_iter().map(Into::into).collect();
        self.lock()
            .rank_suggestions(&preceding, &prefix, cands)
            .into_iter()
            .map(|r| FfiRanked {
                word: r.word,
                lang: r.lang,
            })
            .collect()
    }

    /// Every learned word paired with its observed frequency (for swipe ranking).
    pub fn learned_frequencies(&self) -> Vec<FfiWordFreq> {
        self.lock()
            .learned_frequencies()
            .into_iter()
            .map(|(word, freq)| FfiWordFreq { word, freq })
            .collect()
    }

    /// Every observed key's learned tap-offset bias (for gesture re-centring).
    pub fn tap_offsets(&self) -> Vec<FfiTapOffset> {
        self.lock()
            .tap_offsets()
            .into_iter()
            .map(|(key, dx, dy)| FfiTapOffset { key, dx, dy })
            .collect()
    }

    /// Record a strip pick (`prefix -> picked`) — unless `field` is sensitive.
    pub fn observe_strip_pick(
        &self,
        prefix: String,
        picked: String,
        field: std::sync::Arc<dyn SensitiveField>,
    ) {
        self.lock()
            .observe_strip_pick(&prefix, &picked, &FieldSource(field.as_ref()));
    }

    /// Record a delete-and-retype demotion for `word` — unless `field` is sensitive.
    pub fn observe_delete_retype(&self, word: String, field: std::sync::Arc<dyn SensitiveField>) {
        self.lock()
            .observe_delete_retype(&word, &FieldSource(field.as_ref()));
    }

    /// Migrate legacy `(prev, next, count)` bigram transitions into the encrypted
    /// next-word model (set-semantics; idempotent). A deliberate one-time import.
    pub fn import_context(&self, transitions: Vec<FfiTransition>) {
        self.lock()
            .import_context(transitions.into_iter().map(|t| (t.prev, t.next, t.count)));
    }

    /// Migrate legacy `(word, count)` learned frequencies into the encrypted
    /// personalization model (set-semantics; idempotent). A deliberate one-time
    /// import, paired with [`import_context`](Self::import_context).
    pub fn import_frequencies(&self, frequencies: Vec<FfiWordFreq>) {
        self.lock()
            .import_frequencies(frequencies.into_iter().map(|f| (f.word, f.freq)));
    }

    /// Fold a tap offset for `key` — unless `field` is sensitive (E-2 / BR-26).
    pub fn observe_tap(
        &self,
        key: String,
        dx: f32,
        dy: f32,
        field: std::sync::Arc<dyn SensitiveField>,
    ) -> Result<(), FfiError> {
        let ch = key.chars().next().ok_or(FfiError::TouchModel)?;
        let mut core = self.lock();
        core.observe_tap(ch, dx, dy, &FieldSource(field.as_ref()))?;
        Ok(())
    }

    /// Add `word` to the user dictionary (deliberate action; not gated).
    pub fn add_to_dictionary(&self, word: String) {
        self.lock().add_to_dictionary(&word);
    }

    /// The keys of the active layout, in logical coordinates, for the shell to
    /// render. Fetch again after any page switch below.
    pub fn layout_keys(&self) -> Vec<FfiKey> {
        self.lock()
            .layout_keys()
            .into_iter()
            .map(FfiKey::from)
            .collect()
    }

    /// Switch to the QWERTY letter page (the default).
    pub fn use_alpha_layout(&self) {
        self.lock().use_alpha_layout();
    }

    /// Switch to the numeric page.
    pub fn use_numeric_layout(&self) {
        self.lock().use_numeric_layout();
    }

    /// Switch to the symbols page.
    pub fn use_symbols_layout(&self) {
        self.lock().use_symbols_layout();
    }

    /// Choose the Latin key arrangement (`Auto` = per-language default). Latin-only:
    /// a Cyrillic/Greek primary keeps its native block.
    pub fn set_latin_layout(&self, layout: FfiLatinLayout) {
        self.lock().set_latin_layout(map_latin(layout));
    }

    /// Active language tags in preference order.
    pub fn active_languages(&self) -> Vec<String> {
        self.lock().active_languages()
    }

    /// Replace the active language set atomically.
    pub fn set_active_languages(&self, languages: Vec<LanguagePack>) -> Result<(), FfiError> {
        let langs = languages.into_iter().map(|p| (p.tag, p.words)).collect();
        self.lock().set_active_languages(langs)?;
        Ok(())
    }

    /// Encrypt and persist learned vocabulary. Call from a background thread on
    /// a debounce; it is off the input path.
    pub fn persist(&self) -> Result<(), FfiError> {
        let core = self.lock();
        core.persist(&self.store)?;
        Ok(())
    }

    /// Rank shell-gathered candidates with current language momentum.
    pub fn rank(&self, candidates: Vec<FfiRankCandidate>, k: u32) -> Vec<FfiRanked> {
        let cands = candidates.into_iter().map(Into::into).collect();
        self.lock()
            .rank_candidates(cands, k as usize)
            .into_iter()
            .map(|r| FfiRanked {
                word: r.word,
                lang: r.lang,
            })
            .collect()
    }

    /// Multilingual momentum-aware correction (never clobbers a known word).
    pub fn choose_correction(
        &self,
        text: String,
        device_known: Vec<String>,
        device_cands: Vec<FfiRankCandidate>,
    ) -> Result<FfiCorrection, FfiError> {
        let cands = device_cands.into_iter().map(Into::into).collect();
        let c = self.lock().choose_correction(&text, &device_known, cands)?;
        Ok(FfiCorrection {
            primary: c.primary,
            alternatives: c.alternatives,
            applied: c.applied,
        })
    }

    /// Fold a committed word's recogniser languages into momentum. The shell must
    /// only call this when consent is on and the field is not sensitive.
    pub fn observe_language(&self, recognizers: Vec<String>) {
        self.lock().observe_language(recognizers);
    }
}

impl KeyboardCore {
    /// Lock the core. `Mutex` poisoning can only happen if a prior holder
    /// panicked while holding it; the core is panic-free by construction
    /// (SEDD §5.5), so we recover the guard rather than propagate poisoning.
    fn lock(&self) -> std::sync::MutexGuard<'_, FeatherKeyCore> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_latin_layout_maps_auto_to_none() {
        use crate::LatinLayout;
        assert_eq!(map_latin(FfiLatinLayout::Auto), None);
        assert_eq!(map_latin(FfiLatinLayout::Qwerty), Some(LatinLayout::Qwerty));
        assert_eq!(map_latin(FfiLatinLayout::Qwertz), Some(LatinLayout::Qwertz));
        assert_eq!(map_latin(FfiLatinLayout::Azerty), Some(LatinLayout::Azerty));
    }

    #[test]
    fn ffi_candidate_converts_to_contract_candidate() {
        let c: featherkey_contracts::Candidate = FfiRankCandidate {
            word: "hola".into(),
            lang: "es".into(),
            source: FfiSource::Device,
            source_rank: 2,
        }
        .into();
        assert_eq!(c.word, "hola");
        assert_eq!(c.lang, "es");
        assert_eq!(c.source, featherkey_contracts::Source::Device);
        assert_eq!(c.source_rank, 2);
    }
}
