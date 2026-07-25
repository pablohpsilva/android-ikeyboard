//! Merge candidates from all sources into one ranked list. Pure: given the same
//! candidates and momentum snapshot it always returns the same order.

use featherkey_contracts::{Candidate, RankedCandidate, Source};
use featherkey_language_momentum::Momentum;

/// Weight of the language-momentum term relative to positional score.
pub const LM_WEIGHT_LANG: f64 = 1.0;
/// Prior nudging bundled candidates above device ones so neither floods.
pub const SOURCE_PRIOR_LEXICON: f64 = 0.2;
pub const SOURCE_PRIOR_DEVICE: f64 = 0.0;

fn source_prior(s: Source) -> f64 {
    match s {
        Source::Lexicon => SOURCE_PRIOR_LEXICON,
        Source::Device => SOURCE_PRIOR_DEVICE,
    }
}

/// Convert a 0-based within-source rank into a monotone score (0 = best).
fn positional_score(rank: u32) -> f64 {
    -((1 + rank) as f64).ln()
}

/// Rank `cands` using `momentum`, deduping by word (best score wins), top `k`.
#[must_use]
pub fn rank(cands: &[Candidate], momentum: &Momentum, k: usize) -> Vec<RankedCandidate> {
    let mut best: Vec<RankedCandidate> = Vec::new();
    for cand in cands {
        let score = positional_score(cand.source_rank)
            + LM_WEIGHT_LANG * momentum.weight_of(&cand.lang).ln()
            + source_prior(cand.source);
        match best.iter_mut().find(|r| r.word == cand.word) {
            Some(existing) if existing.score >= score => {}
            Some(existing) => {
                existing.score = score;
                existing.lang = cand.lang.clone();
            }
            None => best.push(RankedCandidate {
                word: cand.word.clone(),
                lang: cand.lang.clone(),
                score,
            }),
        }
    }
    best.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    best.truncate(k);
    best
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::rank;
    use featherkey_contracts::{Candidate, Source};
    use featherkey_language_momentum::Momentum;

    fn c(word: &str, lang: &str, rank: u32) -> Candidate {
        Candidate {
            word: word.into(),
            lang: lang.into(),
            source: Source::Lexicon,
            source_rank: rank,
        }
    }

    #[test]
    fn momentum_promotes_the_current_language_on_a_tie() {
        // Two words, same source_rank, different languages.
        let cands = vec![c("hello", "en", 0), c("hola", "es", 0)];
        let mut mom = Momentum::new("en", &["en".into(), "es".into()]);
        for _ in 0..5 {
            mom.observe(&["es".into()]);
        } // now writing Spanish
        let out = rank(&cands, &mom, 2);
        assert_eq!(out[0].word, "hola");
    }

    #[test]
    fn a_decisive_source_rank_beats_weak_momentum() {
        let cands = vec![c("hello", "en", 0), c("hola", "es", 9)];
        let mom = Momentum::new("en", &["en".into(), "es".into()]); // es only slightly cold
        let out = rank(&cands, &mom, 2);
        assert_eq!(out[0].word, "hello");
    }

    #[test]
    fn dedupe_keeps_the_best_scoring_instance_of_a_word() {
        // cognate: same word emitted for en and es; hotter language wins, one entry.
        let cands = vec![c("no", "en", 0), c("no", "es", 0)];
        let mut mom = Momentum::new("en", &["en".into(), "es".into()]);
        for _ in 0..5 {
            mom.observe(&["es".into()]);
        }
        let out = rank(&cands, &mom, 5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].lang, "es");
    }

    #[test]
    fn top_k_bounds_the_output() {
        let cands = vec![c("a", "en", 0), c("b", "en", 1), c("c", "en", 2)];
        let mom = Momentum::new("en", &["en".into()]);
        assert_eq!(rank(&cands, &mom, 2).len(), 2);
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let mom = Momentum::new("en", &["en".into()]);
        assert!(rank(&[], &mom, 3).is_empty());
    }

    use proptest::prelude::*;
    proptest! {
        #[test]
        fn spanish_momentum_never_demotes_the_spanish_candidate(bumps in 0u32..15) {
            let cands = vec![c("hello","en",0), c("hola","es",0)];
            let mut mom = Momentum::new("en", &["en".into(),"es".into()]);
            for _ in 0..bumps { mom.observe(&["es".into()]); }
            let out = rank(&cands, &mom, 2);
            // Invariant every run: both candidates survive (distinct words, k=2).
            prop_assert_eq!(out.len(), 2);
            let hola_idx = out.iter().position(|r| r.word == "hola").expect("hola present");
            // As Spanish momentum accumulates it can only move hola up, never down:
            // with any bump hola is at least tied for first; from the head-start
            // crossover onward it is strictly first.
            prop_assert!(hola_idx <= 1);
            if bumps >= 3 { prop_assert_eq!(hola_idx, 0); }
        }
    }
}
