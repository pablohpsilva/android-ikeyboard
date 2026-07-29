//! Ordering and scoring for correction candidates.
//!
//! Split from `lib.rs` so each file keeps one concern (and stays inside the
//! no-god-file bound, ARCH §6): `lib.rs` decides **whether** a token may be
//! corrected (the no-clobber rule, BR-12), this module decides **which**
//! candidate wins.
//!
//! Two things matter here and are easy to get wrong:
//!
//! * A candidate's `source_rank` is its **position after ordering by bundled
//!   frequency**, not its raw lexicon rank. Raw ranks span the whole lexicon and
//!   would swamp the ranker's momentum term; positions keep the scale identical
//!   to the strip's (`StatisticalPredictor::suggest_ranked`).
//! * [`Dictionary::fuzzy`] answers in *alphabetical* order (it collects into a
//!   `BTreeSet`). Feeding that straight through as commonness is the defect this
//!   module exists to prevent.

use std::collections::HashMap;

use featherkey_contracts::{Candidate, Source};
use featherkey_dictionary::Dictionary;
use featherkey_language_momentum::Momentum;

use crate::LexiconPack;

/// Stickiness of the trusted edit-distance fix versus the momentum nudge. The
/// primary language's commonest fix carries this bonus, so an unambiguous typo
/// keeps its fix unless a competing-language candidate's momentum-weighted score
/// overtakes it. High ⇒ legacy behaviour; low ⇒ momentum flips corrections sooner.
pub const CORE_FUZZY_PRIOR: f64 = 0.5;

/// One pack's edit-distance-1 neighbours of `text`, **commonest first** by its
/// bundled rank; a word carrying no bundled rank sorts last, and equal ranks keep
/// lexicographic order so the result is deterministic. Total: no panic.
pub(crate) fn ranked_neighbours(
    dict: &Dictionary,
    rank: &HashMap<String, u32>,
    text: &str,
) -> Vec<String> {
    let mut ns = dict.fuzzy(text);
    ns.sort_by(|a, b| {
        let (ra, rb) = (
            rank.get(a).copied().unwrap_or(u32::MAX),
            rank.get(b).copied().unwrap_or(u32::MAX),
        );
        ra.cmp(&rb).then_with(|| a.cmp(b))
    });
    ns
}

/// Correction candidates for `text`: every active language's edit-distance-1
/// neighbours (each ranked within its own language, [`Source::Lexicon`]) unioned
/// with the device-supplied candidates.
pub(crate) fn gather_candidates(
    packs: &[LexiconPack],
    text: &str,
    device_cands: Vec<Candidate>,
) -> Vec<Candidate> {
    let mut cands: Vec<Candidate> = Vec::new();
    for p in packs {
        for (position, word) in ranked_neighbours(&p.dict, &p.rank, text)
            .into_iter()
            .enumerate()
        {
            cands.push(Candidate {
                word,
                lang: p.lang.clone(),
                source: Source::Lexicon,
                // A lexicon's word count is far below `u32::MAX`.
                source_rank: position as u32,
            });
        }
    }
    cands.extend(device_cands);
    cands
}

/// Score every candidate with the shared ranker, adding [`CORE_FUZZY_PRIOR`] to
/// the sticky fix, and return `(index, score)` pairs sorted best-first.
///
/// The sticky fix is the primary language's **commonest** lexicon neighbour —
/// its `source_rank == 0`, which [`gather_candidates`] assigns by bundled
/// frequency — with the first lexicon candidate as fallback.
pub(crate) fn score_with_sticky(
    cands: &[Candidate],
    packs: &[LexiconPack],
    momentum: &Momentum,
) -> Vec<(usize, f64)> {
    let primary = packs.first().map(|p| p.lang.clone());
    let sticky = cands
        .iter()
        .position(|c| {
            c.source == Source::Lexicon && c.source_rank == 0 && Some(&c.lang) == primary.as_ref()
        })
        .or_else(|| cands.iter().position(|c| c.source == Source::Lexicon));

    let mut scored: Vec<(usize, f64)> = cands
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut s = featherkey_candidate_ranker::score(c, momentum);
            if Some(i) == sticky {
                s += CORE_FUZZY_PRIOR;
            }
            (i, s)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

/// The up-to-two best alternatives after the winner, as **distinct** words: a
/// cognate emitted for several active languages appears once, and the winner is
/// never echoed.
pub(crate) fn distinct_alternatives(
    scored: &[(usize, f64)],
    cands: &[Candidate],
    winner: &str,
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    seen.insert(winner.to_owned());
    scored
        .iter()
        .skip(1)
        .map(|&(i, _)| cands[i].word.clone())
        .filter(|w| seen.insert(w.clone()))
        .take(2)
        .collect()
}
