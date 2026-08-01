//! FFI value types (records/enums) marshalled across the UniFFI boundary, and
//! their pure conversions to/from the core's own domain types.
//!
//! Split out of `ffi.rs` purely to keep both files under the repo's file-size
//! fitness gate (ARCH §4 / `core/tools/fitness/check.py`): this module has no
//! behaviour of its own beyond `From`/mapping impls, so it carries no
//! additional design weight — `ffi.rs` still owns the exported `KeyboardCore`
//! object and the foreign-trait boundary.

use crate::correct::AutocorrectOutcome;
use crate::{DecodeResult, FeatherKeyError};

/// One active language and its sorted lexicon, as handed across the FFI.
#[derive(uniffi::Record)]
pub struct LanguagePack {
    pub tag: String,
    /// Words in non-decreasing (sorted) order — see `Dictionary` contract.
    pub words: Vec<String>,
    /// Canonical-cased proper nouns for this language (BR-69). Unordered.
    pub proper: Vec<String>,
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
    /// When the neural gate withheld an otherwise-available correction
    /// (`applied == false` but a winner existed), the winner it declined to
    /// apply — mirrors [`featherkey_contracts::Correction::withheld`]. `None`
    /// when nothing was withheld (a correction applied, or there was no
    /// candidate to gate). The shell surfaces this as the counterfactual
    /// "reached" signal for `observe_autocorrect_outcome`.
    pub withheld: Option<String>,
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

/// Where a shell-gathered candidate came from — mirrors [`featherkey_contracts::Source`].
#[derive(Debug, uniffi::Enum)]
pub enum FfiSource {
    Lexicon,
    Device,
}

/// The Latin arrangement a user picked, or `Auto` (per-language default).
/// Mirrors [`crate::LatinLayout`] plus an `Auto` variant for "no override".
#[derive(Debug, uniffi::Enum)]
pub enum FfiLatinLayout {
    Auto,
    Qwerty,
    Qwertz,
    Azerty,
}

/// The real-world outcome of the last gated correction (Task 9's training
/// signal), marshalled across the FFI. Mirrors [`AutocorrectOutcome`] 1:1.
#[derive(Debug, uniffi::Enum)]
pub enum FfiAutocorrectOutcome {
    /// The user reverted/undid it: push the gate toward "withhold".
    Reverted,
    /// The user kept it, with no strong signal either way.
    Kept,
    /// The user typed on past it cleanly: a confirming signal.
    Reached,
}

impl From<FfiAutocorrectOutcome> for AutocorrectOutcome {
    fn from(o: FfiAutocorrectOutcome) -> Self {
        match o {
            FfiAutocorrectOutcome::Reverted => AutocorrectOutcome::Reverted,
            FfiAutocorrectOutcome::Kept => AutocorrectOutcome::Kept,
            FfiAutocorrectOutcome::Reached => AutocorrectOutcome::Reached,
        }
    }
}

/// Pure boundary mapping (kept out of the exported method so it is unit-testable).
pub fn map_latin(layout: FfiLatinLayout) -> Option<crate::LatinLayout> {
    use crate::LatinLayout;
    match layout {
        FfiLatinLayout::Auto => None,
        FfiLatinLayout::Qwerty => Some(LatinLayout::Qwerty),
        FfiLatinLayout::Qwertz => Some(LatinLayout::Qwertz),
        FfiLatinLayout::Azerty => Some(LatinLayout::Azerty),
    }
}

/// One correction/suggestion candidate the shell gathered (e.g. from the device
/// spell-checker), tagged by language and its rank within its own source.
#[derive(Debug, uniffi::Record)]
pub struct FfiRankCandidate {
    pub word: String,
    pub lang: String,
    pub source: FfiSource,
    pub source_rank: u32,
}

/// A candidate after ranking, in final blended-score order.
#[derive(uniffi::Record)]
pub struct FfiRanked {
    pub word: String,
    pub lang: String,
}

/// One learned word and its observed frequency, handed to the shell's swipe
/// decoder so gesture paths can be ranked by the user's own usage.
#[derive(uniffi::Record)]
pub struct FfiWordFreq {
    pub word: String,
    pub freq: u32,
}

/// One key's learned tap-offset bias, for the shell to re-centre gesture key
/// positions before swipe decoding.
#[derive(uniffi::Record)]
pub struct FfiTapOffset {
    pub key: String,
    pub dx: f32,
    pub dy: f32,
}

/// One `prev -> next` bigram transition with its count, for migrating the legacy
/// Kotlin `context.tsv` into the encrypted core.
#[derive(uniffi::Record)]
pub struct FfiTransition {
    pub prev: String,
    pub next: String,
    pub count: u32,
}

impl From<FfiRankCandidate> for featherkey_contracts::Candidate {
    fn from(c: FfiRankCandidate) -> Self {
        featherkey_contracts::Candidate {
            word: c.word,
            lang: c.lang,
            source: match c.source {
                FfiSource::Lexicon => featherkey_contracts::Source::Lexicon,
                FfiSource::Device => featherkey_contracts::Source::Device,
            },
            source_rank: c.source_rank,
        }
    }
}
