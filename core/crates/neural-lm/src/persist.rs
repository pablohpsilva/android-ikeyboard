//! Encrypted self-persistence for [`NextWordLm`]: the vocabulary, the
//! embedding table, the network, and the warm-up counter are written as one
//! blob under [`Namespace::PersonalLm`] through the injected [`SecureStore`]
//! port; encryption and I/O live in `secure-store`, reached only through the
//! port (ADR-12 Dependency Rule). Nothing leaves the device (BR-13).
//!
//! Uses a distinct key (`lm_v1`) from the bigram `featherkey_context::Context`
//! model's `v1`, so the two models — both under `PersonalLm` — never collide.
//!
//! A corrupt, stale, or wrong-shape blob is **not** an error the caller must
//! handle: `load` silently falls back to [`NextWordLm::new`] (cold-start), so
//! a model-format change or a damaged record degrades to today's honest
//! zero-confidence prior rather than a failure. Only a real backend/crypto
//! [`StoreError`] from the store's own `get` propagates.

use featherkey_contracts::{Namespace, SecureStore, StoreError};
use featherkey_nn::MlpMulti;

use crate::model::{EMBED_LEN, H, INPUTS, OUTPUTS};
use crate::{NextWordLm, Vocab};

mod vocab_codec;

/// Storage key for the model's single blob under [`Namespace::PersonalLm`].
/// Distinct from the bigram `Context`'s `b"v1"` key so the two models never
/// collide in the same namespace.
const BLOB_KEY: &[u8] = b"lm_v1";
/// Current on-disk format version, checked by `decode` before anything else
/// so a future encoding change is detected rather than mis-parsed.
const VERSION: u16 = 1;

impl NextWordLm {
    /// Encrypt-and-store the whole model — vocabulary, embedding table,
    /// network, and warm-up counter — as one atomic
    /// [`put`](SecureStore::put) under [`Namespace::PersonalLm`]. A single
    /// write means a failure can never leave a partially-written model.
    ///
    /// # Errors
    /// Propagates any [`StoreError`] from the underlying store; this crate
    /// adds no error of its own on the write path.
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), StoreError> {
        store.put(Namespace::PersonalLm, BLOB_KEY, &encode(self))
    }

    /// Load a model previously written by [`persist`](Self::persist),
    /// falling back to [`NextWordLm::new`] (cold-start) when nothing is
    /// stored **or** the stored blob is corrupt/wrong-shape/wrong-version.
    ///
    /// This never surfaces a corrupt, old-format, or wrong-shape blob as an
    /// error: it degrades to cold-start, so a format or shape change never
    /// breaks next-word prediction. Only a [`StoreError`] from the store's
    /// own `get` propagates.
    ///
    /// # Errors
    /// Returns the store's [`StoreError`] on a backend/crypto failure.
    pub fn load(store: &impl SecureStore) -> Result<Self, StoreError> {
        let Some(bytes) = store.get(Namespace::PersonalLm, BLOB_KEY)? else {
            return Ok(Self::new());
        };
        Ok(decode(&bytes).unwrap_or_default())
    }
}

/// Serialize `lm` into the versioned blob `persist` writes:
/// `[u16 version][u32 vocab_len][vocab bytes][u32 warmup][embed f32 LE ...][net bytes]`.
/// `embed`'s length is fixed by the model's shape ([`EMBED_LEN`]), so it
/// needs no length prefix; `net` is the final field, so it also needs none —
/// [`MlpMulti::from_bytes`] consumes exactly its own bytes and no more.
fn encode(lm: &NextWordLm) -> Vec<u8> {
    let vocab_bytes = vocab_codec::encode(&lm.vocab);
    let net_bytes = lm.net.to_bytes();
    let mut out =
        Vec::with_capacity(2 + 4 + vocab_bytes.len() + 4 + lm.embed.len() * 4 + net_bytes.len());
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(vocab_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&vocab_bytes);
    out.extend_from_slice(&lm.warmup.to_le_bytes());
    for &v in &lm.embed {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&net_bytes);
    out
}

/// Parse the blob `encode` produces; any shape/format/version problem ⇒
/// `None` so the caller falls back to cold-start (never an error the caller
/// must handle). Panic-free by construction: `split_first_chunk`/
/// `split_at_checked` return `Option`, never index-panic.
fn decode(bytes: &[u8]) -> Option<NextWordLm> {
    let (version_bytes, rest) = bytes.split_first_chunk::<2>()?;
    if u16::from_le_bytes(*version_bytes) != VERSION {
        return None;
    }

    let (vocab_len_bytes, rest) = rest.split_first_chunk::<4>()?;
    let vocab_len = u32::from_le_bytes(*vocab_len_bytes) as usize;
    let (vocab_bytes, rest) = rest.split_at_checked(vocab_len)?;
    let vocab = Vocab::from_entries(vocab_codec::decode(vocab_bytes)?)?;

    let (warmup_bytes, rest) = rest.split_first_chunk::<4>()?;
    let warmup = u32::from_le_bytes(*warmup_bytes);

    let embed_byte_len = EMBED_LEN.checked_mul(4)?;
    let (embed_bytes, net_bytes) = rest.split_at_checked(embed_byte_len)?;
    let embed: Vec<f32> = embed_bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    let net = MlpMulti::from_bytes(net_bytes)
        .ok()
        .filter(|m| m.inputs() == INPUTS && m.hidden() == H && m.outputs() == OUTPUTS)?;

    Some(NextWordLm::from_parts(vocab, embed, net, warmup))
}

#[cfg(test)]
mod tests;
