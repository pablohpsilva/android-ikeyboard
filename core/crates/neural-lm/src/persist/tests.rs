#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::collections::HashMap as Map;

use featherkey_contracts::{Namespace, SecureStore, StoreError};
use featherkey_nn::MlpMulti;

use crate::model::EMBED_LEN;
use crate::NextWordLm;

/// `(namespace, key) -> value` backing map for the test store.
type StoreData = Map<(String, Vec<u8>), Vec<u8>>;

/// A minimal in-memory `SecureStore` for exercising persist/load without the
/// real encrypted redb adapter, mirroring `neural-tap`'s and `context`'s
/// `MemStore` test doubles.
#[derive(Default)]
struct MemStore {
    data: RefCell<StoreData>,
}
impl SecureStore for MemStore {
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

/// The best-ranked word from a `rank_next` result, or `""` if empty.
fn top(ranked: &[(String, f32)]) -> &str {
    ranked.first().map(|(w, _)| w.as_str()).unwrap_or("")
}

#[test] // @BR-11 persistence
fn trained_model_survives_persist_then_load() {
    let store = MemStore::default();
    let mut lm = NextWordLm::new();
    for _ in 0..200 {
        lm.observe(&["going", "to"], "work");
    }
    lm.persist(&store).unwrap();
    let loaded = NextWordLm::load(&store).unwrap();
    assert_eq!(top(&loaded.rank_next(&["going", "to"], 5)), "work");
    assert_eq!(loaded.confidence(), lm.confidence());
}

#[test]
fn absent_or_corrupt_blob_loads_cold_start() {
    let store = MemStore::default();
    assert_eq!(NextWordLm::load(&store).unwrap().confidence(), 0.0); // absent
    store.put(Namespace::PersonalLm, b"lm_v1", &[0xff]).unwrap();
    assert_eq!(NextWordLm::load(&store).unwrap().confidence(), 0.0); // corrupt -> cold
}

#[test]
fn wrong_version_blob_loads_cold_start() {
    let store = MemStore::default();
    let mut lm = NextWordLm::new();
    lm.observe(&["going", "to"], "work");
    lm.persist(&store).unwrap();

    // Flip the version header on the already-persisted blob so a
    // future/older format is rejected outright rather than mis-parsed.
    let key = (Namespace::PersonalLm.as_str().to_owned(), b"lm_v1".to_vec());
    let mut bytes = store.data.borrow().get(&key).cloned().expect("persisted");
    bytes[0] = 0xFF;
    bytes[1] = 0xFF;
    store.data.borrow_mut().insert(key, bytes);

    assert_eq!(NextWordLm::load(&store).unwrap().confidence(), 0.0);
}

#[test]
fn wrong_shape_net_blob_loads_cold_start() {
    // A well-formed blob (right version, empty vocab, zero warmup, correctly
    // sized embed section) whose only defect is a `net` sub-blob with the
    // wrong shape — must degrade to cold-start exactly like the TapWarp
    // model's equivalent guard.
    let store = MemStore::default();
    let mut blob = Vec::new();
    blob.extend_from_slice(&1u16.to_le_bytes()); // version
    blob.extend_from_slice(&0u32.to_le_bytes()); // vocab_len = 0 (empty vocab)
    blob.extend_from_slice(&0u32.to_le_bytes()); // warmup
    for _ in 0..EMBED_LEN {
        blob.extend_from_slice(&0f32.to_le_bytes());
    }
    let bad_net = MlpMulti::with_weights(vec![0.0], vec![0.0], vec![0.0], vec![0.0], 1, 1, 1);
    blob.extend_from_slice(&bad_net.to_bytes());

    store.put(Namespace::PersonalLm, b"lm_v1", &blob).unwrap();
    assert_eq!(NextWordLm::load(&store).unwrap().confidence(), 0.0);
}
