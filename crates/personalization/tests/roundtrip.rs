//! Integration tests: the `Personalization` model round-trips through a fake
//! `SecureStore` and propagates store errors as values (never panics).
//!
//! The fake store lives here (test code) — the library depends only on the
//! `SecureStore` *port*, never a concrete adapter (ADR-12 Dependency Rule).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::collections::HashMap;

use featherkey_contracts::{Namespace, SecureStore, StoreError};
use featherkey_personalization::Personalization;

/// An in-memory `SecureStore` fake keyed by `(namespace, key)`.
///
/// Optionally fails every `put`/`get` to exercise the library's error paths,
/// or returns arbitrary raw bytes to exercise decode failures.
type Records = HashMap<(Namespace, Vec<u8>), Vec<u8>>;

#[derive(Default)]
struct FakeStore {
    map: RefCell<Records>,
    fail_put: bool,
    fail_get: bool,
}

impl FakeStore {
    fn new() -> Self {
        Self::default()
    }
    fn failing_put() -> Self {
        Self { fail_put: true, ..Self::default() }
    }
    fn failing_get() -> Self {
        Self { fail_get: true, ..Self::default() }
    }
    /// Seed a raw value directly (bypasses `persist`) to drive decode paths.
    fn seed(&self, ns: Namespace, key: &[u8], val: &[u8]) {
        self.map.borrow_mut().insert((ns, key.to_vec()), val.to_vec());
    }
}

impl SecureStore for FakeStore {
    fn put(&self, ns: Namespace, key: &[u8], val: &[u8]) -> Result<(), StoreError> {
        if self.fail_put {
            return Err(StoreError::Backend);
        }
        self.map.borrow_mut().insert((ns, key.to_vec()), val.to_vec());
        Ok(())
    }
    fn get(&self, ns: Namespace, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        if self.fail_get {
            return Err(StoreError::Crypto);
        }
        Ok(self.map.borrow().get(&(ns, key.to_vec())).cloned())
    }
}

#[test]
fn persisted_state_round_trips_through_the_store() {
    let mut p = Personalization::new();
    p.observe("hello");
    p.observe("hello");
    p.observe("world");
    p.whitelist("acme");

    let store = FakeStore::new();
    p.persist(&store).expect("persist succeeds");

    let loaded = Personalization::load(&store).expect("load succeeds");
    assert_eq!(loaded.frequency("hello"), 2);
    assert_eq!(loaded.frequency("world"), 1);
    assert!(loaded.is_known("hello"));
    assert!(loaded.is_known("acme"));
    assert!(!loaded.is_known("unseen"));
}

#[test]
fn load_from_empty_store_yields_a_fresh_model() {
    let store = FakeStore::new();
    let p = Personalization::load(&store).expect("empty store loads clean");
    assert_eq!(p.frequency("anything"), 0);
    assert!(!p.is_known("anything"));
}

#[test]
fn persist_propagates_a_store_backend_error() {
    let mut p = Personalization::new();
    p.observe("word");
    let store = FakeStore::failing_put();
    assert_eq!(p.persist(&store), Err(StoreError::Backend));
}

#[test]
fn load_propagates_a_store_crypto_error() {
    let store = FakeStore::failing_get();
    assert_eq!(Personalization::load(&store).err(), Some(StoreError::Crypto));
}

#[test]
fn load_reports_corrupt_dictionary_bytes_as_backend_error() {
    let store = FakeStore::new();
    // Non-UTF-8 bytes under the dictionary namespace: corruption, not a value.
    store.seed(Namespace::PersonalLm, b"v1", &[0xff, 0xfe]);
    assert_eq!(Personalization::load(&store).err(), Some(StoreError::Backend));
}

#[test]
fn load_reports_corrupt_whitelist_bytes_as_backend_error() {
    let store = FakeStore::new();
    store.seed(Namespace::UserDict, b"v1", &[0xff, 0xfe]);
    assert_eq!(Personalization::load(&store).err(), Some(StoreError::Backend));
}

#[test]
fn whitelist_persists_even_with_an_empty_frequency_dictionary() {
    let mut p = Personalization::new();
    p.whitelist("brandname");
    let store = FakeStore::new();
    p.persist(&store).expect("persist");

    let loaded = Personalization::load(&store).expect("load");
    assert!(loaded.is_known("brandname"));
    assert_eq!(loaded.frequency("brandname"), 0);
}
