//! The single error type the façade surfaces to the shell.
//!
//! Every fallible sub-crate error (a bad lexicon, an empty layout, a rejected
//! language set, a tap-model or storage failure) is folded into one flat,
//! `#[non_exhaustive]` enum so the UniFFI surface exposes a single, stable error
//! shape rather than leaking each internal crate's error type across the FFI
//! boundary. Errors are values, never panics (SEDD §5.5 r3).

use core::fmt;

use featherkey_contracts::StoreError;
use featherkey_dictionary::DictionaryError;
use featherkey_kernel::CoreError;
use featherkey_locale_manager::LocaleError;
use featherkey_touch_model::TouchModelError;

/// Everything the core can fail with, as one flat enum for the FFI surface.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FeatherKeyError {
    /// The core was configured with no active language; at least one is required.
    NoLanguages,
    /// A language's word list was not a valid non-decreasing sorted set.
    Lexicon,
    /// The requested active-language set was rejected (empty or duplicate tag).
    Locale,
    /// A keystroke could not be decoded because the active layout has no keys.
    EmptyLayout,
    /// A tap observation carried a non-finite offset and was rejected.
    TouchModel,
    /// The secure store failed to persist or load personal data.
    Store,
}

impl fmt::Display for FeatherKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            FeatherKeyError::NoLanguages => "at least one active language is required",
            FeatherKeyError::Lexicon => "a language word list is not a sorted set",
            FeatherKeyError::Locale => "the active-language set was rejected",
            FeatherKeyError::EmptyLayout => "the active layout has no keys to decode against",
            FeatherKeyError::TouchModel => "the tap observation was rejected",
            FeatherKeyError::Store => "the secure store failed",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for FeatherKeyError {}

impl From<DictionaryError> for FeatherKeyError {
    fn from(_: DictionaryError) -> Self {
        FeatherKeyError::Lexicon
    }
}

impl From<CoreError> for FeatherKeyError {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::EmptyLayout => FeatherKeyError::EmptyLayout,
            // `CoreError` is #[non_exhaustive]; any future variant maps to the
            // closest surface error rather than panicking.
            _ => FeatherKeyError::EmptyLayout,
        }
    }
}

impl From<LocaleError> for FeatherKeyError {
    fn from(err: LocaleError) -> Self {
        match err {
            LocaleError::NoActiveLanguages => FeatherKeyError::NoLanguages,
            LocaleError::DuplicateLanguage => FeatherKeyError::Locale,
            // `LocaleError` is #[non_exhaustive]; a future variant folds into the
            // closest surface error rather than panicking.
            _ => FeatherKeyError::Locale,
        }
    }
}

impl From<TouchModelError> for FeatherKeyError {
    fn from(_: TouchModelError) -> Self {
        FeatherKeyError::TouchModel
    }
}

impl From<StoreError> for FeatherKeyError {
    fn from(_: StoreError) -> Self {
        FeatherKeyError::Store
    }
}
