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

/// The blended score the ranker assigns one candidate under `momentum`:
/// positional score + language-momentum term + per-source prior. Public so a
/// caller that must add its own bias (correction's sticky-fix bonus) shares the
/// exact scoring the strip uses.
#[must_use]
pub fn score(cand: &Candidate, momentum: &Momentum) -> f64 {
    positional_score(cand.source_rank)
        + LM_WEIGHT_LANG * momentum.weight_of(&cand.lang).ln()
        + source_prior(cand.source)
}

/// Rank `cands` using `momentum`, deduping by word (best score wins), top `k`.
#[must_use]
pub fn rank(cands: &[Candidate], momentum: &Momentum, k: usize) -> Vec<RankedCandidate> {
    rank_with_bias(cands, momentum, k, |_| 0.0)
}

/// Rank like [`rank`], but add a caller-supplied per-word `bias` to each
/// candidate's [`score`] before deduping, ordering, and truncation. The core
/// uses this to apply the correction "sticky-fix" bonus: a completion the user
/// has repeatedly picked for the current prefix is promoted (see
/// `observe_strip_pick`). Because the bias is added before the top-`k` cut, a
/// promoted candidate that would otherwise be dropped survives. `bias` returns
/// `0.0` for a word with no correction history, so [`rank`] is exactly
/// `rank_with_bias(cands, momentum, k, |_| 0.0)`.
#[must_use]
pub fn rank_with_bias(
    cands: &[Candidate],
    momentum: &Momentum,
    k: usize,
    bias: impl Fn(&str) -> f64,
) -> Vec<RankedCandidate> {
    let mut best: Vec<RankedCandidate> = Vec::new();
    for cand in cands {
        let score = score(cand, momentum) + bias(&cand.word);
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
    use super::{rank, score};
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
    fn score_matches_rank_ordering_for_a_single_candidate() {
        let mom = Momentum::new("en", &["en".into(), "es".into()]);
        let a = c("hello", "en", 0);
        let b = c("hola", "es", 3);
        assert!(score(&a, &mom) > score(&b, &mom)); // en primary + better rank
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let mom = Momentum::new("en", &["en".into()]);
        assert!(rank(&[], &mom, 3).is_empty());
    }

    #[test]
    fn a_device_sourced_candidate_is_ranked() {
        let mom = Momentum::new("en", &["en".into()]);
        let cands = vec![Candidate {
            word: "hello".into(),
            lang: "en".into(),
            source: Source::Device,
            source_rank: 0,
        }];
        let out = rank(&cands, &mom, 1);
        assert_eq!(out[0].word, "hello");
    }

    #[test]
    fn a_bias_promotes_a_lower_ranked_word() {
        use super::rank_with_bias;
        let mom = Momentum::new("en", &["en".into()]);
        let cands = vec![c("tea", "en", 0), c("team", "en", 1)];
        // No bias: the better source_rank ("tea") leads.
        assert_eq!(rank(&cands, &mom, 2)[0].word, "tea");
        // A sticky-fix bonus on "team" big enough to clear the rank0→rank1 gap
        // (ln 2 ≈ 0.69) promotes it to the front.
        let biased = rank_with_bias(&cands, &mom, 2, |w| if w == "team" { 1.0 } else { 0.0 });
        assert_eq!(biased[0].word, "team");
        assert_eq!(biased.len(), 2, "the demoted word is still present");
    }

    #[test]
    fn a_zero_bias_is_identical_to_rank() {
        use super::rank_with_bias;
        let mut mom = Momentum::new("en", &["en".into(), "es".into()]);
        for _ in 0..3 {
            mom.observe(&["es".into()]);
        }
        let cands = vec![c("hello", "en", 0), c("hola", "es", 1), c("hi", "en", 2)];
        assert_eq!(
            rank(&cands, &mom, 3),
            rank_with_bias(&cands, &mom, 3, |_| 0.0)
        );
    }

    use proptest::prelude::*;
    proptest! {
        #[test]
        fn spanish_momentum_flips_at_the_first_word_and_never_reverts(bumps in 0u32..40) {
            let cands = vec![c("hello","en",0), c("hola","es",0)];
            let mut mom = Momentum::new("en", &["en".into(),"es".into()]);
            for _ in 0..bumps { mom.observe(&["es".into()]); }
            let out = rank(&cands, &mom, 2);
            // Invariant every run: both candidates survive (distinct words, k=2).
            prop_assert_eq!(out.len(), 2);
            let hola_idx = out.iter().position(|r| r.word == "hola").expect("hola present");
            // Exact, non-vacuous crossover. Same source_rank and source, so order is
            // decided purely by momentum weight. Seed: en=1.05 (FLOOR+HEAD_START),
            // es=0.05 (FLOOR). One Spanish word already flips it — es=0.05·0.9+1=1.045
            // beats en=1.05·0.9=0.945 — and every later word only widens the gap
            // (es→10, en→FLOOR), so hola leads forever after. The range runs past the
            // ~29th word where en clamps to FLOOR, proving the lead survives the clamp.
            if bumps == 0 {
                prop_assert_eq!(hola_idx, 1); // cold es: primary head-start keeps English first
            } else {
                prop_assert_eq!(hola_idx, 0); // any Spanish word puts hola first, and it stays
            }
        }
    }
}
