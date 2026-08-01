//! Encrypted self-persistence for [`AutocorrectGate`]. The learned MLP is
//! written as one blob under [`Namespace::AutocorrectGate`] through the
//! injected [`SecureStore`] port (the sole writer of that namespace);
//! encryption and I/O live in `secure-store`, reached only through the port
//! (ADR-12 Dependency Rule). Nothing leaves the device (BR-13).
//!
//! A corrupt or stale blob is **not** an error the caller must handle: `load`
//! silently falls back to the cold-start prior, so a model-format change or a
//! damaged record degrades to today's base+floor autocorrect policy rather
//! than a failure.

use featherkey_contracts::{Namespace, SecureStore, StoreError};
use featherkey_nn::Mlp;

use crate::{AutocorrectGate, INPUTS};

/// Storage key for the model's single blob under [`Namespace::AutocorrectGate`].
/// Versioned so a future encoding change is detected rather than mis-parsed.
const BLOB_KEY: &[u8] = b"v1";

impl AutocorrectGate {
    /// Encrypt-and-store the learned model through the injected store, as one
    /// atomic [`put`](SecureStore::put) under [`Namespace::AutocorrectGate`].
    ///
    /// # Errors
    /// Propagates any [`StoreError`] from the store; this crate adds no error
    /// of its own on the write path.
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), StoreError> {
        store.put(Namespace::AutocorrectGate, BLOB_KEY, &self.nn.to_bytes())
    }

    /// Load a model previously written by [`persist`](Self::persist), falling
    /// back to the cold-start prior (see [`from_prior`](Self::from_prior))
    /// when nothing is stored **or** the stored blob is corrupt/stale.
    ///
    /// A corrupt, old-format, **or wrong-shape** blob is not surfaced as an
    /// error: it degrades to the prior (today's base+floor autocorrect
    /// policy), so a format or feature-count change never breaks autocorrect.
    /// Only a [`StoreError`] from the store's own `get` propagates.
    ///
    /// # Errors
    /// Returns the store's [`StoreError`] on a backend/crypto failure.
    pub fn load(store: &impl SecureStore) -> Result<Self, StoreError> {
        let nn = match store.get(Namespace::AutocorrectGate, BLOB_KEY)? {
            Some(bytes) => match Mlp::from_bytes(&bytes) {
                Ok(nn) if nn.inputs() == INPUTS => nn,
                Ok(_) | Err(_) => Self::from_prior().nn,
            },
            None => return Ok(Self::from_prior()),
        };
        Ok(Self { nn })
    }
}

/// A fixed [`GateFeatures`] probe used across the persist/load tests.
#[cfg(test)]
fn probe() -> crate::GateFeatures {
    crate::GateFeatures {
        edit_distance: 1.0,
        winner_confidence: 0.6,
        dict_rank_norm: 0.3,
        typed_len_norm: 0.4,
        momentum_weight: 0.1,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::GATE_LR;
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
        let mut g = AutocorrectGate::from_prior();
        let f = probe();
        for _ in 0..50 {
            g.reinforce(&f, 1.0, GATE_LR);
        }
        g.persist(&store).expect("persist");
        let back = AutocorrectGate::load(&store).expect("load");
        assert!((back.residual(&f) - g.residual(&f)).abs() < 1e-6);
    }

    #[test]
    fn absent_or_corrupt_blob_falls_back_to_prior() {
        let store = InMemoryStore::default();
        let g = AutocorrectGate::load(&store).expect("absent -> prior");
        // The cold-start prior's residual is ~0 (small, not exactly zero — the
        // centred unit pairs cancel to a small output; see `from_prior`).
        assert!(g.residual(&probe()).abs() < 0.05);
        store
            .put(Namespace::AutocorrectGate, b"v1", b"garbage")
            .unwrap();
        let g2 = AutocorrectGate::load(&store).expect("corrupt -> prior, never Err");
        assert!(g2.residual(&probe()).abs() < 0.05);
    }
}
