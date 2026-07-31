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

use featherkey_contracts::{Candidate, Correction, Source};
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

/// Plain Levenshtein distance between `typed` and `winner`, capped at the
/// longer string's length (so it can never exceed a trivial delete-then-insert
/// bound). Used only to report the winner's edit distance to the caller
/// (`AvailableCorrection::edit_distance`) — it plays no role in candidate
/// generation or scoring, which stay [`Dictionary::fuzzy`]'s edit-distance-1
/// neighbours. Operates on Unicode scalar values (`char`), not bytes, and is
/// total: no panic for any pair of strings, including empty ones.
pub(crate) fn edit_distance(typed: &str, winner: &str) -> u32 {
    let a: Vec<char> = typed.chars().collect();
    let b: Vec<char> = winner.chars().collect();
    let cap = a.len().max(b.len());
    if a.is_empty() {
        return b.len() as u32;
    }
    if b.is_empty() {
        return a.len() as u32;
    }
    // Classic O(n*m) DP; lexicon words are short (a handful of chars), so this
    // is cheap and needs no space optimisation.
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    (prev[b.len()].min(cap)) as u32
}

/// The result of `NoClobberCorrector::assess`: today's [`Correction`] outcome
/// alongside — when today's policy *would* apply a correction — the winning
/// candidate's own confidence, so a caller (the autocorrect gate) can decide
/// whether to actually apply it without re-deriving the winner.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionAssessment {
    /// The exact outcome `correct` returns today; unchanged by this type's
    /// existence (`correct` delegates to `assess().correction`).
    pub correction: Correction,
    /// `Some` only when a real correction winner exists — i.e. the token was
    /// not vetoed (`NoClobberCorrector::is_intended`) and at least one
    /// candidate survived and differs from the typed word. `None` for a
    /// vetoed or no-candidate token: there is nothing for a gate to weigh.
    pub available: Option<AvailableCorrection>,
}

/// The winning candidate's own detail, surfaced for a gate to weigh — not used
/// by `correct` itself, which still applies the winner unconditionally.
#[derive(Debug, Clone, PartialEq)]
pub struct AvailableCorrection {
    /// The candidate `correct` would apply.
    pub winner: String,
    /// The winner's own ranked score (`scored[0].1`, sticky-fix bonus and
    /// momentum included) — higher is more confident, but the scale is the
    /// ranker's own and not normalised to `[0, 1]`.
    pub winner_confidence: f64,
    /// Plain (capped) Levenshtein distance between the typed word and the
    /// winner.
    pub edit_distance: u32,
    /// The language the winner came from.
    pub winner_lang: String,
    /// The winner's bundled frequency rank in its own language's pack
    /// (`0` = commonest), or `None` when the winner has no such pack — e.g. it
    /// came solely from the device's own candidates.
    pub winner_dict_rank: Option<u32>,
}

/// Build the [`AvailableCorrection`] detail for a winning candidate: its edit
/// distance from the typed word, and its bundled rank looked up from its own
/// language's pack (`None` when the winner has no such pack, e.g. it came
/// solely from the device's own candidates).
pub(crate) fn available_correction(
    packs: &[LexiconPack],
    typed: &str,
    winner: &str,
    winner_lang: String,
    winner_confidence: f64,
) -> AvailableCorrection {
    let winner_dict_rank = packs
        .iter()
        .find(|p| p.lang == winner_lang)
        .and_then(|p| p.rank.get(winner).copied());
    AvailableCorrection {
        winner: winner.to_owned(),
        winner_confidence,
        edit_distance: edit_distance(typed, winner),
        winner_lang,
        winner_dict_rank,
    }
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
