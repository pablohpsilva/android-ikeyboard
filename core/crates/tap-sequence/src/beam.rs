//! The bounded beam search over a tap sequence.
//!
//! Kept apart from `lib.rs` so the buffer types and the search read separately
//! (ARCH §6). The search is the whole reason the crate exists: it is what lets a
//! tap already committed as `r` be reconsidered once `h` and `e` arrive.

use crate::{Lexicon, TapSequence, BEAM, BRANCH, COMPLETIONS, FLOOR, TAIL_PENALTY};

/// One word the taps could have meant, with the log-probability that they did.
///
/// `score` is **spatial fit only** — higher is better, always negative (it is a
/// sum of log-probabilities, minus a length penalty). It is not comparable
/// across different tap sequences and carries no frequency or context term; the
/// caller blends it with those.
#[derive(Debug, Clone, PartialEq)]
pub struct Hypothesis {
    /// The real word.
    pub word: String,
    /// How well the taps explain it (log-probability, higher is better).
    pub score: f32,
}

/// One live prefix in the beam: the string so far and its summed log-probability.
#[derive(Debug, Clone)]
struct Live {
    prefix: String,
    score: f32,
}

/// The words `taps` most plausibly spell, best-explained first, at most `limit`.
///
/// Total: an empty sequence, a lexicon that continues nothing, or a tap that
/// kept no candidate all yield an empty `Vec` rather than an error or a panic.
///
/// # Bounded work
/// Each tap expands at most `BEAM` live prefixes by at most [`BRANCH`] keys, and
/// the beam is truncated back to `BEAM` before the next tap — so the lexicon is
/// probed at most `BEAM × BRANCH` times per tap, plus one completion pass over
/// the survivors. Nothing here scales with the size of the lexicon.
#[must_use]
pub fn hypotheses(taps: &TapSequence, lex: &impl Lexicon, limit: usize) -> Vec<Hypothesis> {
    if taps.is_empty() || limit == 0 {
        return Vec::new();
    }
    let Some(beam) = walk(taps, lex) else {
        return Vec::new();
    };
    complete(&beam, taps.len(), lex, limit)
}

/// Advance the beam through every tap, pruning prefixes the lexicon cannot
/// continue. `None` when nothing survives.
fn walk(taps: &TapSequence, lex: &impl Lexicon) -> Option<Vec<Live>> {
    let mut beam = vec![Live {
        prefix: String::new(),
        score: 0.0,
    }];
    for tap in taps.taps() {
        let mut next: Vec<Live> = Vec::with_capacity(beam.len() * BRANCH);
        for live in &beam {
            for (key, probability) in tap.keys() {
                let mut prefix = live.prefix.clone();
                prefix.push(*key);
                if !lex.is_live_prefix(&prefix) {
                    continue;
                }
                next.push(Live {
                    prefix,
                    score: live.score + probability.max(FLOOR).ln(),
                });
            }
        }
        if next.is_empty() {
            // Every continuation is dead: the taps spell nothing real. Keeping
            // the previous beam would answer a different word than was typed.
            return None;
        }
        next.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        next.truncate(BEAM);
        beam = next;
    }
    Some(beam)
}

/// Complete each surviving prefix into real words, penalising the characters a
/// completion adds beyond what was actually typed.
fn complete(beam: &[Live], taps: usize, lex: &impl Lexicon, limit: usize) -> Vec<Hypothesis> {
    let mut best: Vec<Hypothesis> = Vec::new();
    for live in beam {
        for word in lex.completions(&live.prefix, COMPLETIONS) {
            let extra = word.chars().count().saturating_sub(taps);
            #[allow(clippy::cast_precision_loss)] // extra is bounded by word length
            let score = live.score - TAIL_PENALTY * extra as f32;
            match best.iter_mut().find(|h| h.word == word) {
                // The same word can be reached by several prefixes (e.g. via an
                // accent fold); keep its best explanation.
                Some(existing) => existing.score = existing.score.max(score),
                None => best.push(Hypothesis { word, score }),
            }
        }
    }
    best.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.word.cmp(&b.word))
    });
    best.truncate(limit);
    best
}
