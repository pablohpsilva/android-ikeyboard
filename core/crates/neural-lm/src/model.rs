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

/// Context width: the last `K` words feed the model.
const K: usize = 2;
/// Embedding dimension per word.
const D: usize = 16;
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
/// uniform, honest-confidence-zero state (see the module docs); training
/// (`observe`) is added in a later task, persistence in the one after that.
#[derive(Debug, Clone)]
pub struct NextWordLm {
    vocab: Vocab,
    /// Embedding table, `(2 + MAX_VOCAB) * D` rows-major by index.
    embed: Vec<f32>,
    net: MlpMulti,
    /// Training steps observed so far; drives [`NextWordLm::confidence`].
    warmup: u32,
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
        }
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
        let x = self.assemble(context);
        let logits = self.net.forward(&x);
        MlpMulti::softmax(&logits)
    }

    /// Last `K` of `context`, left-padded with `BOS`, mapped to embedding
    /// rows and concatenated into an `INPUTS`-length vector. Never panics:
    /// an out-of-range row (shouldn't happen given `Vocab`'s invariants)
    /// contributes zeros rather than indexing out of bounds.
    fn assemble(&self, context: &[&str]) -> Vec<f32> {
        let tail_start = context.len().saturating_sub(K);
        let tail = &context[tail_start..];
        let pad = K.saturating_sub(tail.len());

        let mut x = Vec::with_capacity(INPUTS);
        for _ in 0..pad {
            self.append_embed_row(BOS, &mut x);
        }
        for word in tail {
            self.append_embed_row(self.vocab.index_of(word), &mut x);
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
