//! `NextWordLm`: cold-start init + next-word inference (see design §7).
//!
//! Cold-start is the load-bearing part of this module: only the output
//! layer (`w2`/`b2`) is zero-initialised. The embedding table and the
//! input->hidden weights (`w1`/`b1`) are non-zero and deterministic. That
//! split is what keeps a fresh model *uniform* (zero output layer -> every
//! logit equals the same zero bias -> uniform softmax -> confidence 0)
//! while staying *trainable* (non-zero `w1`/`b1` -> non-zero hidden
//! activations -> the output layer receives gradient on the very first
//! training step). If `w1`/`b1` were also zero, hidden activations would be
//! zero and the output layer would never receive gradient — the model would
//! be frozen at uniform forever.

use crate::vocab::{BOS, MAX_VOCAB, UNK};
use crate::Vocab;
use featherkey_nn::MlpMulti;

/// Context width: the last `K` words feed the model. `pub(crate)`: `learn.rs`
/// (a sibling module) iterates the `K` context slots to update their
/// embedding rows from the training gradient.
pub(crate) const K: usize = 2;
/// Embedding dimension per word. `pub(crate)`: `learn.rs` slices `dInput`
/// into `K` chunks of this width, one per context slot.
pub(crate) const D: usize = 16;
/// Hidden layer width.
const H: usize = 32;
/// Output classes: one per vocab index (reserved `UNK`/`BOS` + learned words).
const OUTPUTS: usize = 2 + MAX_VOCAB;
/// `MlpMulti` input width: the concatenated context embedding.
const INPUTS: usize = K * D;

/// `observe` steps (Task 8) needed for `confidence` to reach 0.5.
const WARMUP_HALF: f32 = 50.0;
/// Floor a probability is clamped to before `ln`, so `score_next` is always
/// finite even for a zero-probability class.
const MIN_PROB: f32 = 1e-9;

/// Disjoint seed ranges so the embedding table, `w1`, and `b1` don't hash to
/// the same deterministic values by coincidence.
const W1_SEED_BASE: u64 = 1 << 32;
const B1_SEED_BASE: u64 = 2 << 32;

/// Split `context` into `(pad, tail)`: `tail` is the last `K` words (fewer if
/// `context` is shorter), `pad` is how many leading `BOS` slots are needed to
/// fill out the remaining `K - tail.len()` slots. Shared by
/// [`NextWordLm::assemble`] and [`NextWordLm::assemble_for_training`], which
/// differ only in how they resolve a tail word to a vocab index.
fn split_context<'a>(context: &'a [&str]) -> (usize, &'a [&'a str]) {
    let tail_start = context.len().saturating_sub(K);
    let tail = &context[tail_start..];
    let pad = K.saturating_sub(tail.len());
    (pad, tail)
}

/// Build the `K`-length index list: `pad` leading `BOS` slots, then `tail`
/// resolved word-by-word through `resolve` (`Vocab::index_of` for read-only
/// inference, `Vocab::intern` for training — see the two `assemble*`
/// callers).
fn padded_indices(pad: usize, tail: &[&str], mut resolve: impl FnMut(&str) -> usize) -> Vec<usize> {
    let mut indices = Vec::with_capacity(K);
    indices.extend(std::iter::repeat_n(BOS, pad));
    for &word in tail {
        indices.push(resolve(word));
    }
    indices
}

/// Cheap deterministic hash (splitmix64) mapped into `[-0.1, 0.1)`. No
/// `rand` crate (zero-new-deps); reproducible across runs and platforms —
/// the cold-start init depends on that.
fn det_val(seed: u64) -> f32 {
    let mut x = seed ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    let frac = (x % 20_000) as f32 / 20_000.0; // [0, 1)
    (frac - 0.5) * 0.2 // [-0.1, 0.1)
}

/// A tiny per-user embedding next-word language model. Cold-starts to a
/// uniform, honest-confidence-zero state (see the module docs); `observe`
/// (in the sibling `learn` module) trains it online.
// Fields are `pub(crate)`, not private: `learn.rs` is a sibling module (both
// children of the crate root, per `lib.rs`), and it backpropagates through
// `net` and mutates `embed`/`warmup` directly — mirroring why `featherkey_nn`
// keeps `MlpMulti`'s `w1`/`b1`/`w2`/`b2` `pub(crate)` for its own sibling
// `multi_train` module.
#[derive(Debug, Clone)]
pub struct NextWordLm {
    pub(crate) vocab: Vocab,
    /// Embedding table, `(2 + MAX_VOCAB) * D` rows-major by index.
    pub(crate) embed: Vec<f32>,
    pub(crate) net: MlpMulti,
    /// Training steps observed so far; drives [`NextWordLm::confidence`].
    pub(crate) warmup: u32,
    /// Test-only escape hatch: when `true`, `observe` (in `learn.rs`) skips
    /// the embedding-row update so a test can isolate it as the sole path to
    /// generalisation. Never part of the public API — see
    /// [`NextWordLm::new_frozen_embeddings_for_test`].
    #[cfg(test)]
    pub(crate) freeze_embeddings: bool,
}

impl Default for NextWordLm {
    fn default() -> Self {
        Self::new()
    }
}

impl NextWordLm {
    /// Cold-start a fresh model: zero output layer, deterministic non-zero
    /// embeddings/`w1`/`b1` (see module docs for why the split matters).
    #[must_use]
    pub fn new() -> Self {
        let embed = (0..OUTPUTS * D).map(|i| det_val(i as u64)).collect();
        let w1 = (0..H * INPUTS)
            .map(|i| det_val(W1_SEED_BASE + i as u64))
            .collect();
        let b1 = (0..H).map(|i| det_val(B1_SEED_BASE + i as u64)).collect();
        let w2 = vec![0.0_f32; OUTPUTS * H];
        let b2 = vec![0.0_f32; OUTPUTS];
        let net = MlpMulti::with_weights(w1, b1, w2, b2, INPUTS, H, OUTPUTS);
        Self {
            vocab: Vocab::new(),
            embed,
            net,
            warmup: 0,
            #[cfg(test)]
            freeze_embeddings: false,
        }
    }

    /// Test-only twin of [`NextWordLm::new`] whose `observe` skips the
    /// embedding-row update (step 4). Used by the generalisation
    /// contamination guard to prove the embedding update — not `w1`/`b1`
    /// alone — is what lets similar contexts generalise. Never public API.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn new_frozen_embeddings_for_test() -> Self {
        let mut lm = Self::new();
        lm.freeze_embeddings = true;
        lm
    }

    /// Log-probability of `word` following `context`: `ln(softmax(forward)[idx])`,
    /// with the probability clamped away from 0 so the result is always finite.
    #[must_use]
    pub fn score_next(&self, context: &[&str], word: &str) -> f32 {
        let probs = self.predict(context);
        let idx = self.vocab.index_of(word);
        probs.get(idx).copied().unwrap_or(0.0).max(MIN_PROB).ln()
    }

    /// Top-`limit` `(word, log_prob)` pairs, best-first. Reserved indices
    /// (`UNK`/`BOS`) and indices with no live word are never emitted. Ties
    /// (e.g. the cold-start uniform distribution) break by ascending word,
    /// so the ranking is deterministic.
    #[must_use]
    pub fn rank_next(&self, context: &[&str], limit: usize) -> Vec<(String, f32)> {
        let probs = self.predict(context);
        let mut scored: Vec<(String, f32)> = probs
            .iter()
            .enumerate()
            .filter(|&(idx, _)| idx != UNK && idx != BOS)
            .filter_map(|(idx, &p)| {
                self.vocab
                    .word_of(idx)
                    .map(|w| (w.to_owned(), p.max(MIN_PROB).ln()))
            })
            .collect();
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        scored.truncate(limit);
        scored
    }

    /// Bounded `[0, 1]` confidence: a saturating warm-up curve over the
    /// number of training steps observed (`n / (n + WARMUP_HALF)`). A fresh
    /// model (`warmup == 0`) is exactly `0.0`.
    #[must_use]
    pub fn confidence(&self) -> f32 {
        let n = self.warmup as f32;
        n / (n + WARMUP_HALF)
    }

    /// Forward + softmax over the assembled context.
    fn predict(&self, context: &[&str]) -> Vec<f32> {
        let (x, _indices) = self.assemble(context);
        let logits = self.net.forward(&x);
        MlpMulti::softmax(&logits)
    }

    /// Last `K` of `context`, left-padded with `BOS`, mapped to embedding
    /// rows and concatenated into an `INPUTS`-length vector, alongside the
    /// `K` vocab indices that formed each slot. Read-only: an unregistered
    /// context word resolves to `UNK` rather than being interned, which is
    /// correct for inference (`score_next`/`rank_next` must never mutate
    /// `vocab` as a side effect of a query). Never panics: an out-of-range
    /// row (shouldn't happen given `Vocab`'s invariants) contributes zeros
    /// rather than indexing out of bounds.
    pub(crate) fn assemble(&self, context: &[&str]) -> (Vec<f32>, Vec<usize>) {
        let (pad, tail) = split_context(context);
        let indices = padded_indices(pad, tail, |word| self.vocab.index_of(word));
        (self.embed_rows(&indices), indices)
    }

    /// Training-time twin of [`NextWordLm::assemble`] (`learn.rs::observe`
    /// calls this, not `assemble`): context words are `intern`ed rather than
    /// merely looked up, so a word that is *only* ever seen as context (never
    /// independently trained as a `next_word` target — e.g. "going" in
    /// "going to work") still gets a stable, distinct vocab index and
    /// therefore a distinct, trainable embedding row. Without this, every
    /// never-a-target context word would collapse onto the same `UNK` row and
    /// the model could never tell two such contexts apart.
    pub(crate) fn assemble_for_training(&mut self, context: &[&str]) -> (Vec<f32>, Vec<usize>) {
        let (pad, tail) = split_context(context);
        let indices = padded_indices(pad, tail, |word| self.vocab.intern(word));
        (self.embed_rows(&indices), indices)
    }

    /// Concatenate the embedding rows for `indices` into an `INPUTS`-length
    /// vector (shared by [`NextWordLm::assemble`] and
    /// [`NextWordLm::assemble_for_training`]).
    fn embed_rows(&self, indices: &[usize]) -> Vec<f32> {
        let mut x = Vec::with_capacity(INPUTS);
        for &idx in indices {
            self.append_embed_row(idx, &mut x);
        }
        x
    }

    fn append_embed_row(&self, index: usize, out: &mut Vec<f32>) {
        let start = index.saturating_mul(D);
        for d in 0..D {
            out.push(self.embed.get(start + d).copied().unwrap_or(0.0));
        }
    }
}

#[cfg(test)]
mod tests;
