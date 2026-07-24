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

use featherkey_contracts::{Predictor, Suggestion, Suggestions, TypingContext};
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
    /// The active-language lexicons this predictor draws completions from. An
    /// empty set is valid — it simply yields no completions.
    lexicons: Vec<Dictionary>,
}

impl StatisticalPredictor {
    /// Build a predictor over the given active-language lexicons.
    ///
    /// Ownership is taken because a [`Dictionary`] is a read-only value the
    /// predictor holds for its lifetime. Passing an empty vector is legal and
    /// produces a predictor that always returns no suggestions.
    #[must_use]
    pub fn new(lexicons: Vec<Dictionary>) -> Self {
        Self { lexicons }
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
        for lexicon in &self.lexicons {
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
}
