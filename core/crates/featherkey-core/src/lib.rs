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
mod packs;
mod rank;

#[cfg(feature = "uniffi")]
mod ffi;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

pub use crate::error::FeatherKeyError;

// Re-exported so the shell depends only on this façade, never on the internal
// crates directly (SEDD §3.6, EP-3 boundary).
pub use featherkey_contracts::{
    Candidate, Correction, Namespace, RankedCandidate, SecureStore, SensitiveContextSource,
    StoreError, Suggestion, Suggestions, Token, TypingContext,
};
pub use featherkey_layout_engine::Layout;
pub use featherkey_secure_store::RedbSecureStore;

use featherkey_context::Context;
use featherkey_contracts::Predictor;
use featherkey_corrections::Corrections;
use featherkey_dictionary::Dictionary;
use featherkey_input_decoder::{InputDecoder, NearestKeyDecoder};
use featherkey_kernel::TouchPoint;
use featherkey_language_momentum::Momentum;
use featherkey_locale_manager::LangId;
use featherkey_personalization::Personalization;

use crate::packs::{build_packs, primary_tag, Pack};
use featherkey_prediction::StatisticalPredictor;
use featherkey_sensitive_context::SensitivityPolicy;
use featherkey_touch_model::TouchModel;

/// Weight of the correction "sticky-fix" bonus in the strip ranking. A candidate
/// the user has repeatedly picked for the current prefix gets
/// `CORRECTION_STICKY_WEIGHT * ln(1 + picks)` added to its rank score. At weight
/// `1.0` the crossover is intuitive against the ranker's positional step
/// (`positional_score(rank) = -ln(1 + rank)`): the widest adjacent gap is
/// rank0→rank1 (`ln 2 ≈ 0.69`), which `ln(1 + picks)` ties at the 1st pick and
/// clears by the 2nd (`ln 3 ≈ 1.10`) — so a *repeatedly* chosen lower suggestion
/// overtakes a higher default in about two picks, while a single stray pick never
/// dominates. The bonus grows sub-linearly, so it saturates rather than runs away.
const CORRECTION_STICKY_WEIGHT: f64 = 1.0;

/// Weight of the correction "unwanted" demotion — a word the user repeatedly
/// deletes and retypes (`observe_delete_retype`) is pushed *down* by
/// `CORRECTION_UNWANTED_WEIGHT * ln(1 + unwanted)`. Half the promotion weight on
/// purpose: a delete-retype is a weaker, noisier negative signal than an explicit
/// pick, so a single one is only a ~0.35 nudge (well under the `ln 2 ≈ 0.69`
/// rank0→rank1 step) and cannot bury a strong default; it takes ~4 delete-retypes
/// to move a word down one rank position. Like the promotion it saturates.
const CORRECTION_UNWANTED_WEIGHT: f64 = 0.5;

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

/// One key of the active layout, in the layout's logical coordinate space — the
/// shell renders each `label` at `(x, y, width, height)` and reports touches back
/// in the same space, so what is drawn is exactly what the core decodes.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutKey {
    /// The character the key commits (e.g. `"q"`, `"1"`, `"."`).
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
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
    /// On-device next-word (bigram) model, persisted under `PersonalLm`.
    context: Context,
    /// On-device correction-signal model, persisted under `Corrections`.
    corrections: Corrections,
    /// Active languages, each with its validated lexicon, in preference order.
    packs: Vec<Pack>,
    sensitivity: SensitivityPolicy,
    /// Recency-weighted per-language weight tracking which active language the
    /// user is currently writing in; seeded on construction and re-seeded on
    /// every language switch.
    momentum: Momentum,
}

impl FeatherKeyCore {
    /// Assemble a core over one or more active languages, each a `(tag, words)`
    /// pair whose `words` are in **frequency order** (most-common first — the
    /// shell's asset order). The core records each word's input position as its
    /// bundled rank and byte-sorts internally for the `fst`, so word *order* is
    /// no longer a rejection reason (DECISION option A). The alpha page follows
    /// the primary (first) language's script (`Layout::alpha_for`), so a Cyrillic
    /// or Greek locale opens on a native block; switch pages with
    /// [`Self::use_numeric_layout`] / [`Self::use_symbols_layout`] /
    /// [`Self::use_alpha_layout`].
    ///
    /// # Errors
    /// - [`FeatherKeyError::NoLanguages`] if `languages` is empty.
    /// - [`FeatherKeyError::Locale`] if two languages share a tag.
    pub fn new(languages: Vec<(String, Vec<String>)>) -> Result<Self, FeatherKeyError> {
        let packs = build_packs(languages)?;
        let primary = primary_tag(&packs);
        let tags: Vec<String> = packs.iter().map(|p| p.lang.as_str().to_owned()).collect();
        Ok(Self {
            layout: Layout::alpha_for(&primary),
            decoder: NearestKeyDecoder::new(),
            touch_model: TouchModel::default(),
            personalization: Personalization::new(),
            context: Context::new(),
            corrections: Corrections::new(),
            packs,
            sensitivity: SensitivityPolicy::new(),
            momentum: Momentum::new(&primary, &tags),
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
        // The alpha script follows the (new) primary language.
        let primary = primary_tag(&self.packs);
        let tags: Vec<String> = self
            .packs
            .iter()
            .map(|p| p.lang.as_str().to_owned())
            .collect();
        self.momentum.set_languages(&primary, &tags);
        self.layout = Layout::alpha_for(&primary);
        Ok(())
    }

    /// The active language tags, in preference order (ARCH §9.1 `ActiveLanguages`).
    #[must_use]
    pub fn active_languages(&self) -> Vec<String> {
        self.packs
            .iter()
            .map(|p| p.lang.as_str().to_owned())
            .collect()
    }

    /// Fold one committed word's recogniser languages into momentum. Caller is
    /// responsible for consent/sensitivity gating (this is not called in a
    /// sensitive field or with learning disabled).
    pub fn observe_language(&mut self, recognizers: Vec<String>) {
        self.momentum.observe(&recognizers);
    }

    /// Current momentum weight for `lang` (test/inspection seam).
    #[must_use]
    pub fn language_weight(&self, lang: &str) -> f64 {
        self.momentum.weight_of(lang)
    }

    /// Swap the active on-screen layout page (alpha/numeric/symbol, or any
    /// custom [`Layout`]). The composition root owns which page is live.
    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    /// Switch back to the alpha letter page for the primary active language.
    pub fn use_alpha_layout(&mut self) {
        self.layout = Layout::alpha_for(&primary_tag(&self.packs));
    }

    /// Switch to the numeric page.
    pub fn use_numeric_layout(&mut self) {
        self.layout = Layout::numeric();
    }

    /// Switch to the symbols page.
    pub fn use_symbols_layout(&mut self) {
        self.layout = Layout::symbols();
    }

    /// The keys of the active layout, in the layout's logical coordinate space,
    /// for the shell to render (ARCH §9.1). What the shell draws from this is
    /// exactly what [`Self::decode`] resolves against.
    #[must_use]
    pub fn layout_keys(&self) -> Vec<LayoutKey> {
        self.layout
            .keys()
            .iter()
            .map(|k| LayoutKey {
                label: k.id.ch().to_string(),
                x: k.x,
                y: k.y,
                width: k.width,
                height: k.height,
            })
            .collect()
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

    /// Rank shell-gathered candidates (bundled + device + decode) with the current
    /// language momentum. Read-only.
    #[must_use]
    pub fn rank_candidates(&self, cands: Vec<Candidate>, k: usize) -> Vec<RankedCandidate> {
        featherkey_candidate_ranker::rank(&cands, &self.momentum, k)
    }

    /// Clone each active lexicon — the derived read engines (predictor, corrector)
    /// own their inputs by value, so the façade hands them clones of its packs.
    fn lexicon_clones(&self) -> Vec<Dictionary> {
        self.packs.iter().map(|p| p.dict.clone()).collect()
    }

    /// The active packs as `(LangId, Dictionary)` pairs for [`LocaleManager`],
    /// which only needs the tag+lexicon (not the frequency rank).
    fn locale_packs(&self) -> Vec<(LangId, Dictionary)> {
        self.packs
            .iter()
            .map(|p| (p.lang.clone(), p.dict.clone()))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn observing_a_language_raises_its_weight() {
        let mut core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["hello".into()]),
            ("es".into(), vec!["hola".into()]),
        ])
        .expect("core");
        let before = core.language_weight("es");
        core.observe_language(vec!["es".into()]);
        assert!(core.language_weight("es") > before * 0.9); // bumped past pure decay
    }

    #[test]
    fn switching_languages_reseeds_momentum() {
        let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["hi".into()])]).expect("core");
        core.set_active_languages(vec![("es".into(), vec!["hola".into()])])
            .expect("switch");
        assert!(core.language_weight("es") >= core.language_weight("en"));
    }

    #[test]
    fn rank_candidates_uses_momentum() {
        use featherkey_contracts::{Candidate, Source};
        let mut core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["hello".into()]),
            ("es".into(), vec!["hola".into()]),
        ])
        .expect("core");
        for _ in 0..5 {
            core.observe_language(vec!["es".into()]);
        }
        let cands = vec![
            Candidate {
                word: "hello".into(),
                lang: "en".into(),
                source: Source::Lexicon,
                source_rank: 0,
            },
            Candidate {
                word: "hola".into(),
                lang: "es".into(),
                source: Source::Lexicon,
                source_rank: 0,
            },
        ];
        let out = core.rank_candidates(cands, 2);
        assert_eq!(out[0].word, "hola");
    }

    #[test]
    fn learn_word_records_both_frequency_and_context_when_allowed() {
        struct Ordinary;
        impl featherkey_contracts::SensitiveContextSource for Ordinary {
            fn is_sensitive(&self) -> bool {
                false
            }
        }
        let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
        core.learn_word("the", "cat", &Ordinary);
        assert_eq!(core.word_frequency("cat"), 1);
        assert_eq!(core.context_next_words("the", 5), vec!["cat".to_string()]);
    }

    #[test]
    fn correction_hooks_record_when_field_is_ordinary() {
        struct Ordinary;
        impl featherkey_contracts::SensitiveContextSource for Ordinary {
            fn is_sensitive(&self) -> bool {
                false
            }
        }
        let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["teh".into()])]).expect("core");
        core.observe_strip_pick("teh", "teh", &Ordinary);
        core.observe_delete_retype("ducking", &Ordinary);
        assert_eq!(core.correction_pref_count("teh", "teh"), 1);
        assert_eq!(core.correction_unwanted_count("ducking"), 1);
    }
}
