//! Autocomplete and next-word prediction over the active-language lexicons.
//!
//! A [`StatisticalPredictor`] implements the driving
//! [`Predictor`](featherkey_contracts::Predictor) port (SEDD §5.4): given the
//! [`TypingContext`](featherkey_contracts::TypingContext) around the token the
//! user is typing, it returns ranked completions best-first (BR-10). It has a
//! single responsibility (SEDD §5.2) — *ranking* prefix completions — and reads
//! the pure lexical substrate below it (`dictionary`, ADR-13): each active
//! language contributes its [`Dictionary`](featherkey_dictionary::Dictionary)'s
//! `prefix` matches, which this crate merges, scores, and orders.
//!
//! The MVP engine is deliberately shallow (BR-10 "competitive", not "beats
//! iOS"): the neural engine lands in v1.x behind the **same** trait (ADR-3), so
//! callers never change when it is swapped. Two behaviours are therefore *not*
//! faked here — an empty prefix yields no completions (real next-word ranking is
//! v1.x), and `preceding` context is not yet consulted.
//!
//! Everything is deterministic, pure, and I/O-free: [`suggest`] is a total
//! function of the predictor's lexicons and the context (SEDD §5.5). There is no
//! failure path, so — unlike lexicon construction — nothing here returns a
//! `Result`.
//!
//! [`suggest`]: featherkey_contracts::Predictor::suggest

use std::collections::BTreeMap;

use featherkey_contracts::{Candidate, Predictor, Source, Suggestion, Suggestions, TypingContext};
use featherkey_dictionary::Dictionary;

/// The most suggestions [`StatisticalPredictor::suggest`] will return.
///
/// It mirrors the dictionary's own per-query cap
/// ([`MAX_COMPLETIONS`](featherkey_dictionary::MAX_COMPLETIONS)): the suggestion
/// strip can only surface a handful of words, so ranking more than the substrate
/// itself would ever yield is wasted work.
pub const MAX_SUGGESTIONS: usize = featherkey_dictionary::MAX_COMPLETIONS;

/// The score awarded to a completion that adds *no* characters — i.e. the prefix
/// is already a whole word. Every additional character the completion appends
/// beyond the prefix subtracts one, so shorter (closer) completions rank higher.
///
/// The base is large enough that realistic word lengths never underflow it; a
/// pathologically long word saturates to `0` rather than wrapping.
const EXACT_PREFIX_SCORE: u32 = 1000;

/// A statistical, prefix-completion predictor over a fixed set of active-language
/// lexicons.
///
/// Construct it with exactly the [`Dictionary`]s of the currently active
/// languages (the caller — the composition root — owns language activation via
/// `locale-manager`); the predictor then ranks completions drawn from all of
/// them together, so concurrent multilingual input (BR-16) is completed without
/// the caller choosing a language first.
#[derive(Debug)]
pub struct StatisticalPredictor {
    /// The active-language lexicons this predictor draws completions from,
    /// paired with the language tag they were activated under. An empty set is
    /// valid — it simply yields no completions.
    ///
    /// The bare [`new`](StatisticalPredictor::new) constructor tags every
    /// lexicon with the empty string (it never consults the tag), while
    /// [`new_ranked`](StatisticalPredictor::new_ranked) preserves each pack's
    /// real language so [`suggest_ranked`](StatisticalPredictor::suggest_ranked)
    /// can emit language-tagged [`Candidate`]s.
    lexicons: Vec<(String, Dictionary)>,
    /// Learned/personalisation word frequencies (higher = typed more often).
    /// Empty for a [`new`](StatisticalPredictor::new) predictor.
    freq: BTreeMap<String, u32>,
    /// Bundled per-word rank (`0` = commonest). A missing word sorts *last*.
    /// Empty for a [`new`](StatisticalPredictor::new) predictor.
    dict_rank: BTreeMap<String, u32>,
    /// Next-word counts for the token *preceding* the one being typed (a bigram
    /// snapshot, already resolved for the current `preceding` by the caller).
    /// Empty for a [`new`](StatisticalPredictor::new) predictor.
    context: BTreeMap<String, u32>,
}

impl StatisticalPredictor {
    /// Build a predictor over the given active-language lexicons.
    ///
    /// Ownership is taken because a [`Dictionary`] is a read-only value the
    /// predictor holds for its lifetime. Passing an empty vector is legal and
    /// produces a predictor that always returns no suggestions.
    ///
    /// This constructor drives only the legacy [`Predictor::suggest`] path: it
    /// tags every lexicon with the empty language (unused by `suggest`) and
    /// carries no frequency/rank/context data. Use
    /// [`new_ranked`](StatisticalPredictor::new_ranked) for the enriched,
    /// language-tagged ranking.
    #[must_use]
    pub fn new(lexicons: Vec<Dictionary>) -> Self {
        Self {
            lexicons: lexicons.into_iter().map(|d| (String::new(), d)).collect(),
            freq: BTreeMap::new(),
            dict_rank: BTreeMap::new(),
            context: BTreeMap::new(),
        }
    }

    /// Build a predictor that ranks completions the way the Kotlin
    /// `Vocabulary.candidatesByLanguage` does, preserving each completion's
    /// language for downstream source/momentum weighting.
    ///
    /// * `lang_lexicons` — the active `(language, lexicon)` packs, in priority
    ///   order; the first is the *primary* language used as a fallback tag.
    /// * `freq` — learned/personalisation counts (higher ranks earlier).
    /// * `dict_rank` — bundled per-word rank (`0` = commonest; a missing word
    ///   ranks last).
    /// * `context` — next-word counts for the preceding token (a bigram
    ///   snapshot; higher ranks earlier).
    ///
    /// The three snapshots are cloned so the predictor is self-contained for its
    /// lifetime. See [`suggest_ranked`](StatisticalPredictor::suggest_ranked)
    /// for the ordering.
    #[must_use]
    pub fn new_ranked(
        lang_lexicons: Vec<(String, Dictionary)>,
        freq: &BTreeMap<String, u32>,
        dict_rank: &BTreeMap<String, u32>,
        context: &BTreeMap<String, u32>,
    ) -> Self {
        Self {
            lexicons: lang_lexicons,
            freq: freq.clone(),
            dict_rank: dict_rank.clone(),
            context: context.clone(),
        }
    }

    /// The primary (first-activated) language, used as the fallback tag for a
    /// next-word that no active pack contains. Empty when there are no lexicons
    /// — safe, since an unknown language yields the momentum FLOOR downstream.
    fn primary_lang(&self) -> String {
        self.lexicons
            .first()
            .map(|(lang, _)| lang.clone())
            .unwrap_or_default()
    }

    /// Language-tagged, best-first candidates reproducing the Kotlin
    /// `Vocabulary.candidatesByLanguage` ordering.
    ///
    /// **Non-empty prefix:** gather each pack's accent-insensitive completions
    /// (`fold_prefix(&fold(prefix))`), merge them (a word shared by several
    /// packs keeps the first pack's language), and order by
    /// **context DESC → learned (`freq`) DESC → `dict_rank` ASC** (a word with no
    /// bundled rank sorts last). Ties keep the deterministic
    /// `(pack order, lexicographic)` order the merge produced.
    ///
    /// **Empty prefix:** a word boundary — emit the context snapshot's top
    /// next-words (count DESC, then lexicographic) instead. The bigram model is
    /// language-agnostic, so each next-word is tagged with the language of the
    /// first pack that [`contains`](Dictionary::contains) it, falling back to the
    /// [`primary`](StatisticalPredictor::primary_lang) language.
    ///
    /// Every candidate is a [`Source::Lexicon`] whose `source_rank` is its `0`-based
    /// position in the returned order. Output is capped at [`MAX_SUGGESTIONS`].
    #[must_use]
    pub fn suggest_ranked(&self, ctx: &TypingContext) -> Vec<Candidate> {
        if ctx.prefix.is_empty() {
            return self.empty_prefix_candidates();
        }

        // Gather completions across every pack, preserving language and
        // de-duplicating a word shared by several packs (first pack wins its
        // language, mirroring the empty-prefix `contains` first-match rule).
        // `fold_prefix` results are lexicographic, and packs are visited in
        // priority order, so insertion order is deterministic and forms the
        // stable tie-break below.
        let folded = featherkey_fold::fold(&ctx.prefix);
        let mut seen: BTreeMap<String, ()> = BTreeMap::new();
        let mut merged: Vec<(String, String)> = Vec::new();
        for (lang, dict) in &self.lexicons {
            for word in dict.fold_prefix(&folded) {
                if seen.insert(word.clone(), ()).is_none() {
                    merged.push((word, lang.clone()));
                }
            }
        }

        // context DESC → learned DESC → dict_rank ASC (unknown rank last).
        // A stable sort preserves the deterministic insertion order for full
        // ties. `u32::MAX` is the "no bundled rank" sentinel, sorting last.
        merged.sort_by_key(|(word, _)| {
            (
                std::cmp::Reverse(self.context.get(word).copied().unwrap_or(0)),
                std::cmp::Reverse(self.freq.get(word).copied().unwrap_or(0)),
                self.dict_rank.get(word).copied().unwrap_or(u32::MAX),
            )
        });

        Self::to_candidates(merged.into_iter())
    }

    /// The empty-prefix branch of [`suggest_ranked`](StatisticalPredictor::suggest_ranked):
    /// the context snapshot's top next-words, count DESC then lexicographic.
    fn empty_prefix_candidates(&self) -> Vec<Candidate> {
        let primary = self.primary_lang();
        // The `BTreeMap` yields words lexicographically; a stable sort by
        // descending count keeps that order for equal counts.
        let mut next: Vec<(String, u32)> =
            self.context.iter().map(|(w, c)| (w.clone(), *c)).collect();
        next.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

        let tagged = next.into_iter().map(|(word, _)| {
            let lang = self
                .lexicons
                .iter()
                .find(|(_, dict)| dict.contains(&word))
                .map_or_else(|| primary.clone(), |(lang, _)| lang.clone());
            (word, lang)
        });
        Self::to_candidates(tagged)
    }

    /// Turn an ordered `(word, lang)` stream into capped, position-ranked
    /// [`Source::Lexicon`] candidates.
    fn to_candidates(items: impl Iterator<Item = (String, String)>) -> Vec<Candidate> {
        items
            .take(MAX_SUGGESTIONS)
            .enumerate()
            .map(|(position, (word, lang))| Candidate {
                word,
                lang,
                source: Source::Lexicon,
                // `MAX_SUGGESTIONS` bounds `position` far below `u32::MAX`.
                source_rank: position as u32,
            })
            .collect()
    }

    /// Score a single completion by how much it *adds* to the prefix.
    ///
    /// `word` is guaranteed by [`Dictionary::prefix`] to start with the prefix,
    /// so its character count is never smaller; the extra characters are the
    /// user's remaining typing, and fewer of them means a likelier, higher-ranked
    /// completion. `saturating_sub` keeps the function total for any input.
    fn score(prefix_chars: usize, word: &str) -> u32 {
        let extra = word.chars().count().saturating_sub(prefix_chars);
        EXACT_PREFIX_SCORE.saturating_sub(extra as u32)
    }
}

impl Predictor for StatisticalPredictor {
    fn suggest(&self, ctx: &TypingContext) -> Suggestions {
        // An empty prefix is a word boundary: real next-word ranking is v1.x, so
        // we return nothing rather than dumping every word in the lexicons.
        if ctx.prefix.is_empty() {
            return Suggestions::default();
        }

        let prefix_chars = ctx.prefix.chars().count();

        // Merge completions across every active lexicon, de-duplicating a word
        // shared by several languages. The `BTreeMap` also fixes a deterministic
        // lexicographic order over the keys, which is the stable tie-break below
        // for completions of equal length (equal score).
        let mut ranked: BTreeMap<String, u32> = BTreeMap::new();
        for (_lang, lexicon) in &self.lexicons {
            for word in lexicon.prefix(&ctx.prefix) {
                let score = Self::score(prefix_chars, &word);
                ranked.entry(word).or_insert(score);
            }
        }

        let mut items: Vec<Suggestion> = ranked
            .into_iter()
            .map(|(word, score)| Suggestion { word, score })
            .collect();

        // Best-first by score. `sort_by` is stable, so equal-score completions
        // keep the lexicographic order the `BTreeMap` gave them — the ordering is
        // therefore fully determined by the lexicons alone.
        items.sort_by_key(|item| std::cmp::Reverse(item.score));
        items.truncate(MAX_SUGGESTIONS);

        Suggestions { items }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Build a lexicon from pre-sorted fixture words. `expect` is confined to
    /// tests, never library code (SEDD §5.5 r3).
    fn dict(words: &[&str]) -> Dictionary {
        Dictionary::from_sorted_words(words.iter().copied()).expect("fixture is sorted")
    }

    fn ctx(prefix: &str) -> TypingContext {
        TypingContext {
            preceding: String::new(),
            prefix: prefix.to_string(),
        }
    }

    /// Convenience: the ranked words, dropping scores, for order assertions.
    fn words(s: &Suggestions) -> Vec<&str> {
        s.items.iter().map(|i| i.word.as_str()).collect()
    }

    #[test]
    fn known_prefix_returns_completions_ranked_best_first() {
        // BR-10: completions are drawn from the lexicon and returned best-first.
        // "an" is itself a word (adds nothing) so it must outrank the longer
        // completions, which tie on length and fall back to lexicographic order.
        let p = StatisticalPredictor::new(vec![dict(&["an", "and", "ant"])]);
        let s = p.suggest(&ctx("an"));
        assert_eq!(words(&s), ["an", "and", "ant"]);
        // Exact-prefix word scores highest; the two length-3 words tie below it.
        assert_eq!(s.items[0].score, EXACT_PREFIX_SCORE);
        assert!(s.items[1].score < s.items[0].score);
        assert_eq!(s.items[1].score, s.items[2].score);
    }

    #[test]
    fn shorter_completions_outrank_longer_ones() {
        let p = StatisticalPredictor::new(vec![dict(&["app", "apple", "apply"])]);
        let s = p.suggest(&ctx("app"));
        // "app" (0 extra) beats the length-5 words (2 extra each).
        assert_eq!(words(&s), ["app", "apple", "apply"]);
        assert!(s.items[0].score > s.items[1].score);
    }

    #[test]
    fn empty_prefix_yields_no_completions() {
        // A word boundary: next-word ranking is v1.x and is not faked here.
        let p = StatisticalPredictor::new(vec![dict(&["a", "b", "c"])]);
        let s = p.suggest(&ctx(""));
        assert!(s.items.is_empty());
    }

    #[test]
    fn unknown_prefix_yields_empty_suggestions() {
        let p = StatisticalPredictor::new(vec![dict(&["cat", "dog"])]);
        assert!(p.suggest(&ctx("z")).items.is_empty());
    }

    #[test]
    fn a_predictor_with_no_lexicons_suggests_nothing() {
        let p = StatisticalPredictor::new(vec![]);
        assert!(p.suggest(&ctx("a")).items.is_empty());
    }

    #[test]
    fn completions_merge_across_active_languages_and_dedup_shared_words() {
        // BR-16: two active lexicons contribute together. "hello" is in both and
        // must appear exactly once; "help" (en) and "helado" (es) both surface.
        let en = dict(&["hello", "help"]);
        let es = dict(&["helado", "hello"]);
        let p = StatisticalPredictor::new(vec![en, es]);
        let s = p.suggest(&ctx("hel"));
        // Shortest completion first: help (4) > hello (5) > helado (6).
        assert_eq!(words(&s), ["help", "hello", "helado"]);
        // Exactly one "hello" despite being in both lexicons.
        assert_eq!(s.items.iter().filter(|i| i.word == "hello").count(), 1);
        // Strictly decreasing scores as each completion adds one more character.
        assert!(s.items[0].score > s.items[1].score);
        assert!(s.items[1].score > s.items[2].score);
    }

    #[test]
    fn output_is_capped_at_max_suggestions_keeping_the_highest_scored() {
        // One lexicon of short (high-score) completions, one of long (low-score)
        // ones, together exceeding the cap. The short words must survive.
        let short: Vec<String> = (0..MAX_SUGGESTIONS).map(|i| format!("pre{i:02}")).collect();
        let long: Vec<String> = (0..MAX_SUGGESTIONS)
            .map(|i| format!("preXXXXXX{i:02}"))
            .collect();
        let p = StatisticalPredictor::new(vec![
            Dictionary::from_sorted_words(short.iter()).expect("sorted"),
            Dictionary::from_sorted_words(long.iter()).expect("sorted"),
        ]);
        let s = p.suggest(&ctx("pre"));
        assert_eq!(s.items.len(), MAX_SUGGESTIONS);
        // Every survivor is a short (5-char) completion — none of the long ones
        // displaced a shorter, higher-scored word.
        assert!(s.items.iter().all(|i| i.word.chars().count() == 5));
    }

    #[test]
    fn score_saturates_and_never_panics_on_extreme_lengths() {
        // A completion far longer than the base score saturates to 0 rather than
        // underflowing — the function stays total (SEDD §5.5 r3).
        assert_eq!(StatisticalPredictor::score(0, "x"), EXACT_PREFIX_SCORE - 1);
        let very_long = "x".repeat((EXACT_PREFIX_SCORE as usize) + 10);
        assert_eq!(StatisticalPredictor::score(0, &very_long), 0);
    }

    #[test]
    fn predictor_is_debug() {
        // `missing_debug_implementations` is denied workspace-wide; prove it.
        let p = StatisticalPredictor::new(vec![dict(&["a"])]);
        assert!(!format!("{p:?}").is_empty());
    }

    // --- new_ranked / suggest_ranked (Task W3) ---------------------------------

    fn map(pairs: &[(&str, u32)]) -> BTreeMap<String, u32> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    /// The ranked candidate words, dropping tags, for order assertions.
    fn cwords(c: &[Candidate]) -> Vec<&str> {
        c.iter().map(|x| x.word.as_str()).collect()
    }

    #[test]
    fn ranks_context_then_learned_then_rank() {
        // Context precedence: "the" precedes, and the bigram snapshot favours
        // "cat" even though "car" is the commoner completion by bundled rank.
        let en = dict(&["car", "cat"]);
        let freq = BTreeMap::new();
        let dict_rank = map(&[("car", 0), ("cat", 5)]); // car is "commoner"
        let context = map(&[("cat", 1)]); // ...but context favours cat
        let p = StatisticalPredictor::new_ranked(
            vec![("en".to_string(), en)],
            &freq,
            &dict_rank,
            &context,
        );
        let c = p.suggest_ranked(&ctx("ca"));
        assert_eq!(cwords(&c), ["cat", "car"]);
        // Tagged with its pack's language and positioned by rank.
        assert_eq!(c[0].lang, "en");
        assert_eq!(c[0].source, Source::Lexicon);
        assert_eq!(c[0].source_rank, 0);
        assert_eq!(c[1].source_rank, 1);
    }

    #[test]
    fn learned_breaks_ties_when_context_is_equal() {
        // No context signal: learned (freq) DESC decides, above dict_rank.
        let en = dict(&["car", "cat"]);
        let freq = map(&[("cat", 10)]);
        let dict_rank = map(&[("car", 0), ("cat", 5)]);
        let context = BTreeMap::new();
        let p = StatisticalPredictor::new_ranked(
            vec![("en".to_string(), en)],
            &freq,
            &dict_rank,
            &context,
        );
        assert_eq!(cwords(&p.suggest_ranked(&ctx("ca"))), ["cat", "car"]);
    }

    #[test]
    fn dict_rank_breaks_ties_and_unknown_rank_sorts_last() {
        // No context, no learned: dict_rank ASC decides; the word with no
        // bundled rank sorts last.
        let en = dict(&["can", "car", "cat"]);
        let dict_rank = map(&[("cat", 0), ("car", 1)]); // "can" has no rank
        let p = StatisticalPredictor::new_ranked(
            vec![("en".to_string(), en)],
            &BTreeMap::new(),
            &dict_rank,
            &BTreeMap::new(),
        );
        assert_eq!(cwords(&p.suggest_ranked(&ctx("ca"))), ["cat", "car", "can"]);
    }

    #[test]
    fn accent_prefix_completes_via_fold() {
        // Accent-insensitive: the bare prefix "tambe" surfaces "também" via
        // fold_prefix(fold(prefix)) — an exact prefix would miss it.
        let pt = dict(&["tal", "também"]);
        let p = StatisticalPredictor::new_ranked(
            vec![("pt".to_string(), pt)],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        let c = p.suggest_ranked(&ctx("tambe"));
        assert_eq!(cwords(&c), ["também"]);
        assert_eq!(c[0].lang, "pt");
    }

    #[test]
    fn empty_prefix_returns_context_next_words() {
        // Was empty before (bare `suggest`); the ranked path emits the context
        // snapshot's top next-words, ordered by count DESC.
        let en = dict(&["cat", "dog"]);
        let context = map(&[("cat", 2), ("dog", 1)]);
        let p = StatisticalPredictor::new_ranked(
            vec![("en".to_string(), en)],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &context,
        );
        let c = p.suggest_ranked(&ctx(""));
        assert_eq!(cwords(&c), ["cat", "dog"]);
        // The bare `suggest` path is unchanged: empty prefix still yields nothing.
        assert!(p.suggest(&ctx("")).items.is_empty());
    }

    #[test]
    fn empty_prefix_tags_next_word_with_containing_pack_else_primary() {
        // "cat" lives in the en pack -> tagged en; "xyz" is in no pack -> falls
        // back to the primary (first) language.
        let en = dict(&["cat"]);
        let es = dict(&["gato"]);
        let context = map(&[("cat", 2), ("xyz", 1)]);
        let p = StatisticalPredictor::new_ranked(
            vec![("en".to_string(), en), ("es".to_string(), es)],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &context,
        );
        let c = p.suggest_ranked(&ctx(""));
        let cat = c.iter().find(|x| x.word == "cat").expect("cat present");
        let xyz = c.iter().find(|x| x.word == "xyz").expect("xyz present");
        assert_eq!(cat.lang, "en");
        assert_eq!(xyz.lang, "en"); // primary fallback
    }

    #[test]
    fn ranked_merges_across_packs_and_dedups_keeping_first_lang() {
        // A word shared by two packs keeps the first (priority) pack's language.
        let en = dict(&["hello"]);
        let es = dict(&["hello"]);
        let p = StatisticalPredictor::new_ranked(
            vec![("en".to_string(), en), ("es".to_string(), es)],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        let c = p.suggest_ranked(&ctx("hel"));
        assert_eq!(c.iter().filter(|x| x.word == "hello").count(), 1);
        assert_eq!(c[0].lang, "en");
    }

    #[test]
    fn ranked_candidates_are_debug() {
        let p = StatisticalPredictor::new_ranked(
            vec![("en".to_string(), dict(&["a"]))],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert!(!format!("{:?}", p.suggest_ranked(&ctx("a"))).is_empty());
    }
}
