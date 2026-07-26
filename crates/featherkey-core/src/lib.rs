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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use featherkey_context::Context;
use featherkey_contracts::{Predictor, Source};
use featherkey_corrections::Corrections;
use featherkey_dictionary::Dictionary;
use featherkey_input_decoder::{InputDecoder, NearestKeyDecoder};
use featherkey_kernel::TouchPoint;
use featherkey_language_momentum::Momentum;
use featherkey_locale_manager::{LangId, LocaleManager};
use featherkey_personalization::Personalization;
use featherkey_prediction::{StatisticalPredictor, MAX_SUGGESTIONS};
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

/// One active language: its tag, its validated (byte-sorted) lexicon, and the
/// bundled per-word frequency **rank** recovered from the activation order.
///
/// The [`Dictionary`] is a byte-sorted `fst` with no frequency of its own, so the
/// commonness a word carried in the shipped list would be lost the moment it is
/// sorted. [`build_packs`] therefore records each word's *input position* — the
/// shell activates languages in frequency order, most-common first — as its
/// `rank` (`0` = commonest) **before** sorting for the `fst`. The predictor
/// consumes this as `dict_rank` so common words still rank ahead of rare ones
/// (DECISION option A: carry frequency into the Rust core).
#[derive(Debug, Clone)]
struct Pack {
    lang: LangId,
    dict: Dictionary,
    /// `word -> rank` (`0` = commonest). A word absent here sorts last.
    rank: HashMap<String, u32>,
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

    /// The whole suggestion-strip blend, core-owned (ARCH §9.1 `Suggest`,
    /// option **b**): predictor completions + shell-gathered `device` candidates
    /// → language-momentum ranking → dictionary fold-group variant guarantee.
    /// Read-only — never mutates learned state. The shell just renders the words.
    ///
    /// Ordering within a language is context → learned → bundled rank (via the
    /// ranked predictor); across languages it is the momentum-weighted
    /// [`candidate_ranker`](featherkey_candidate_ranker). Finally the accent/
    /// apostrophe variant of the typed token is guaranteed a slot so a commoner
    /// plain twin (`hell`) cannot crowd out `he'll` — derived from the shipped
    /// lexicons' fold index, never a hand-authored replacement table.
    ///
    /// # Speed (BR-46 / plan Global Constraint)
    /// The learned `freq`/`dict_rank` snapshots handed to the predictor are
    /// **scoped to just this query's completions**, so no whole-vocabulary map is
    /// cloned per keystroke. (The lexicons themselves are cloned into the
    /// predictor exactly as the legacy [`suggest`](Self::suggest) already does;
    /// materialising them is the deferred W4 follow-up.)
    #[must_use]
    pub fn rank_suggestions(
        &self,
        preceding: &str,
        prefix: &str,
        device: Vec<Candidate>,
    ) -> Vec<RankedCandidate> {
        let context = self.context.next_counts(preceding);
        let (freq, dict_rank) = self.scoped_learned_snapshots(prefix);
        let lang_lexicons: Vec<(String, Dictionary)> = self
            .packs
            .iter()
            .map(|p| (p.lang.as_str().to_owned(), p.dict.clone()))
            .collect();
        let predictor = StatisticalPredictor::new_ranked(lang_lexicons, &freq, &dict_rank, &context);
        let mut cands = predictor.suggest_ranked(&TypingContext {
            preceding: preceding.to_owned(),
            prefix: prefix.to_owned(),
        });
        cands.extend(device);
        let ranked = featherkey_candidate_ranker::rank(&cands, &self.momentum, MAX_SUGGESTIONS);
        self.guarantee_fold_variant(prefix, ranked)
    }

    /// The learned `freq` and bundled `dict_rank` snapshots the ranked predictor
    /// needs — restricted to the words that `prefix` actually completes to, so a
    /// keystroke never clones the whole learned/bundled vocabulary. An empty
    /// prefix completes to nothing here (the predictor's empty-prefix branch uses
    /// only `context`), so both maps are empty.
    fn scoped_learned_snapshots(
        &self,
        prefix: &str,
    ) -> (BTreeMap<String, u32>, BTreeMap<String, u32>) {
        if prefix.is_empty() {
            return (BTreeMap::new(), BTreeMap::new());
        }
        let folded = featherkey_fold::fold(prefix);
        let mut words: BTreeSet<String> = BTreeSet::new();
        for p in &self.packs {
            for w in p.dict.fold_prefix(&folded) {
                words.insert(w);
            }
        }
        let mut freq = BTreeMap::new();
        let mut dict_rank = BTreeMap::new();
        for w in &words {
            let f = self.personalization.frequency(w);
            if f > 0 {
                freq.insert(w.clone(), f);
            }
            if let Some(r) = self.packs.iter().filter_map(|p| p.rank.get(w).copied()).min() {
                dict_rank.insert(w.clone(), r);
            }
        }
        (freq, dict_rank)
    }

    /// Guarantee the typed token's accent/apostrophe variant a strip slot, exactly
    /// as the Kotlin `SuggestionStrip.withGuaranteedVariant` did — moved core-side
    /// (plan W5 Step 1). The **device**-derived variant stays a thin Kotlin
    /// post-step; this covers the shipped-lexicon fold group only.
    fn guarantee_fold_variant(
        &self,
        prefix: &str,
        ranked: Vec<RankedCandidate>,
    ) -> Vec<RankedCandidate> {
        if prefix.is_empty() {
            return dedup_cap(ranked, MAX_SUGGESTIONS);
        }
        let shown: HashSet<String> = ranked.iter().map(|r| r.word.to_lowercase()).collect();
        let variant = self
            .accent_variants(prefix)
            .into_iter()
            .find(|v| !shown.contains(&v.word.to_lowercase()));
        let Some(variant) = variant else {
            return dedup_cap(ranked, MAX_SUGGESTIONS);
        };
        let mut out = ranked;
        let at = std::cmp::min(1, out.len());
        out.insert(at, variant);
        dedup_cap(out, MAX_SUGGESTIONS)
    }

    /// Real dictionary words in `prefix`'s **exact** accent-fold group whose
    /// spelling differs from what was typed (`ive → I've`, `voce → você`,
    /// `hell → he'll`, `tambem → também`), best-ranked (commonest) first. Derived
    /// purely from the shipped lexicons via the fold index — the Rust twin of
    /// `Vocabulary.accentVariantsOf`.
    fn accent_variants(&self, prefix: &str) -> Vec<RankedCandidate> {
        let folded = featherkey_fold::fold(prefix);
        let lower_prefix = prefix.to_lowercase();
        let mut seen: HashSet<String> = HashSet::new();
        let mut hits: Vec<(String, String, u32)> = Vec::new();
        for p in &self.packs {
            for w in p.dict.fold_prefix(&folded) {
                // fold_prefix returns prefix matches; keep only the *exact* group.
                if featherkey_fold::fold(&w) != folded || w.to_lowercase() == lower_prefix {
                    continue;
                }
                if !seen.insert(w.to_lowercase()) {
                    continue;
                }
                let rank = self
                    .packs
                    .iter()
                    .filter_map(|q| q.rank.get(&w).copied())
                    .min()
                    .unwrap_or(u32::MAX);
                hits.push((w, p.lang.as_str().to_owned(), rank));
            }
        }
        hits.sort_by_key(|(_, _, rank)| *rank); // most frequent first
        hits.into_iter()
            .map(|(word, lang, _)| {
                let score = featherkey_candidate_ranker::score(
                    &Candidate {
                        word: word.clone(),
                        lang: lang.clone(),
                        source: Source::Lexicon,
                        source_rank: 0,
                    },
                    &self.momentum,
                );
                RankedCandidate { word, lang, score }
            })
            .collect()
    }
}

/// De-duplicate `words` by lowercased spelling (first occurrence wins, preserving
/// order) and cap to `cap`. Mirrors the Kotlin `SuggestionStrip.dedupCap`.
fn dedup_cap(words: Vec<RankedCandidate>, cap: usize) -> Vec<RankedCandidate> {
    let mut seen: HashSet<String> = HashSet::new();
    words
        .into_iter()
        .filter(|w| seen.insert(w.word.to_lowercase()))
        .take(cap)
        .collect()
}

/// Validate a `(tag, words)` language list into lexicon packs, recording each
/// word's bundled frequency **rank** from the activation order. Shared by
/// construction and language switching so both apply the identical contract.
///
/// `words` arrive in **frequency order** (most-common first — the shell's asset
/// order; DECISION option A). Each word's input position becomes its `rank`
/// (`0` = commonest) *before* the list is byte-sorted for the `fst`, so the
/// bundled commonness survives sorting. A repeated word keeps its earliest (most
/// frequent) position. The set is still validated as non-empty with no duplicate
/// tag; ordering is no longer a rejection reason (the core sorts internally).
fn build_packs(languages: Vec<(String, Vec<String>)>) -> Result<Vec<Pack>, FeatherKeyError> {
    let mut packs = Vec::with_capacity(languages.len());
    for (tag, words) in languages {
        let mut rank: HashMap<String, u32> = HashMap::with_capacity(words.len());
        for (position, word) in words.iter().enumerate() {
            // First (most-frequent) occurrence wins; `position` never exceeds the
            // input length, far below `u32::MAX`.
            rank.entry(word.clone()).or_insert(position as u32);
        }
        // The `fst` needs non-decreasing byte order; sort a copy of the
        // frequency-ordered input (adjacent duplicates are merged by the
        // dictionary itself).
        let mut sorted = words;
        sorted.sort();
        let dict = Dictionary::from_sorted_words(sorted)?;
        packs.push(Pack {
            lang: LangId::new(tag),
            dict,
            rank,
        });
    }
    // Build a real LocaleManager purely to validate the set — it rejects an
    // empty set (→ NoLanguages) and a duplicate tag (→ Locale). Discarded;
    // `correct` rebuilds one on demand.
    let locale_pairs: Vec<(LangId, Dictionary)> = packs
        .iter()
        .map(|p| (p.lang.clone(), p.dict.clone()))
        .collect();
    LocaleManager::new(locale_pairs)?;
    Ok(packs)
}

/// The primary (first) active language tag — the one whose script drives the
/// alpha page. Falls back to `"en"` (QWERTY) when the set is empty, which the
/// public API never produces (`build_packs` rejects an empty set).
fn primary_tag(packs: &[Pack]) -> String {
    packs
        .first()
        .map_or_else(|| "en".to_owned(), |p| p.lang.as_str().to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn primary_tag_falls_back_to_en_on_an_empty_set() {
        assert_eq!(primary_tag(&[]), "en");
    }

    #[test]
    fn primary_tag_is_the_first_pack_in_preference_order() {
        let core = FeatherKeyCore::new(vec![
            ("ru".to_owned(), vec!["да".to_owned()]),
            ("en".to_owned(), vec!["cat".to_owned()]),
        ])
        .expect("valid core");
        assert_eq!(primary_tag(&core.packs), "ru");
    }

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

    // ---- Wave 4: frequency-carry, rank_suggestions, gated hooks ---------------

    fn words_of(ranked: &[RankedCandidate]) -> Vec<&str> {
        ranked.iter().map(|r| r.word.as_str()).collect()
    }

    #[test]
    fn build_packs_records_frequency_rank_from_input_position() {
        // Words arrive in frequency order; rank = input position even though the
        // fst stores them alphabetically (aardvark < cat < the).
        let core = FeatherKeyCore::new(vec![(
            "en".into(),
            vec!["the".into(), "cat".into(), "aardvark".into()],
        )])
        .expect("core");
        let rank = &core.packs[0].rank;
        assert_eq!(rank.get("the"), Some(&0));
        assert_eq!(rank.get("cat"), Some(&1));
        assert_eq!(rank.get("aardvark"), Some(&2));
    }

    #[test]
    fn rank_suggestions_orders_by_bundled_rank_when_nothing_learned() {
        // No context, no learned usage: the commoner bundled word (lower rank,
        // earlier in the frequency-ordered input) wins. Proves dict_rank flows.
        let core = FeatherKeyCore::new(vec![(
            "en".into(),
            vec!["cat".into(), "car".into(), "can".into()],
        )])
        .expect("core");
        let out = core.rank_suggestions("", "ca", vec![]);
        assert_eq!(words_of(&out), ["cat", "car", "can"]);
    }

    #[test]
    fn rank_suggestions_lets_context_beat_bundled_rank() {
        // "car" is commoner (rank 0) than "cat" (rank 1), but the bigram context
        // after "the" favours "cat", which must then win. Proves context flows.
        let mut core =
            FeatherKeyCore::new(vec![("en".into(), vec!["car".into(), "cat".into()])]).expect("core");
        core.import_context([("the".to_string(), "cat".to_string(), 3)]);
        let out = core.rank_suggestions("the", "ca", vec![]);
        assert_eq!(out[0].word, "cat");
    }

    #[test]
    fn rank_suggestions_tags_completion_with_its_pack_language() {
        // A completion drawn from the es pack keeps its language across the blend.
        let core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["cat".into()]),
            ("es".into(), vec!["gato".into()]),
        ])
        .expect("core");
        let out = core.rank_suggestions("", "ga", vec![]);
        assert_eq!(out[0].word, "gato");
        assert_eq!(out[0].lang, "es");
    }

    #[test]
    fn rank_suggestions_surfaces_the_apostrophe_variant_of_the_typed_token() {
        // Typing "hell" must still offer "he'll" — derived from the fold group,
        // never a hand-authored table.
        let core = FeatherKeyCore::new(vec![(
            "en".into(),
            vec!["hell".into(), "hello".into(), "he'll".into()],
        )])
        .expect("core");
        let out = core.rank_suggestions("", "hell", vec![]);
        assert!(
            out.iter().any(|r| r.word == "he'll"),
            "he'll not offered: {:?}",
            words_of(&out)
        );
    }

    #[test]
    fn accent_variants_are_the_exact_fold_group_minus_the_typed_word() {
        // "hell" folds to itself; its exact fold group is {hell, he'll}. The
        // typed word is excluded and "hello" (different fold) is not a member.
        let core = FeatherKeyCore::new(vec![(
            "en".into(),
            vec!["hell".into(), "hello".into(), "he'll".into()],
        )])
        .expect("core");
        let variants: Vec<String> = core
            .accent_variants("hell")
            .into_iter()
            .map(|r| r.word)
            .collect();
        assert_eq!(variants, vec!["he'll".to_string()]);
    }

    #[test]
    fn accent_variants_rank_by_minimum_across_all_active_packs() {
        // Regression pin (r-u-sure round 1): a variant shared across languages
        // with crossed frequency ranks must sort by the MINIMUM rank across packs
        // (Kotlin Vocabulary.rankOf), not the first pack's rank. Here "café" is
        // rare in en (position 2) but commonest in es (position 0), while "cafè"
        // is position 1 in en only. Min ranks: café=0, cafè=1 -> café first. The
        // old first-pack-only lookup would have ranked cafè (en pos 1) ahead.
        let core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["the".into(), "cafè".into(), "café".into()]),
            ("es".into(), vec!["café".into(), "and".into()]),
        ])
        .expect("core");
        let variants: Vec<String> = core
            .accent_variants("cafe")
            .into_iter()
            .map(|r| r.word)
            .collect();
        assert_eq!(variants, vec!["café".to_string(), "cafè".to_string()]);
    }

    #[test]
    fn guarantee_fold_variant_inserts_an_unshown_variant_at_slot_two() {
        // With only the plain twin ranked, the guarantee splices the accented
        // form into the second slot (index 1), mirroring the Kotlin behaviour.
        let core = FeatherKeyCore::new(vec![(
            "en".into(),
            vec!["hell".into(), "he'll".into()],
        )])
        .expect("core");
        let ranked = vec![RankedCandidate {
            word: "hell".into(),
            lang: "en".into(),
            score: 0.0,
        }];
        let out = core.guarantee_fold_variant("hell", ranked);
        assert_eq!(words_of(&out), ["hell", "he'll"]);
    }

    #[test]
    fn rank_suggestions_appends_device_candidates_under_momentum() {
        // Device candidates blend in; strong es momentum promotes the es word
        // over an equally-ranked en one — proving language survives the blend.
        use featherkey_contracts::{Candidate, Source};
        let mut core = FeatherKeyCore::new(vec![
            ("en".into(), vec!["hello".into()]),
            ("es".into(), vec!["hola".into()]),
        ])
        .expect("core");
        for _ in 0..5 {
            core.observe_language(vec!["es".into()]);
        }
        let device = vec![
            Candidate {
                word: "hello".into(),
                lang: "en".into(),
                source: Source::Device,
                source_rank: 0,
            },
            Candidate {
                word: "hola".into(),
                lang: "es".into(),
                source: Source::Device,
                source_rank: 0,
            },
        ];
        let out = core.rank_suggestions("", "", device);
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
        let mut core =
            FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
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
        let mut core =
            FeatherKeyCore::new(vec![("en".into(), vec!["teh".into()])]).expect("core");
        core.observe_strip_pick("teh", "teh", &Ordinary);
        core.observe_delete_retype("ducking", &Ordinary);
        assert_eq!(core.correction_pref_count("teh", "teh"), 1);
        assert_eq!(core.correction_unwanted_count("ducking"), 1);
    }
}
