//! Word-level noisy-channel decode: which real words does a *sequence* of
//! ambiguous taps explain?
//!
//! `input-decoder` answers one tap at a time and its answer is final. That is
//! enough until a tap lands between two keys: the character is committed, and no
//! later evidence can revise it. Typing `the` with a first tap that drifts onto
//! `r` produces `rhe` — a live prefix (`rhythm`, `rhino`), so no per-tap rescue
//! fires, and two edits from `the`, so edit-distance-1 correction cannot reach it
//! either.
//!
//! This crate keeps every tap as a **distribution** and searches over the whole
//! word, so an early tap can be reinterpreted once later taps arrive (BR-5,
//! BR-6). The search is a bounded beam: expand each surviving prefix by the tap's
//! [`BRANCH`] likeliest keys, drop every prefix the lexicon cannot continue, keep
//! the [`BEAM`] best by summed log-probability, then complete the survivors.
//!
//! **It scores spatial fit and nothing else.** Word frequency, learned counts,
//! next-word context and language momentum already live in `prediction`,
//! `personalization`, `context` and `candidate-ranker`; a second copy here would
//! be exactly the duplication that lets two rankings drift apart. The caller
//! combines this score with those.
//!
//! The lexicon arrives through the [`Lexicon`] trait, so this crate depends on no
//! dictionary implementation and is testable against a `BTreeSet`.
//!
//! Errors are values (SEDD §5.5 r3): every entry point is total — an empty
//! sequence, an empty lexicon, or a word no prefix survives all yield an empty
//! result. No `unwrap`, `expect`, or `panic` appears in this crate.

mod beam;

pub use beam::{hypotheses, Hypothesis};

/// Key candidates considered per tap. Three covers a tap that drifted onto a
/// neighbour without letting the search wander onto keys the finger never
/// approached.
pub const BRANCH: usize = 3;

/// Live prefixes carried between taps. Wide enough that the right word survives
/// an early wrong-looking letter, narrow enough to keep the work bounded.
pub const BEAM: usize = 12;

/// Longest word the buffer holds. Beyond this the oldest tap is dropped: a
/// 33-character token is not a word this search can help with, and the bound is
/// what keeps the buffer from growing (BR-46).
pub const MAX_TAPS: usize = 32;

/// Completions requested per surviving prefix.
pub const COMPLETIONS: usize = 6;

/// Probability floor for a key the decoder rated at (or near) zero — keeps the
/// logarithm finite without letting an implausible key win.
pub const FLOOR: f32 = 0.03;

/// Penalty per character a completion adds beyond the number of taps, so a short
/// exact explanation outranks a long speculative one.
pub const TAIL_PENALTY: f32 = 0.25;

/// One tap, as the decoder saw it: up to [`BRANCH`] `(key, probability)` pairs,
/// best first, stored inline so recording a tap allocates nothing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TapDistribution {
    keys: [(char, f32); BRANCH],
    len: usize,
}

impl TapDistribution {
    /// Keep the [`BRANCH`] most likely of `ranked` (already best-first, as
    /// `KeyCandidates::ranked` yields). Extra candidates are dropped, not
    /// merged: they are the ones the finger was furthest from.
    #[must_use]
    pub fn from_ranked(ranked: impl Iterator<Item = (char, f32)>) -> Self {
        let mut keys = [(' ', 0.0); BRANCH];
        let mut len = 0;
        for (key, probability) in ranked.take(BRANCH) {
            keys[len] = (key, probability);
            len += 1;
        }
        Self { keys, len }
    }

    /// The `(key, probability)` pairs this tap kept, best first.
    #[must_use]
    pub fn keys(&self) -> &[(char, f32)] {
        &self.keys[..self.len]
    }

    /// How many candidates this tap kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the tap kept no candidate at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The likeliest key — what the decoder committed for this tap.
    #[must_use]
    pub fn best(&self) -> Option<char> {
        self.keys().first().map(|(key, _)| *key)
    }
}

/// The taps of the word currently being typed, oldest first.
///
/// Bounded and preallocated: beyond [`MAX_TAPS`] it drops the oldest tap rather
/// than growing. Purely in-memory — transient input state, never persisted and
/// never handed to a `SecureStore` (BR-26).
#[derive(Debug, Clone)]
pub struct TapSequence {
    taps: Vec<TapDistribution>,
}

impl TapSequence {
    /// An empty sequence with room for [`MAX_TAPS`] taps already reserved.
    #[must_use]
    pub fn new() -> Self {
        Self {
            taps: Vec::with_capacity(MAX_TAPS),
        }
    }

    /// Record one tap. At capacity the oldest tap is dropped, so the buffer can
    /// never grow past its preallocated room (BR-46).
    pub fn push(&mut self, dist: TapDistribution) {
        if self.taps.len() == MAX_TAPS {
            self.taps.remove(0);
        }
        self.taps.push(dist);
    }

    /// Drop the most recent tap (backspace). A no-op when empty — never a panic.
    pub fn pop(&mut self) {
        self.taps.pop();
    }

    /// Drop every tap (word boundary, or a prefix the taps cannot explain).
    pub fn clear(&mut self) {
        self.taps.clear();
    }

    /// Drop taps until at most `len` remain.
    pub fn truncate(&mut self, len: usize) {
        self.taps.truncate(len);
    }

    /// How many taps are buffered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.taps.len()
    }

    /// Whether no tap is buffered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.taps.is_empty()
    }

    /// The fixed capacity — [`MAX_TAPS`].
    #[must_use]
    pub fn capacity(&self) -> usize {
        MAX_TAPS
    }

    /// The buffered taps, oldest first.
    #[must_use]
    pub fn taps(&self) -> &[TapDistribution] {
        &self.taps
    }

    /// The string the decoder actually committed — each tap's likeliest key.
    /// The caller compares this with the prefix the shell reports to decide
    /// whether the buffer still describes the word being typed.
    #[must_use]
    pub fn committed(&self) -> String {
        self.taps.iter().filter_map(TapDistribution::best).collect()
    }
}

impl Default for TapSequence {
    fn default() -> Self {
        Self::new()
    }
}

/// What the beam needs from a lexicon, and no more.
///
/// Keeping this a trait is what keeps the crate free of any dictionary
/// dependency — the composition root implements it over the active language
/// packs, tests over a `BTreeSet`.
pub trait Lexicon {
    /// Does any real word start with `prefix`? The beam prunes on this, so it is
    /// called far more often than [`completions`](Lexicon::completions).
    fn is_live_prefix(&self, prefix: &str) -> bool;

    /// Up to `limit` real words starting with `prefix`. The implementor decides
    /// the order; one that truncates should return its **commonest** words, since
    /// the beam cannot recover a word it never sees.
    fn completions(&self, prefix: &str, limit: usize) -> Vec<String>;
}
