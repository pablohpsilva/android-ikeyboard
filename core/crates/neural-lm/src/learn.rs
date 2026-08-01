//! `NextWordLm::observe`: online cross-entropy training + the embedding
//! update that lets similar contexts generalise (see design §6).
//!
//! `observe` is the whole reason this crate exists: cold-start (Task 7) is
//! neutral, but a model that never learns is not a language model. Two
//! things happen on every call, together, in the same step:
//! - `net.train_step` backprops the target word's cross-entropy loss through
//!   `MlpMulti`, updating `w1`/`b1`/`w2`/`b2` by SGD and returning `dL/dInput`.
//! - That `dInput` — the gradient the network would like to see at its input
//!   — is applied to the `K` embedding rows that *formed* that input, one
//!   `D`-wide slice per context slot. This is what lets "the" and "a" (both
//!   followed by "cat" in training) drift toward similar vectors, so a novel
//!   context like "a dog" can borrow from what "the dog" taught the model —
//!   the bigram table this replaces cannot do that (see
//!   `generalises_across_similar_contexts_via_embeddings`).

use crate::model::D;
use crate::vocab::UNK;
use crate::NextWordLm;

/// SGD learning rate for both the network step and the embedding update.
/// Recommended by the design (§6); tuned against
/// `generalises_across_similar_contexts_via_embeddings` so the contamination
/// guard is genuinely RED without the embedding update (see `learn/tests.rs`).
const LM_LR: f32 = 0.006;

impl NextWordLm {
    /// One online training step: `context` predicts `next_word`.
    ///
    /// 1. Interns `next_word` into a target class. A non-learnable word (per
    ///    `Vocab`'s `is_learnable` gate — too short, or a codec separator)
    ///    interns to [`UNK`]; a `<unk>` target teaches the model nothing
    ///    useful and would only pollute the output layer's `UNK` row, so this
    ///    step is skipped entirely — no network step, no `warmup` bump. (A
    ///    full-vocab eviction does *not* take this path: it still returns a
    ///    real, reused index — see `Vocab::intern`.)
    /// 2. Assembles the `K`-slot context embedding, capturing which vocab
    ///    index formed each slot. Unlike `score_next`/`rank_next` (which must
    ///    never mutate `vocab` as a side effect of a query), this interns
    ///    each context word — a word that is only ever seen as context (never
    ///    independently trained as a `next_word` target) still needs a
    ///    stable index of its own, or it collapses onto `UNK` and becomes
    ///    indistinguishable from every other never-a-target context word.
    /// 3. One `MlpMulti::train_step` (cross-entropy + SGD on `w1/b1/w2/b2`).
    ///    A [`featherkey_nn::NnError::Shape`] (the target index outgrew the
    ///    network's fixed output width — not reachable today since `OUTPUTS`
    ///    is sized to the vocab ceiling, but a caller error either way, not a
    ///    panic) skips the rest of the step rather than propagating.
    /// 4. Applies the returned `dInput` to the `K` embedding rows that formed
    ///    the input: `row[w_{t-j}] -= LM_LR * dInput[j*D..(j+1)*D]`. This is
    ///    the step under test's `freeze_embeddings` escape hatch.
    /// 5. Bumps `warmup`, which drives [`NextWordLm::confidence`].
    pub fn observe(&mut self, context: &[&str], next_word: &str) {
        let target = self.vocab.intern(next_word);
        if target == UNK {
            return;
        }

        let (input, indices) = self.assemble_for_training(context);
        let Ok((_loss, dinput)) = self.net.train_step(&input, target, LM_LR) else {
            return; // Shape race: target outgrew `outputs` — skip, don't panic.
        };

        if !self.embeddings_frozen() {
            for (slot, &idx) in indices.iter().enumerate() {
                let start = slot * D;
                if let Some(grad) = dinput.get(start..start + D) {
                    self.apply_embedding_gradient(idx, grad);
                }
            }
        }

        self.warmup = self.warmup.saturating_add(1);
    }

    /// One SGD step on a single embedding row: `row[index] -= LM_LR * grad`.
    /// Never panics: `get_mut` skips any cell the row doesn't have (e.g. an
    /// out-of-range index, which `Vocab`'s invariants shouldn't produce).
    fn apply_embedding_gradient(&mut self, index: usize, grad: &[f32]) {
        let start = index.saturating_mul(D);
        for (d, &g) in grad.iter().enumerate() {
            if let Some(cell) = self.embed.get_mut(start + d) {
                *cell -= LM_LR * g;
            }
        }
    }

    /// `true` when this instance's `observe` must skip the embedding update
    /// (the `#[cfg(test)]`-only contamination-guard twin). Always `false`
    /// outside test builds, where `freeze_embeddings` doesn't exist at all.
    #[cfg(test)]
    fn embeddings_frozen(&self) -> bool {
        self.freeze_embeddings
    }

    #[cfg(not(test))]
    fn embeddings_frozen(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests;
