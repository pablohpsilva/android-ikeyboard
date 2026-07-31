//! Feature assembly for the neural re-ranker, plus the one-snapshot cache of the
//! shown set for later pairwise training.
//!
//! Kept out of `rank.rs` so both files stay inside the no-god-file bound
//! (ARCH §6): `rank.rs` owns the strip-blend *policy*; this module owns the
//! translation of one [`Candidate`] into the [`RankFeatures`] the net scores, and
//! the [`RankSnapshot`] the core caches after each ranked query so a subsequent
//! strip pick can train the ranker against exactly what the user saw (Task 12).

use featherkey_candidate_ranker::positional_score;
use featherkey_contracts::{Candidate, RankedCandidate, Source};
use featherkey_neural_ranker::RankFeatures;

use crate::FeatherKeyCore;

/// The most recent ranked query's shown set: the `prefix` (lowercased, matching
/// how `observe_strip_pick` keys corrections) and, per shown word, the exact
/// [`RankFeatures`] that scored it. Bounded to a single snapshot — every
/// `rank_suggestions` overwrites it — so a later pick trains the net against
/// precisely the set the user saw, at no standing memory cost.
#[derive(Debug)]
#[allow(dead_code)] // fields read by Task 12 (reinforce-from-pick); tested here.
pub(crate) struct RankSnapshot {
    /// The in-progress prefix these suggestions completed, lowercased.
    pub prefix: String,
    /// Each shown word with the feature vector that ranked it, in shown order.
    pub shown: Vec<(String, RankFeatures)>,
}

impl FeatherKeyCore {
    /// Assemble the ranking feature vector for one candidate completing `prefix`,
    /// given this query's `spatial` hypotheses `(word, log-prob)`.
    ///
    /// Built so the cold-start prior ([`PRIOR_COEFFS`](crate::rank::PRIOR_COEFFS))
    /// reproduces the classic linear score exactly: each slot is the raw signal
    /// the matching coefficient weights — the positional curve, `ln` of the
    /// language's momentum weight, the one-hot source flags, the two correction
    /// parts, and the spatial log-probability — so the net's `coeffs · features`
    /// equals the old `score + (promote − demote) + SPATIAL_WEIGHT · spatial`.
    pub(crate) fn rank_features(
        &self,
        cand: &Candidate,
        prefix: &str,
        spatial: &[(String, f32)],
    ) -> RankFeatures {
        let (promote, demote) = self.correction_parts(prefix, &cand.word);
        RankFeatures {
            positional: positional_score(cand.source_rank) as f32,
            ln_momentum: self.momentum.weight_of(&cand.lang).ln() as f32,
            is_lexicon: if matches!(cand.source, Source::Lexicon) {
                1.0
            } else {
                0.0
            },
            is_device: if matches!(cand.source, Source::Device) {
                1.0
            } else {
                0.0
            },
            correction_promote: promote as f32,
            correction_demote: demote as f32,
            spatial: spatial
                .iter()
                .find(|(w, _)| *w == cand.word)
                .map_or(0.0, |(_, s)| *s),
        }
    }

    /// Build the [`RankSnapshot`] for one ranked query: pair each shown word with
    /// the exact features that scored it (found back in `cands`), keyed by the
    /// lowercased `prefix`. Words dropped by the top-`k`/dedup cut are not shown,
    /// so they are not recorded.
    pub(crate) fn snapshot_shown(
        &self,
        prefix: &str,
        ranked: &[RankedCandidate],
        cands: &[Candidate],
        spatial: &[(String, f32)],
    ) -> RankSnapshot {
        let shown = ranked
            .iter()
            .filter_map(|rc| {
                cands
                    .iter()
                    .find(|c| c.word == rc.word)
                    .map(|c| (rc.word.clone(), self.rank_features(c, prefix, spatial)))
            })
            .collect();
        RankSnapshot {
            prefix: prefix.to_lowercase(),
            shown,
        }
    }

    /// The most recent ranked query's cached shown set, if any. Read seam for the
    /// pairwise trainer wired in Task 12; exercised now by the caching test.
    #[allow(dead_code)] // consumed by Task 12 (reinforce-from-pick); tested here.
    pub(crate) fn last_ranked(&self) -> Option<&RankSnapshot> {
        self.last_ranked.as_ref()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn rank_suggestions_matches_legacy_order_before_training() {
        // Before any training the neural scorer reproduces the classic linear
        // order. With no context, learning, device candidates or spatial signal,
        // bundled rank alone decides, so the frequency order is preserved exactly
        // — the same order the old `rank_with_bias` path produced. Pinned as a
        // hardcoded expectation, the way the sibling ordering tests pin today's
        // behaviour (they are the parity safety net proper).
        let mut core = FeatherKeyCore::new(vec![(
            "en".into(),
            vec!["tea".into(), "team".into(), "teal".into()],
        )])
        .expect("core");
        let out: Vec<String> = core
            .rank_suggestions("", "te", vec![])
            .into_iter()
            .map(|r| r.word)
            .collect();
        assert_eq!(out, ["tea", "team", "teal"]);
    }

    #[test]
    fn rank_suggestions_caches_the_shown_set() {
        // After a ranked query the core holds a single snapshot: the lowercased
        // prefix and the shown words (with their features) in the returned order.
        let mut core = FeatherKeyCore::new(vec![(
            "en".into(),
            vec!["tea".into(), "team".into(), "teal".into()],
        )])
        .expect("core");
        let out = core.rank_suggestions("", "te", vec![]);
        let out_words: Vec<&str> = out.iter().map(|r| r.word.as_str()).collect();

        let snap = core.last_ranked().expect("a snapshot is cached");
        assert_eq!(snap.prefix, "te");
        let shown_words: Vec<&str> = snap.shown.iter().map(|(w, _)| w.as_str()).collect();
        assert_eq!(shown_words, out_words);
    }

    #[test]
    fn rank_features_reproduces_the_classic_scalar_score() {
        // The eight-slot vector, dotted with PRIOR_COEFFS, must equal the classic
        // `candidate_ranker::score` plus the correction net and spatial term — the
        // per-candidate identity that makes the whole re-rank a no-op at cold start.
        let core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
        let cand = Candidate {
            word: "cat".into(),
            lang: "en".into(),
            source: Source::Lexicon,
            source_rank: 2,
        };
        let spatial = vec![("cat".to_string(), 0.5_f32)];
        let f = core.rank_features(&cand, "ca", &spatial);
        // Source flags are one-hot for a lexicon candidate; correction history is
        // empty; spatial matches the candidate word.
        assert_eq!(f.is_lexicon, 1.0);
        assert_eq!(f.is_device, 0.0);
        assert_eq!(f.correction_promote, 0.0);
        assert_eq!(f.correction_demote, 0.0);
        assert_eq!(f.spatial, 0.5);
        assert_eq!(f.positional, positional_score(2) as f32);
    }

    #[test]
    fn rank_features_marks_a_device_candidate_and_ignores_unmatched_spatial() {
        // A device candidate flips the source one-hot; a spatial list that does
        // not name this word leaves the spatial slot at 0.0.
        let core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
        let cand = Candidate {
            word: "hey".into(),
            lang: "en".into(),
            source: Source::Device,
            source_rank: 0,
        };
        let spatial = vec![("cat".to_string(), 0.9_f32)];
        let f = core.rank_features(&cand, "he", &spatial);
        assert_eq!(f.is_lexicon, 0.0);
        assert_eq!(f.is_device, 1.0);
        assert_eq!(f.spatial, 0.0);
    }
}
