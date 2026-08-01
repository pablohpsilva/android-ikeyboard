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
//! Keeping the surface FFI-shaped now means Wave 5 annotates, it does not redesign.
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
mod rank_features;
mod recent;
mod spatial;

#[cfg(feature = "uniffi")]
mod ffi;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

pub use crate::correct::AutocorrectOutcome;
pub use crate::error::FeatherKeyError;

// Re-exported so the shell depends only on this façade, never on the internal
// crates directly (SEDD §3.6, EP-3 boundary).
pub use featherkey_contracts::{
    Candidate, Correction, Namespace, RankedCandidate, SecureStore, SensitiveContextSource,
    StoreError, Suggestion, Suggestions, Token, TypingContext,
};
pub use featherkey_layout_engine::{LatinLayout, Layout, LayoutKind};
pub use featherkey_secure_store::RedbSecureStore;

use featherkey_autocorrect_gate::AutocorrectGate;
use featherkey_context::Context;
use featherkey_contracts::Predictor;
use featherkey_corrections::Corrections;
use featherkey_dictionary::Dictionary;
use featherkey_input_decoder::{InputDecoder, NearestKeyDecoder};
use featherkey_kernel::TouchPoint;
use featherkey_language_momentum::Momentum;
use featherkey_locale_manager::LangId;
use featherkey_neural_lm::NextWordLm;
use featherkey_neural_ranker::NeuralRanker;
use featherkey_neural_tap::TapWarp;
use featherkey_personalization::Personalization;

use crate::correct::LastCorrection;
use crate::packs::{build_packs, primary_tag, Pack};
use crate::rank::PRIOR_COEFFS;
use crate::rank_features::RankSnapshot;
use crate::recent::RecentWords;
use featherkey_prediction::StatisticalPredictor;
use featherkey_sensitive_context::SensitivityPolicy;
use featherkey_tap_sequence::{TapDistribution, TapSequence};
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
    /// Per-user coordinate warp applied to a touch before decode (cold-start
    /// prior, near-zero shift). See [`Self::decode`].
    tap_warp: TapWarp,
    personalization: Personalization,
    /// On-device next-word (bigram) model, persisted under `PersonalLm`.
    context: Context,
    /// On-device correction-signal model, persisted under `Corrections`.
    corrections: Corrections,
    /// The tiny neural re-ranker, initialised to the cold-start prior
    /// ([`PRIOR_COEFFS`]) and persisted under `RankerModel`. Held here so it
    /// survives language switches; scores the suggestion strip in
    /// [`rank_suggestions`](Self::rank_suggestions).
    neural_ranker: NeuralRanker,
    /// The tiny per-user autocorrect gate (cold-start prior, persisted under
    /// `AutocorrectGate`); `pub(crate)` so the correction use-case reaches it.
    pub(crate) autocorrect_gate: AutocorrectGate,
    /// The most recent ranked query's shown set (words + the features that ranked
    /// them), bounded to one snapshot. Written by every `rank_suggestions`; read
    /// by the pairwise trainer (reinforce-from-pick) in `learn.rs`.
    last_ranked: Option<RankSnapshot>,
    /// The most recent gated correction decision, read by the gate trainer.
    last_correction: Option<LastCorrection>,
    /// Active languages, each with its validated lexicon, in preference order.
    packs: Vec<Pack>,
    sensitivity: SensitivityPolicy,
    /// Recency-weighted per-language weight tracking which active language the
    /// user is currently writing in; seeded on construction and re-seeded on
    /// every language switch.
    momentum: Momentum,
    /// The taps of the word in progress, as distributions rather than committed
    /// characters, so an early slip can still be reconsidered (BR-5/BR-6).
    /// Transient in-memory state: never persisted, no `Namespace` (BR-26).
    taps: TapSequence,
    /// The user's chosen Latin arrangement, or `None` for the per-language
    /// default ("Auto"). Held across language switches so a switch never drops
    /// the choice (design §4.2). Latin-only: non-Latin scripts ignore it.
    latin_override: Option<LatinLayout>,
    /// The tiny on-device next-word embedding model, cold-started and
    /// persisted under `PersonalLm` (alongside `context`'s bigram model, under
    /// a distinct key). Not yet trained or consulted for ranking (Tasks 5/7);
    /// held here so it survives language switches like every other learned
    /// model.
    lm: NextWordLm,
    /// Ephemeral 2-word context buffer for the LM (never persisted, no
    /// `Namespace` — matches `taps`). Not yet wired into `push`/
    /// `two_word_context` call sites (Task 5/7).
    recent: RecentWords,
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
            layout: Layout::alpha_for(&primary, None),
            decoder: NearestKeyDecoder::new(),
            touch_model: TouchModel::default(),
            tap_warp: TapWarp::from_prior(),
            personalization: Personalization::new(),
            context: Context::new(),
            corrections: Corrections::new(),
            neural_ranker: NeuralRanker::from_prior(&PRIOR_COEFFS),
            autocorrect_gate: AutocorrectGate::from_prior(),
            last_ranked: None,
            last_correction: None,
            packs,
            sensitivity: SensitivityPolicy::new(),
            momentum: Momentum::new(&primary, &tags),
            taps: TapSequence::new(),
            latin_override: None,
            lm: NextWordLm::new(),
            recent: RecentWords::new(),
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
        self.layout = Layout::alpha_for(&primary, self.latin_override);
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

    /// Choose the Latin key arrangement (`None` = "Auto", the per-language
    /// default). Re-derives the live page immediately **only if** it is the alpha
    /// page, so the change shows without a language switch while a numeric/symbol
    /// page in progress is left alone (design §4.2).
    pub fn set_latin_layout(&mut self, layout: Option<LatinLayout>) {
        self.latin_override = layout;
        if self.layout.kind() == LayoutKind::Alpha {
            self.layout = Layout::alpha_for(&primary_tag(&self.packs), self.latin_override);
        }
    }

    /// Switch back to the alpha letter page for the primary active language.
    pub fn use_alpha_layout(&mut self) {
        self.layout = Layout::alpha_for(&primary_tag(&self.packs), self.latin_override);
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
    pub fn decode(&mut self, x: f32, y: f32) -> Result<DecodeResult, FeatherKeyError> {
        let (nx, ny) = self.layout.normalize(x, y);
        let (wdx, wdy) = self.tap_warp.warp(nx, ny);
        let touch = TouchPoint::new(x + wdx, y + wdy);
        let candidates = self
            .decoder
            .decode(touch, &self.layout, &self.touch_model)?;
        // Keep the tap as a distribution, not just its winner: that is what lets
        // a slip on this key be reconsidered once the rest of the word arrives
        // (BR-5/BR-6). Bounded and preallocated, so the hot path never grows the
        // buffer (BR-46).
        self.taps.push(TapDistribution::from_ranked(
            candidates
                .ranked()
                .iter()
                .map(|(id, conf)| (id.ch(), conf.value())),
        ));
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

    /// The held neural re-ranker. Test-only accessor: production ranking and
    /// persistence reach the ranker through the `neural_ranker` field directly.
    #[cfg(test)]
    pub(crate) fn neural_ranker(&self) -> &NeuralRanker {
        &self.neural_ranker
    }

    /// The held tap-warp. Test-only accessor for decode-path probes.
    #[cfg(test)]
    pub(crate) fn tap_warp(&self) -> &TapWarp {
        &self.tap_warp
    }

    /// The held next-word LM. Test-only accessor for persist/restore probes.
    #[cfg(test)]
    pub(crate) fn lm(&self) -> &NextWordLm {
        &self.lm
    }

    /// Mutable access to the held next-word LM. Test-only seam: `learn_word`
    /// does not yet drive `NextWordLm::observe` on the commit path (that is
    /// Task 7), so a test that needs a *warm* LM to exercise the `lm_logprob`
    /// re-ranker feature trains it directly through this accessor instead of
    /// waiting on the not-yet-built wiring.
    #[cfg(test)]
    pub(crate) fn lm_mut(&mut self) -> &mut NextWordLm {
        &mut self.lm
    }

    /// Mutable access to the ephemeral 2-word context buffer. Test-only seam:
    /// `learn_word` does not yet call `RecentWords::push` on the commit path
    /// (Task 7), so a test that needs the buffer positioned at a specific
    /// 2-word boundary drives it directly through this accessor.
    #[cfg(test)]
    pub(crate) fn recent_mut(&mut self) -> &mut RecentWords {
        &mut self.recent
    }

    /// The active layout. Test-only accessor for decode-path probes.
    #[cfg(test)]
    pub(crate) fn layout(&self) -> &Layout {
        &self.layout
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
mod tests;
