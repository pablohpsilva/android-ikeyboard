//! Encrypted self-persistence for [`TapWarp`]. Both learned MLPs (Δx and Δy)
//! are written as one blob under [`Namespace::TapWarpModel`] through the
//! injected [`SecureStore`] port (the sole writer of that namespace);
//! encryption and I/O live in `secure-store`, reached only through the port
//! (ADR-12 Dependency Rule). Nothing leaves the device (BR-13).
//!
//! A corrupt, stale, or wrong-shape blob is **not** an error the caller must
//! handle: `load` silently falls back to the cold-start prior, so a
//! model-format change or a damaged record degrades to today's near-zero
//! warp rather than a failure.

use featherkey_contracts::{Namespace, SecureStore, StoreError};
use featherkey_nn::Mlp;

use crate::{TapWarp, INPUTS};

/// Storage key for the model's single blob under [`Namespace::TapWarpModel`].
/// Versioned so a future encoding change is detected rather than mis-parsed.
const BLOB_KEY: &[u8] = b"v1";

impl TapWarp {
    /// Encrypt-and-store both learned axis models through the injected store,
    /// as one atomic [`put`](SecureStore::put) under
    /// [`Namespace::TapWarpModel`].
    ///
    /// # Errors
    /// Propagates any [`StoreError`] from the store; this crate adds no error
    /// of its own on the write path.
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), StoreError> {
        let dx = self.dx.to_bytes();
        let dy = self.dy.to_bytes();
        let mut blob = Vec::with_capacity(4 + dx.len() + dy.len());
        blob.extend_from_slice(&(dx.len() as u32).to_le_bytes());
        blob.extend_from_slice(&dx);
        blob.extend_from_slice(&dy);
        store.put(Namespace::TapWarpModel, BLOB_KEY, &blob)
    }

    /// Load a model previously written by [`persist`](Self::persist), falling
    /// back to the cold-start prior (see [`from_prior`](Self::from_prior))
    /// when nothing is stored **or** the stored blob is corrupt/wrong-shape.
    ///
    /// A corrupt, old-format, or wrong-shape blob is not surfaced as an
    /// error: it degrades to the prior (today's near-zero warp), so a format
    /// or feature-count change never breaks tap decoding. Only a
    /// [`StoreError`] from the store's own `get` propagates.
    ///
    /// # Errors
    /// Returns the store's [`StoreError`] on a backend/crypto failure.
    pub fn load(store: &impl SecureStore) -> Result<Self, StoreError> {
        let Some(bytes) = store.get(Namespace::TapWarpModel, BLOB_KEY)? else {
            return Ok(Self::from_prior());
        };
        Ok(decode(&bytes).unwrap_or_else(Self::from_prior))
    }
}

/// Parse the `[u32 dx_len][dx][dy]` blob; any shape/format problem ⇒ `None`
/// so the caller falls back to the prior (never an error the caller must
/// handle). Panic-free by construction: `split_first_chunk`/`split_at_checked`
/// return `Option`, never index-panic.
fn decode(bytes: &[u8]) -> Option<TapWarp> {
    let (len_bytes, rest) = bytes.split_first_chunk::<4>()?;
    let dx_len = u32::from_le_bytes(*len_bytes) as usize;
    let (dx_b, dy_b) = rest.split_at_checked(dx_len)?;
    let dx = Mlp::from_bytes(dx_b)
        .ok()
        .filter(|m| m.inputs() == INPUTS)?;
    let dy = Mlp::from_bytes(dy_b)
        .ok()
        .filter(|m| m.inputs() == INPUTS)?;
    Some(TapWarp { dx, dy })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::{TapWarp, WARP_LR};
    use featherkey_contracts::{Namespace, SecureStore, StoreError};
    use std::cell::RefCell;
    use std::collections::HashMap;

    type StoreData = HashMap<(String, Vec<u8>), Vec<u8>>;

    #[derive(Default)]
    struct InMemoryStore {
        data: RefCell<StoreData>,
    }
    impl SecureStore for InMemoryStore {
        fn put(&self, ns: Namespace, key: &[u8], val: &[u8]) -> Result<(), StoreError> {
            self.data
                .borrow_mut()
                .insert((ns.as_str().to_owned(), key.to_vec()), val.to_vec());
            Ok(())
        }
        fn get(&self, ns: Namespace, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
            Ok(self
                .data
                .borrow()
                .get(&(ns.as_str().to_owned(), key.to_vec()))
                .cloned())
        }
    }

    #[test]
    fn round_trips_through_the_store() {
        let store = InMemoryStore::default();
        let mut w = TapWarp::from_prior();
        for _ in 0..200 {
            w.reinforce(0.3, -0.2, -18.0, 6.0, WARP_LR);
        }
        w.persist(&store).expect("persist");
        let back = TapWarp::load(&store).expect("load");
        let (a, b) = w.warp(0.3, -0.2);
        let (c, d) = back.warp(0.3, -0.2);
        assert!((a - c).abs() < 1e-6 && (b - d).abs() < 1e-6);
    }

    #[test]
    fn absent_or_corrupt_blob_falls_back_to_prior() {
        let store = InMemoryStore::default();
        let w = TapWarp::load(&store).expect("absent -> prior");
        assert!(w.warp(0.5, 0.5).0.abs() < 0.05);
        store
            .put(Namespace::TapWarpModel, b"v1", b"garbage")
            .unwrap();
        let w2 = TapWarp::load(&store).expect("corrupt -> prior, never Err");
        assert!(w2.warp(0.5, 0.5).0.abs() < 0.05);
    }

    #[test]
    fn a_valid_but_wrong_shape_blob_falls_back_to_prior() {
        // A well-formed blob whose inner MLPs have the WRONG input width (3, not 2)
        // must degrade to the prior — this is the only test that exercises the
        // `inputs() == INPUTS` guard (the "garbage" case fails earlier in
        // from_bytes), so it closes the coverage on that branch.
        use featherkey_nn::Mlp;
        let store = InMemoryStore::default();
        let three_in = Mlp::with_weights(vec![0.0; 6], vec![0.0, 0.0], vec![0.0, 0.0], 0.0, 3, 2);
        let inner = three_in.to_bytes();
        let mut blob = (inner.len() as u32).to_le_bytes().to_vec();
        blob.extend_from_slice(&inner);
        blob.extend_from_slice(&inner);
        store.put(Namespace::TapWarpModel, b"v1", &blob).unwrap();
        let w = TapWarp::load(&store).expect("wrong-shape -> prior, never Err");
        assert!(w.warp(0.5, 0.5).0.abs() < 0.05);
    }
}
