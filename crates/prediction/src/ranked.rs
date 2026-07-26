//! The enriched, language-tagged ranking path (`new_ranked` / `suggest_ranked`).
//!
//! [`StatisticalPredictor`](crate::StatisticalPredictor) has two faces. The
//! bare [`Predictor::suggest`](featherkey_contracts::Predictor::suggest) in
//! `lib.rs` is the length-scored prefix completion the walking skeleton shipped;
//! this module is the enriched one the composition root actually ranks the
//! suggestion strip with — it blends the bundled dictionary rank with the
//! per-user learned frequency and bigram context, completes accent-insensitively
//! through the fold index, and keeps each completion's language so momentum
//! weighting downstream still knows which pack it came from.
//!
//! Both faces share one struct so the snapshots live in a single place; only the
//! ordering rules differ, and they are what this file owns.

use std::collections::BTreeMap;

use featherkey_contracts::{Candidate, Source, TypingContext};
use featherkey_dictionary::Dictionary;

use crate::{StatisticalPredictor, MAX_SUGGESTIONS};

impl StatisticalPredictor {
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // The bare `suggest` path is asserted to stay unchanged by the empty-prefix
    // test below, so the trait it lives on is in scope here too.
    use featherkey_contracts::Predictor;

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
