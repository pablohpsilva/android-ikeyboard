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
/// Hidden layer width. `pub(crate)`: `persist.rs` validates a loaded net's
/// shape against it before trusting the blob.
pub(crate) const H: usize = 32;
/// Output classes: one per vocab index (reserved `UNK`/`BOS` + learned words).
/// `pub(crate)`: `persist.rs` validates a loaded net's shape against it, and
/// derives [`EMBED_LEN`] from it.
pub(crate) const OUTPUTS: usize = 2 + MAX_VOCAB;
/// `MlpMulti` input width: the concatenated context embedding. `pub(crate)`:
/// `persist.rs` validates a loaded net's shape against it.
pub(crate) const INPUTS: usize = K * D;
/// Total length of [`NextWordLm::embed`] — fixed by the model's shape, not
/// per-instance — so `persist.rs` can slice the embedding sub-blob without a
/// length prefix.
pub(crate) const EMBED_LEN: usize = OUTPUTS * D;

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

/// The deterministic cold-start embedding row for vocab `index` — the exact
/// `D` values [`NextWordLm::new`] seeds that row with when it builds the
/// whole table. Shared by `new` (bulk init, every index) and
/// [`NextWordLm::reset_evicted_index`] (a single index, after an eviction),
/// so "what a fresh row looks like" has exactly one definition.
fn det_embed_row(index: usize) -> [f32; D] {
    let start = index.saturating_mul(D);
    let mut row = [0.0_f32; D];
    for (d, cell) in row.iter_mut().enumerate() {
        *cell = det_val(start.saturating_add(d) as u64);
    }
    row
}

/// The log-prob distribution over the whole vocabulary for one context,
/// computed by [`NextWordLm::scores`] with a single forward pass. Indexed by
/// vocab index (`0..OUTPUTS`); look a word's log-prob up via
/// [`NextWordLm::logprob_in`]. Sharing one `LmScores` across every candidate
/// in a query — instead of calling [`NextWordLm::score_next`] per candidate —
/// is what collapses N redundant forward passes into one per query.
#[derive(Debug, Clone)]
pub struct LmScores {
    /// `ln(softmax(forward(context)))`, one entry per vocab index.
    logprobs: Vec<f32>,
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
        let mut embed = Vec::with_capacity(OUTPUTS * D);
        for index in 0..OUTPUTS {
            embed.extend_from_slice(&det_embed_row(index));
        }
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

    /// Test-only twin of [`NextWordLm::new`] with a small `Vocab` learned
    /// ceiling (mirrors [`Vocab::with_capacity_for_test`]), so eviction —
    /// and therefore [`NextWordLm::reset_evicted_index`] — can be exercised
    /// without training thousands of words first. Never public API.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn new_with_vocab_ceiling_for_test(ceiling: usize) -> Self {
        let mut lm = Self::new();
        lm.vocab = Vocab::with_capacity_for_test(ceiling);
        lm
    }

    /// Rebuild a `NextWordLm` from its already-validated parts. `pub(crate)`:
    /// the sole caller is `persist::decode`, which has already checked the
    /// `vocab`/`embed`/`net` shapes against this module's constants before
    /// calling this — a fallible `load` never constructs a value here that it
    /// isn't sure of, so this constructor itself can stay infallible.
    pub(crate) fn from_parts(vocab: Vocab, embed: Vec<f32>, net: MlpMulti, warmup: u32) -> Self {
        Self {
            vocab,
            embed,
            net,
            warmup,
            #[cfg(test)]
            freeze_embeddings: false,
        }
    }

    /// Log-probability of `word` following `context`: `ln(softmax(forward)[idx])`,
    /// with the probability clamped away from 0 so the result is always finite.
    ///
    /// Implemented on top of [`Self::scores`]/[`Self::logprob_in`] so there is
    /// exactly one forward-pass path; a caller scoring many words against the
    /// *same* context should call [`Self::scores`] once and look each word up
    /// via [`Self::logprob_in`] instead of calling this per word (each call
    /// here re-runs the full `MlpMulti::forward` + softmax).
    #[must_use]
    pub fn score_next(&self, context: &[&str], word: &str) -> f32 {
        self.logprob_in(&self.scores(context), word)
    }

    /// The log-prob distribution over the whole vocabulary for one `context`,
    /// computed with a single forward pass. Callers that need more than one
    /// word's log-prob against the same context (the re-ranker scoring every
    /// candidate in a query) should compute this once and look words up via
    /// [`Self::logprob_in`], rather than calling [`Self::score_next`] per word
    /// — which would otherwise redundantly re-run the forward pass.
    #[must_use]
    pub fn scores(&self, context: &[&str]) -> LmScores {
        let probs = self.predict(context);
        LmScores {
            logprobs: probs.iter().map(|&p| p.max(MIN_PROB).ln()).collect(),
        }
    }

    /// Look up `word`'s log-probability within an [`LmScores`] distribution
    /// previously computed by [`Self::scores`]. `pub` (not a method on
    /// `LmScores` itself): resolving a word to its vocab index needs
    /// `self.vocab`, which `LmScores` deliberately doesn't carry — it is just
    /// the per-index distribution, reusable across every candidate word.
    #[must_use]
    pub fn logprob_in(&self, scores: &LmScores, word: &str) -> f32 {
        let idx = self.vocab.index_of(word);
        scores.logprobs.get(idx).copied().unwrap_or(MIN_PROB.ln())
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

    /// `ln(1 / OUTPUTS)`: the log-probability every class would have under a
    /// perfectly uniform distribution. The single source of the output class
    /// count (`2 + MAX_VOCAB`), so a caller centering a raw [`Self::score_next`]
    /// (e.g. the `featherkey-core` re-ranker's `lm_logprob` feature) never
    /// re-derives `OUTPUTS` itself.
    #[must_use]
    pub fn log_uniform(&self) -> f32 {
        -((2 + MAX_VOCAB) as f32).ln()
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
    /// the model could never tell two such contexts apart. When interning a
    /// context word evicts another, [`NextWordLm::reset_evicted_index`] runs
    /// *before* the freed index's row is read into the assembled input, so
    /// this step's own input never contains a stale, evicted-word vector.
    pub(crate) fn assemble_for_training(&mut self, context: &[&str]) -> (Vec<f32>, Vec<usize>) {
        let (pad, tail) = split_context(context);
        let indices = padded_indices(pad, tail, |word| {
            let (idx, evicted) = self.vocab.intern(word);
            if let Some(freed) = evicted {
                self.reset_evicted_index(freed);
            }
            idx
        });
        (self.embed_rows(&indices), indices)
    }

    /// Reset vocab index `index`'s embedding row (back to the deterministic
    /// cold-start value [`NextWordLm::new`] would have given it) and, via
    /// `net`, its `MlpMulti` output row (back to zero — again matching
    /// cold-start). Called whenever `Vocab::intern` reports `index` was just
    /// freed by an eviction, so the new word taking over that index starts
    /// from the same neutral state a genuinely fresh index would have,
    /// rather than inheriting whatever the evicted word had trained there.
    pub(crate) fn reset_evicted_index(&mut self, index: usize) {
        let row = det_embed_row(index);
        let start = index.saturating_mul(D);
        for (d, &value) in row.iter().enumerate() {
            if let Some(cell) = self.embed.get_mut(start + d) {
                *cell = value;
            }
        }
        self.net.reset_output_row(index);
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
