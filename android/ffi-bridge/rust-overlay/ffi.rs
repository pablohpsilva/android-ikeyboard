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

use crate::{DecodeResult, FeatherKeyCore, FeatherKeyError};

// ---- FFI value types (records/enums the bindings marshal) ------------------

/// One active language and its sorted lexicon, as handed across the FFI.
#[derive(uniffi::Record)]
pub struct LanguagePack {
    pub tag: String,
    /// Words in non-decreasing (sorted) order — see `Dictionary` contract.
    pub words: Vec<String>,
}

/// A decoded key candidate for the shell to render.
#[derive(uniffi::Record)]
pub struct FfiCandidate {
    pub key: String,
    pub confidence: f32,
}

/// The outcome of decoding one touch.
#[derive(uniffi::Record)]
pub struct FfiDecode {
    pub best: Option<String>,
    pub candidates: Vec<FfiCandidate>,
}

/// A ranked completion.
#[derive(uniffi::Record)]
pub struct FfiSuggestion {
    pub word: String,
    pub score: u32,
}

/// An autocorrect decision.
#[derive(uniffi::Record)]
pub struct FfiCorrection {
    pub primary: String,
    pub alternatives: Vec<String>,
    pub applied: bool,
}

/// One key of the active layout for the shell to render, in the layout's logical
/// coordinate space. What the shell draws from these is exactly what `decode`
/// resolves against.
#[derive(uniffi::Record)]
pub struct FfiKey {
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// The single flat error the whole surface returns. Mirrors [`FeatherKeyError`]
/// plus the two FFI-only failures (bad key length, store-open failure).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("at least one active language is required")]
    NoLanguages,
    #[error("a language word list is not a sorted set")]
    Lexicon,
    #[error("the active-language set was rejected")]
    Locale,
    #[error("the active layout has no keys to decode against")]
    EmptyLayout,
    #[error("the tap observation was rejected")]
    TouchModel,
    #[error("the secure store failed")]
    Store,
    #[error("the device key must be exactly 32 bytes")]
    BadKeyLength,
    #[error("the secure store could not be opened")]
    StoreOpen,
}

impl From<FeatherKeyError> for FfiError {
    fn from(e: FeatherKeyError) -> Self {
        match e {
            FeatherKeyError::NoLanguages => FfiError::NoLanguages,
            FeatherKeyError::Lexicon => FfiError::Lexicon,
            FeatherKeyError::Locale => FfiError::Locale,
            FeatherKeyError::EmptyLayout => FfiError::EmptyLayout,
            FeatherKeyError::TouchModel => FfiError::TouchModel,
            FeatherKeyError::Store => FfiError::Store,
        }
    }
}

impl From<DecodeResult> for FfiDecode {
    fn from(d: DecodeResult) -> Self {
        FfiDecode {
            best: d.best,
            candidates: d
                .candidates
                .into_iter()
                .map(|c| FfiCandidate {
                    key: c.key,
                    confidence: c.confidence,
                })
                .collect(),
        }
    }
}

impl From<crate::LayoutKey> for FfiKey {
    fn from(k: crate::LayoutKey) -> Self {
        FfiKey {
            label: k.label,
            x: k.x,
            y: k.y,
            width: k.width,
            height: k.height,
        }
    }
}

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
        let core = self.lock();
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

    /// Correct `text` in its context (never clobbers an intended word).
    pub fn correct(
        &self,
        text: String,
        preceding: String,
        prefix: String,
    ) -> Result<FfiCorrection, FfiError> {
        let core = self.lock();
        let c = core.correct(&text, &preceding, &prefix)?;
        Ok(FfiCorrection {
            primary: c.primary,
            alternatives: c.alternatives,
            applied: c.applied,
        })
    }

    /// Learn `word` — unless `field` is sensitive (E-2 / BR-26).
    pub fn learn_word(&self, word: String, field: std::sync::Arc<dyn SensitiveField>) {
        let mut core = self.lock();
        core.learn_word(&word, &FieldSource(field.as_ref()));
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
        self.lock().layout_keys().into_iter().map(FfiKey::from).collect()
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
}

impl KeyboardCore {
    /// Lock the core. `Mutex` poisoning can only happen if a prior holder
    /// panicked while holding it; the core is panic-free by construction
    /// (SEDD §5.5), so we recover the guard rather than propagate poisoning.
    fn lock(&self) -> std::sync::MutexGuard<'_, FeatherKeyCore> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}
