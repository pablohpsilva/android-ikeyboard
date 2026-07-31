//! Neural re-ranker persistence wiring (Task 10). Proves the composition root
//! writes the ranker's blob under [`Namespace::RankerModel`] as part of the core
//! `persist`, mirroring how `context`/`corrections` fan out. The ranker is not
//! yet consumed by ranking (Task 11), so these tests exercise the persistence
//! mechanics only. Score-level checks of the held/restored ranker live inline in
//! `lib.rs` (they need the crate-internal `neural_ranker()` seam).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use featherkey_core::{FeatherKeyCore, Namespace, RedbSecureStore, SecureStore};

fn core() -> FeatherKeyCore {
    FeatherKeyCore::new(vec![("en".to_owned(), vec!["cat".to_owned()])]).expect("valid core")
}

#[test]
fn persist_writes_a_ranker_blob() {
    // After a core `persist`, the ranker's versioned blob is present under the
    // RankerModel namespace — proof the fan-out reaches the neural ranker.
    let dir = tempfile::tempdir().unwrap();
    let store = RedbSecureStore::open(dir.path().join("store.redb"), [9u8; 32]).expect("open");
    let fk = core();
    fk.persist(&store).unwrap();
    assert!(
        store.get(Namespace::RankerModel, b"v1").unwrap().is_some(),
        "core.persist did not write the ranker blob under RankerModel"
    );
}
