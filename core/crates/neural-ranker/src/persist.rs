//! Encrypted self-persistence for [`NeuralRanker`]. The learned MLP is written
//! as one blob under [`Namespace::RankerModel`] through the injected
//! [`SecureStore`] port (the sole writer of that namespace); encryption and I/O
//! live in `secure-store`, reached only through the port (ADR-12 Dependency
//! Rule). Nothing leaves the device (BR-13).
//!
//! A corrupt or stale blob is **not** an error the caller must handle: `load`
//! silently falls back to the cold-start prior, so a model-format change or a
//! damaged record degrades to today's linear ranking rather than a failure.

use featherkey_contracts::{Namespace, SecureStore, StoreError};
use featherkey_nn::Mlp;

use crate::{NeuralRanker, INPUTS};

/// Storage key for the model's single blob under [`Namespace::RankerModel`].
/// Versioned so a future encoding change is detected rather than mis-parsed.
const BLOB_KEY: &[u8] = b"v1";

impl NeuralRanker {
    /// Encrypt-and-store the learned model through the injected store, as one
    /// atomic [`put`](SecureStore::put) under [`Namespace::RankerModel`].
    ///
    /// # Errors
    /// Propagates any [`StoreError`] from the store; this crate adds no error of
    /// its own on the write path.
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), StoreError> {
        store.put(Namespace::RankerModel, BLOB_KEY, &self.mlp.to_bytes())
    }

    /// Load a model previously written by [`persist`](Self::persist), falling
    /// back to the cold-start `prior` when nothing is stored **or** the stored
    /// blob is corrupt/stale.
    ///
    /// A corrupt, old-format, **or wrong-shape** blob is not surfaced as an error:
    /// it degrades to the prior (today's linear ranking), so a format or feature-
    /// count change never breaks ranking. Only a [`StoreError`] from the store's
    /// own `get` propagates.
    ///
    /// A blob whose [`inputs`](Mlp::inputs) count differs from [`INPUTS`] is
    /// rejected before adoption: scoring it against the `INPUTS`-wide feature
    /// vector would read truncated or misaligned features, so it falls back to the
    /// prior instead.
    ///
    /// # Errors
    /// Returns the store's [`StoreError`] on a backend/crypto failure.
    pub fn load(store: &impl SecureStore, prior: &[f32; INPUTS]) -> Result<Self, StoreError> {
        let mlp = match store.get(Namespace::RankerModel, BLOB_KEY)? {
            Some(bytes) => match Mlp::from_bytes(&bytes) {
                Ok(mlp) if mlp.inputs() == INPUTS => mlp,
                Ok(_) | Err(_) => Self::from_prior(prior).mlp,
            },
            None => return Ok(Self::from_prior(prior)),
        };
        Ok(Self { mlp })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::RankFeatures;
    use std::cell::RefCell;
    use std::collections::HashMap;

    const PRIOR: [f32; INPUTS] = [1.0, 1.0, 0.2, 0.0, 1.0, -1.0, 0.35, 0.0];

    /// `(namespace, key) -> value` backing map for the test store.
    type StoreData = HashMap<(String, Vec<u8>), Vec<u8>>;

    /// A minimal in-memory [`SecureStore`] for exercising persist/load without
    /// the real encrypted redb adapter.
    #[derive(Default)]
    struct FakeStore {
        data: RefCell<StoreData>,
    }
    impl SecureStore for FakeStore {
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

    fn feat(positional: f32) -> RankFeatures {
        RankFeatures {
            positional,
            ln_momentum: 0.3,
            is_lexicon: 1.0,
            is_device: 0.0,
            correction_promote: 0.2,
            correction_demote: -0.1,
            spatial: 0.4,
        }
    }

    #[test]
    fn persist_then_load_is_identity() {
        let store = FakeStore::default();
        let mut r = NeuralRanker::from_prior(&PRIOR);
        let strong = feat(0.0);
        let weak = feat(-1.4);
        for _ in 0..20 {
            r.reinforce(&[strong.clone(), weak.clone()], 1, 0.05);
        }
        r.persist(&store).unwrap();

        let loaded = NeuralRanker::load(&store, &PRIOR).unwrap();
        for p in [-2.0_f32, -1.4, -0.5, 0.0, 0.7] {
            let f = feat(p);
            assert_eq!(loaded.score(&f), r.score(&f));
        }
    }

    #[test]
    fn load_falls_back_to_prior_on_corrupt_blob() {
        let store = FakeStore::default();
        store
            .put(Namespace::RankerModel, BLOB_KEY, b"not a model")
            .unwrap();

        let loaded = NeuralRanker::load(&store, &PRIOR).unwrap();
        let prior = NeuralRanker::from_prior(&PRIOR);
        for p in [-2.0_f32, -1.0, 0.0, 0.5] {
            let f = feat(p);
            assert_eq!(loaded.score(&f), prior.score(&f));
        }
    }

    #[test]
    fn load_falls_back_to_prior_on_wrong_shape_blob() {
        // A stored blob that parses cleanly but has the wrong input count (a
        // different feature layout) must be rejected, not adopted: scoring it
        // against the INPUTS-wide vector would read misaligned features. It
        // degrades to the prior without surfacing an error.
        let store = FakeStore::default();
        let wrong_shape = Mlp::from_linear(&[1.0, 2.0], 0.0, 1.0, 64.0);
        assert_ne!(wrong_shape.inputs(), INPUTS);
        store
            .put(Namespace::RankerModel, BLOB_KEY, &wrong_shape.to_bytes())
            .unwrap();

        let loaded = NeuralRanker::load(&store, &PRIOR).unwrap();
        let prior = NeuralRanker::from_prior(&PRIOR);
        for p in [-2.0_f32, -1.0, 0.0, 0.5] {
            let f = feat(p);
            assert_eq!(loaded.score(&f), prior.score(&f));
        }
    }

    #[test]
    fn load_from_empty_store_yields_prior() {
        let store = FakeStore::default();
        let loaded = NeuralRanker::load(&store, &PRIOR).unwrap();
        let prior = NeuralRanker::from_prior(&PRIOR);
        for p in [-2.0_f32, -1.0, 0.0, 0.5] {
            let f = feat(p);
            assert_eq!(loaded.score(&f), prior.score(&f));
        }
    }

    #[test]
    fn a_store_error_from_get_propagates() {
        struct FailingStore;
        impl SecureStore for FailingStore {
            fn put(&self, _: Namespace, _: &[u8], _: &[u8]) -> Result<(), StoreError> {
                Ok(())
            }
            fn get(&self, _: Namespace, _: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
                Err(StoreError::Backend)
            }
        }
        let err = NeuralRanker::load(&FailingStore, &PRIOR).unwrap_err();
        assert_eq!(err, StoreError::Backend);
    }

    #[test]
    fn a_store_error_from_put_propagates() {
        struct FailingStore;
        impl SecureStore for FailingStore {
            fn put(&self, _: Namespace, _: &[u8], _: &[u8]) -> Result<(), StoreError> {
                Err(StoreError::Crypto)
            }
            fn get(&self, _: Namespace, _: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
                Ok(None)
            }
        }
        let r = NeuralRanker::from_prior(&PRIOR);
        assert_eq!(r.persist(&FailingStore).unwrap_err(), StoreError::Crypto);
    }
}
