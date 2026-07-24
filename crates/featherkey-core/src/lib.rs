//! FeatherKey composition façade.
//!
//! This is the **composition root** for the Rust core (ARCH §9.3): the single
//! place that names concrete types and wires the domain crates behind the
//! `contracts` ports. Everywhere else depends on traits; here we assemble
//! [`NearestKeyDecoder`], [`StatisticalPredictor`], [`NoClobberCorrector`],
//! [`LocaleManager`], [`Personalization`], [`TouchModel`] and the
//! [`SecureStore`] adapter into one [`FeatherKeyCore`] handle, and expose the
//! narrow use-case API the shell calls (ARCH §9.1: decode, suggest, correct,
//! switch/active languages, learn-from-input, manage-user-dictionary).
//!
//! # UniFFI surface
//! The public methods are authored **UniFFI-ready** — owned plain types
//! (`String`, `f32`, `bool`, flat structs/enums) cross the boundary, and every
//! fallible call returns [`FeatherKeyError`], which has a `Display` message. The
//! actual `#[uniffi::export]` scaffolding and Kotlin-binding generation are
//! applied in Wave 5 (ADR-18): the workspace forbids `unsafe`, which UniFFI's
//! generated scaffolding requires, and binding generation needs the Android NDK.
//! Keeping the surface FFI-shaped now means Wave 5 annotates, it does not
//! redesign.
//!
//! # E-2 — sensitive-context ordering (BR-26)
//! Every learning entry point ([`FeatherKeyCore::learn_word`],
//! [`FeatherKeyCore::observe_tap`]) consults [`SensitivityPolicy`] *before*
//! touching any learned state, so a keystroke typed into a password/OTP field is
//! dropped before it can be observed. This ordering is proven by the property
//! test in `tests/e2_sensitive_ordering.rs`.

mod correct;
mod error;
mod learn;

pub use crate::error::FeatherKeyError;

// Re-exported so the shell depends only on this façade, never on the internal
// crates directly (SEDD §3.6, EP-3 boundary).
pub use featherkey_contracts::{
    Correction, Namespace, SecureStore, SensitiveContextSource, StoreError, Suggestion,
    Suggestions, Token, TypingContext,
};
pub use featherkey_layout_engine::Layout;
pub use featherkey_secure_store::RedbSecureStore;

use featherkey_contracts::Predictor;
use featherkey_dictionary::Dictionary;
use featherkey_input_decoder::{InputDecoder, NearestKeyDecoder};
use featherkey_kernel::TouchPoint;
use featherkey_locale_manager::{LangId, LocaleManager};
use featherkey_personalization::Personalization;
use featherkey_prediction::StatisticalPredictor;
use featherkey_sensitive_context::SensitivityPolicy;
use featherkey_touch_model::TouchModel;

/// One ranked key candidate for a touch: the committed character and the
/// decoder's confidence in it (`0.0..=1.0`).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyCandidate {
    /// The character the key commits.
    pub key: String,
    /// Inverse-distance confidence share for this key.
    pub confidence: f32,
}

/// The outcome of decoding one touch: the best key (if any) and the full ranked
/// candidate list, best first.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodeResult {
    /// The most likely committed character, or `None` for an empty candidate set.
    pub best: Option<String>,
    /// All candidates, best first.
    pub candidates: Vec<KeyCandidate>,
}

/// The composed core handle. Owns the single source of truth for learned and
/// language state; the derived read engines (predictor, corrector, locale
/// manager) are rebuilt on demand from it so there is no cache to fall stale.
#[derive(Debug)]
pub struct FeatherKeyCore {
    layout: Layout,
    decoder: NearestKeyDecoder,
    touch_model: TouchModel,
    personalization: Personalization,
    /// Active languages, each with its validated lexicon, in preference order.
    packs: Vec<(LangId, Dictionary)>,
    sensitivity: SensitivityPolicy,
}

impl FeatherKeyCore {
    /// Assemble a core over one or more active languages, each a `(tag, words)`
    /// pair whose `words` are a non-decreasing sorted set. The alpha tracer
    /// layout is active by default; switch pages with [`Self::set_layout`].
    ///
    /// # Errors
    /// - [`FeatherKeyError::NoLanguages`] if `languages` is empty.
    /// - [`FeatherKeyError::Lexicon`] if any word list is not a sorted set.
    /// - [`FeatherKeyError::Locale`] if two languages share a tag.
    pub fn new(languages: Vec<(String, Vec<String>)>) -> Result<Self, FeatherKeyError> {
        let packs = build_packs(languages)?;
        Ok(Self {
            layout: Layout::qwerty_tracer_row(),
            decoder: NearestKeyDecoder::new(),
            touch_model: TouchModel::default(),
            personalization: Personalization::new(),
            packs,
            sensitivity: SensitivityPolicy::new(),
        })
    }

    /// Replace the active language set atomically: the new set is fully validated
    /// before anything is committed, so a rejected switch leaves the current set
    /// intact (ARCH §9.1 `SwitchLanguage`).
    ///
    /// # Errors
    /// Same conditions as [`Self::new`].
    pub fn set_active_languages(
        &mut self,
        languages: Vec<(String, Vec<String>)>,
    ) -> Result<(), FeatherKeyError> {
        self.packs = build_packs(languages)?;
        Ok(())
    }

    /// The active language tags, in preference order (ARCH §9.1 `ActiveLanguages`).
    #[must_use]
    pub fn active_languages(&self) -> Vec<String> {
        self.packs
            .iter()
            .map(|(id, _)| id.as_str().to_owned())
            .collect()
    }

    /// Swap the active on-screen layout page (alpha/numeric/symbol, or any
    /// custom [`Layout`]). The composition root owns which page is live.
    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    /// Decode a touch at surface-local pixel `(x, y)` into ranked candidates
    /// (ARCH §9.1 `DecodeKeystroke`). Biased by the per-user tap model.
    ///
    /// # Errors
    /// [`FeatherKeyError::EmptyLayout`] if the active layout has no keys.
    pub fn decode(&self, x: f32, y: f32) -> Result<DecodeResult, FeatherKeyError> {
        let candidates =
            self.decoder
                .decode(TouchPoint::new(x, y), &self.layout, &self.touch_model)?;
        let ranked = candidates
            .ranked()
            .iter()
            .map(|(id, conf)| KeyCandidate {
                key: id.ch().to_string(),
                confidence: conf.value(),
            })
            .collect();
        Ok(DecodeResult {
            best: candidates.best().map(|id| id.ch().to_string()),
            candidates: ranked,
        })
    }

    /// Ranked completions for the in-progress `prefix` in its `preceding` context
    /// (ARCH §9.1 `Suggest`). Read-only — never mutates learned state.
    #[must_use]
    pub fn suggest(&self, preceding: &str, prefix: &str) -> Suggestions {
        let predictor = StatisticalPredictor::new(self.lexicon_clones());
        predictor.suggest(&TypingContext {
            preceding: preceding.to_owned(),
            prefix: prefix.to_owned(),
        })
    }

    /// Clone each active lexicon — the derived read engines (predictor, corrector)
    /// own their inputs by value, so the façade hands them clones of its packs.
    fn lexicon_clones(&self) -> Vec<Dictionary> {
        self.packs.iter().map(|(_, d)| d.clone()).collect()
    }
}

/// Validate a `(tag, words)` language list into lexicon packs. Shared by
/// construction and language switching so both apply the identical contract:
/// non-empty, every list a sorted set, no duplicate tag.
fn build_packs(
    languages: Vec<(String, Vec<String>)>,
) -> Result<Vec<(LangId, Dictionary)>, FeatherKeyError> {
    let mut packs = Vec::with_capacity(languages.len());
    for (tag, words) in languages {
        packs.push((LangId::new(tag), Dictionary::from_sorted_words(words)?));
    }
    // Build a real LocaleManager purely to validate the set — it rejects an
    // empty set (→ NoLanguages) and a duplicate tag (→ Locale). Discarded;
    // `correct` rebuilds one on demand.
    LocaleManager::new(packs.clone())?;
    Ok(packs)
}
