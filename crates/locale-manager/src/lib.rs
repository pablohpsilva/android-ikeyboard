//! Active languages, concurrent multi-language, per-word language detection.
//!
//! A [`LocaleManager`] holds an **ordered set of active languages** (≥2 at MVP,
//! architected toward 3 — SEDD §6.1), each paired with its
//! [`Dictionary`](featherkey_dictionary::Dictionary). It answers three
//! questions and carries no other policy (SEDD §5.2 single responsibility):
//!
//! * [`active`](LocaleManager::active) — which languages are on, in order
//!   (BR-16 concurrent, no manual toggle required).
//! * [`set_active`](LocaleManager::set_active) — swap the active set instantly
//!   (BR-17 — all lexicons are already loaded, so the switch is a pointer move,
//!   never a reload).
//! * [`detect`](LocaleManager::detect) — which active language a single word
//!   belongs to (BR-19b per-word auto-detection).
//!
//! Detection is the lightweight statistical scheme of **ADR-10**: the word is
//! scored against each active language's lexicon (ADR-13 — this crate reads
//! `dictionary`), and the highest scorer wins. A word present in the lexicon
//! outscores one that is merely a viable prefix, so a completed word pins its
//! language even when a longer language shares the same opening letters. Ties
//! resolve to the *first* (most-recently-chosen) active language — a one-bit
//! hysteresis so mixed input (BR-18) does not thrash between languages.
//!
//! Errors are values, never panics (SEDD §5.5 r3): construction and switching
//! return a [`Result`], and [`detect`](LocaleManager::detect) returns plain
//! data — `None` when no active language recognises the word.

use std::fmt;

use featherkey_dictionary::Dictionary;

/// The identity of an active language: an opaque language-tag string
/// (e.g. `"en"`, `"pt-BR"`).
///
/// Deliberately minimal — it is a stable key, not a locale database. It carries
/// no plurals, collation, or region policy; those belong to layers above this
/// crate. Two `LangId`s are equal exactly when their tags are byte-equal, so the
/// caller owns any normalisation (case, region) before constructing one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LangId(String);

impl LangId {
    /// Wrap a language tag. Accepts anything string-like so both `&str`
    /// literals and owned `String`s construct one without ceremony.
    #[must_use]
    pub fn new(tag: impl Into<String>) -> Self {
        Self(tag.into())
    }

    /// The underlying language tag.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LangId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a [`LocaleManager`] could not be built or reconfigured.
///
/// Detection never errors — only defining the active set can, and only when the
/// set violates the "non-empty ordered set of distinct languages" invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LocaleError {
    /// The active set was empty. A keyboard always has at least one language on;
    /// an empty set has nothing to type or detect against.
    NoActiveLanguages,
    /// The same [`LangId`] appeared twice. The active set is an *ordered set*:
    /// each language occurs at most once, so duplicates are rejected rather than
    /// silently collapsed (which would drop one caller's dictionary).
    DuplicateLanguage,
}

impl fmt::Display for LocaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocaleError::NoActiveLanguages => f.write_str("at least one language must be active"),
            LocaleError::DuplicateLanguage => {
                f.write_str("a language may appear in the active set at most once")
            }
        }
    }
}

/// Weight added to a language's score when the word is *exactly* in its lexicon.
///
/// It dominates any prefix-breadth contribution (bounded by
/// [`MAX_COMPLETIONS`](featherkey_dictionary::MAX_COMPLETIONS) = 16), so a
/// completed word always beats a mere prefix match in another language.
const CONTAINS_WEIGHT: u32 = 100;

/// An ordered set of active languages, each with its lexicon, plus per-word
/// language identification over them.
#[derive(Debug)]
pub struct LocaleManager {
    /// Active language ids in preference order — index 0 is the most-recently
    /// chosen (tie-break winner). Kept contiguous so [`active`](Self::active)
    /// can hand back a `&[LangId]` directly.
    ids: Vec<LangId>,
    /// Lexicons parallel to `ids` by index (`dicts[i]` is `ids[i]`'s).
    dicts: Vec<Dictionary>,
}

impl LocaleManager {
    /// Build a manager from an ordered active set. Index 0 is the preferred
    /// (most-recent) language for tie-breaking.
    ///
    /// # Errors
    /// [`LocaleError::NoActiveLanguages`] if `active` is empty;
    /// [`LocaleError::DuplicateLanguage`] if a [`LangId`] repeats.
    pub fn new(active: Vec<(LangId, Dictionary)>) -> Result<Self, LocaleError> {
        let (ids, dicts) = Self::validate(active)?;
        Ok(Self { ids, dicts })
    }

    /// Replace the active set instantly (BR-17). All languages are already
    /// resident, so this is a swap, not a reload — the previous set is dropped
    /// and the new order takes effect for the very next [`detect`](Self::detect).
    ///
    /// The manager is left untouched if the new set is invalid: validation runs
    /// to completion before anything is stored, so a rejected switch never
    /// leaves a half-applied state.
    ///
    /// # Errors
    /// Same conditions as [`new`](Self::new).
    pub fn set_active(&mut self, active: Vec<(LangId, Dictionary)>) -> Result<(), LocaleError> {
        let (ids, dicts) = Self::validate(active)?;
        self.ids = ids;
        self.dicts = dicts;
        Ok(())
    }

    /// The active languages, in preference order (BR-16). Index 0 is the
    /// most-recently chosen.
    #[must_use]
    pub fn active(&self) -> &[LangId] {
        &self.ids
    }

    /// The active language `word` most likely belongs to (BR-19b), or `None` if
    /// no active language recognises it.
    ///
    /// Scores `word` against each active language (ADR-10) and returns the
    /// highest scorer. A word in a language's lexicon outscores a bare prefix
    /// match; on a tie the earliest (most-recent) active language wins, giving
    /// the hysteresis that keeps mixed input from thrashing (BR-18). An empty
    /// word belongs to no language.
    #[must_use]
    pub fn detect(&self, word: &str) -> Option<LangId> {
        if word.is_empty() {
            return None;
        }
        let mut best: Option<(&LangId, u32)> = None;
        for (id, dict) in self.ids.iter().zip(self.dicts.iter()) {
            let score = Self::score(dict, word);
            if score == 0 {
                // This language does not recognise the word at all — neither as
                // a whole word nor as the start of one.
                continue;
            }
            // Strict `>` keeps the earliest language on a tie: since we walk in
            // preference order, the first to reach the maximum stays the winner.
            match best {
                Some((_, best_score)) if score <= best_score => {}
                _ => best = Some((id, score)),
            }
        }
        best.map(|(id, _)| id.clone())
    }

    /// Score one language's lexicon against `word` (ADR-10).
    ///
    /// An exact membership hit adds [`CONTAINS_WEIGHT`]; prefix breadth (how many
    /// words in this language begin with `word`, bounded by the dictionary's cap)
    /// adds a smaller graded signal so an in-progress token can still be placed.
    /// A score of `0` means "not recognised".
    fn score(dict: &Dictionary, word: &str) -> u32 {
        let mut score = 0;
        if dict.contains(word) {
            score += CONTAINS_WEIGHT;
        }
        // `prefix` is capped at MAX_COMPLETIONS, so this term is bounded and can
        // never overtake a CONTAINS_WEIGHT hit in another language.
        score += dict.prefix(word).len() as u32;
        score
    }

    /// Split an active set into parallel id/dictionary vectors, enforcing the
    /// non-empty, no-duplicate invariant before either is kept.
    fn validate(
        active: Vec<(LangId, Dictionary)>,
    ) -> Result<(Vec<LangId>, Vec<Dictionary>), LocaleError> {
        if active.is_empty() {
            return Err(LocaleError::NoActiveLanguages);
        }
        let mut ids: Vec<LangId> = Vec::with_capacity(active.len());
        let mut dicts: Vec<Dictionary> = Vec::with_capacity(active.len());
        for (id, dict) in active {
            if ids.contains(&id) {
                return Err(LocaleError::DuplicateLanguage);
            }
            ids.push(id);
            dicts.push(dict);
        }
        Ok((ids, dicts))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Build a dictionary from pre-sorted fixture words. `expect` is confined to
    /// tests, never library code (SEDD §5.5 r3).
    fn dict(words: &[&str]) -> Dictionary {
        Dictionary::from_sorted_words(words.iter().copied()).expect("fixture is sorted")
    }

    fn en(words: &[&str]) -> (LangId, Dictionary) {
        (LangId::new("en"), dict(words))
    }

    fn pt(words: &[&str]) -> (LangId, Dictionary) {
        (LangId::new("pt"), dict(words))
    }

    /// The standard concurrent pair: English then Portuguese, "hello" shared.
    fn en_then_pt() -> LocaleManager {
        LocaleManager::new(vec![en(&["hello", "world"]), pt(&["hello", "mundo"])])
            .expect("valid active set")
    }

    // --- LangId ---------------------------------------------------------------

    #[test]
    fn lang_id_exposes_its_tag_and_displays_it() {
        let id = LangId::new("pt-BR");
        assert_eq!(id.as_str(), "pt-BR");
        assert_eq!(format!("{id}"), "pt-BR");
        // Equality is by tag; a String and a &str construct the same id.
        assert_eq!(LangId::new(String::from("en")), LangId::new("en"));
        assert_ne!(LangId::new("en"), LangId::new("pt"));
    }

    // --- construction invariants ---------------------------------------------

    #[test]
    fn new_rejects_an_empty_active_set() {
        let err = LocaleManager::new(vec![]);
        assert_eq!(err.err(), Some(LocaleError::NoActiveLanguages));
    }

    #[test]
    fn new_rejects_a_duplicate_language() {
        let err = LocaleManager::new(vec![en(&["hello"]), en(&["world"])]);
        assert_eq!(err.err(), Some(LocaleError::DuplicateLanguage));
    }

    #[test]
    fn active_reports_two_concurrent_languages_in_order() {
        // BR-16: ≥2 languages on at once, no manual switch to reach either.
        let mgr = en_then_pt();
        let active: Vec<&str> = mgr.active().iter().map(LangId::as_str).collect();
        assert_eq!(active, ["en", "pt"]);
    }

    // --- detection: BR-19b ----------------------------------------------------

    #[test]
    fn word_in_exactly_one_language_detects_that_language() {
        let mgr = en_then_pt();
        // "world" is English-only; "mundo" is Portuguese-only.
        assert_eq!(mgr.detect("world"), Some(LangId::new("en")));
        assert_eq!(mgr.detect("mundo"), Some(LangId::new("pt")));
    }

    #[test]
    fn word_in_both_languages_resolves_to_the_hysteresis_winner() {
        // BR-18: "hello" is in both lexicons with equal prefix breadth, so the
        // scores tie and the first (most-recent) active language wins.
        let mgr = en_then_pt();
        assert_eq!(mgr.detect("hello"), Some(LangId::new("en")));
    }

    #[test]
    fn unknown_word_detects_no_language() {
        let mgr = en_then_pt();
        assert_eq!(mgr.detect("xyzzy"), None);
    }

    #[test]
    fn empty_word_detects_no_language() {
        // An empty prefix would match every lexicon; short-circuit to None so a
        // blank token never spuriously "belongs" to the first language.
        let mgr = en_then_pt();
        assert_eq!(mgr.detect(""), None);
    }

    #[test]
    fn an_in_progress_prefix_detects_via_prefix_breadth_alone() {
        // "wor" is in neither lexicon exactly, but only English has words
        // starting with it, so prefix breadth (not containment) decides.
        let mgr = en_then_pt();
        assert_eq!(mgr.detect("wor"), Some(LangId::new("en")));
    }

    #[test]
    fn an_exact_word_outranks_a_longer_prefix_match_in_another_language() {
        // English merely *starts* words with "tes" (breadth), while Portuguese
        // contains "tes" exactly. Containment must win despite English being the
        // preferred (first) language — proving score, not order, decides here.
        let mgr = LocaleManager::new(vec![en(&["test", "tester", "testing"]), pt(&["tes"])])
            .expect("valid active set");
        assert_eq!(mgr.detect("tes"), Some(LangId::new("pt")));
    }

    // --- switching: BR-17 -----------------------------------------------------

    #[test]
    fn set_active_switches_instantly_and_flips_the_hysteresis_winner() {
        let mut mgr = en_then_pt();
        assert_eq!(mgr.detect("hello"), Some(LangId::new("en")));

        // Instant switch to Portuguese-first: the tie for "hello" now resolves
        // the other way, with no reload step in between.
        mgr.set_active(vec![pt(&["hello", "mundo"]), en(&["hello", "world"])])
            .expect("valid active set");
        assert_eq!(mgr.active().first().map(LangId::as_str), Some("pt"));
        assert_eq!(mgr.detect("hello"), Some(LangId::new("pt")));
    }

    #[test]
    fn set_active_rejects_an_invalid_set_without_disturbing_the_current_one() {
        let mut mgr = en_then_pt();

        assert_eq!(
            mgr.set_active(vec![]).err(),
            Some(LocaleError::NoActiveLanguages)
        );
        assert_eq!(
            mgr.set_active(vec![en(&["a"]), en(&["b"])]).err(),
            Some(LocaleError::DuplicateLanguage)
        );

        // Both rejections left the original en-then-pt set fully intact.
        let active: Vec<&str> = mgr.active().iter().map(LangId::as_str).collect();
        assert_eq!(active, ["en", "pt"]);
        assert_eq!(mgr.detect("world"), Some(LangId::new("en")));
    }

    #[test]
    fn a_single_active_language_is_a_valid_set() {
        // ≥2 is the product target, but the domain type permits one; detection
        // then either names that language or returns None.
        let mgr = LocaleManager::new(vec![en(&["hello"])]).expect("valid");
        assert_eq!(mgr.detect("hello"), Some(LangId::new("en")));
        assert_eq!(mgr.detect("mundo"), None);
    }

    // --- error Display --------------------------------------------------------

    #[test]
    fn locale_error_displays_human_messages() {
        assert_eq!(
            format!("{}", LocaleError::NoActiveLanguages),
            "at least one language must be active"
        );
        assert_eq!(
            format!("{}", LocaleError::DuplicateLanguage),
            "a language may appear in the active set at most once"
        );
    }
}
