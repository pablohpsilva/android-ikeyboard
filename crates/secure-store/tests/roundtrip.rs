//! Behavioural tests for the encrypted `SecureStore` adapter.
//!
//! These bind the Gherkin scenarios in features/secure-store.feature to
//! executable checks (BR-8, BR-23, BR-62): round-trip, absent-key, wrong-key,
//! and namespace isolation.

use featherkey_contracts::{Namespace, SecureStore, StoreError};
use featherkey_secure_store::RedbSecureStore;
use tempfile::TempDir;

const KEY_A: [u8; 32] = [0x11; 32];
const KEY_B: [u8; 32] = [0x22; 32];

fn open(dir: &TempDir, key: [u8; 32]) -> RedbSecureStore {
    RedbSecureStore::open(dir.path().join("store.redb"), key).expect("open store")
}

// BR-8: a value survives the encrypt/persist/decrypt round-trip unchanged.
#[test]
fn put_then_get_round_trips_the_value() {
    let dir = TempDir::new().expect("tempdir");
    let store = open(&dir, KEY_A);

    store
        .put(Namespace::UserDict, b"greeting", b"hello")
        .expect("put");
    let got = store.get(Namespace::UserDict, b"greeting").expect("get");

    assert_eq!(got, Some(b"hello".to_vec()));
}

// BR-8: overwriting a key returns the latest value.
#[test]
fn put_overwrites_the_previous_value() {
    let dir = TempDir::new().expect("tempdir");
    let store = open(&dir, KEY_A);

    store.put(Namespace::UserDict, b"k", b"first").expect("put1");
    store.put(Namespace::UserDict, b"k", b"second").expect("put2");

    assert_eq!(
        store.get(Namespace::UserDict, b"k").expect("get"),
        Some(b"second".to_vec())
    );
}

// BR-23: reading a key that was never written yields nothing (not an error),
// both for a namespace that exists and one never touched.
#[test]
fn get_of_absent_key_is_none() {
    let dir = TempDir::new().expect("tempdir");
    let store = open(&dir, KEY_A);

    // Namespace never written at all -> no table -> None.
    assert_eq!(store.get(Namespace::Clipboard, b"nope").expect("get"), None);

    // Namespace exists (has another key) but this key is absent -> None.
    store.put(Namespace::UserDict, b"present", b"v").expect("put");
    assert_eq!(
        store.get(Namespace::UserDict, b"missing").expect("get"),
        None
    );
}

// BR-62: a value written under one key cannot be decrypted with another; the
// GCM tag check fails and surfaces as a crypto error, never forged plaintext.
#[test]
fn get_with_wrong_key_is_a_crypto_error() {
    let dir = TempDir::new().expect("tempdir");
    open(&dir, KEY_A)
        .put(Namespace::PersonalLm, b"model", b"weights")
        .expect("put with key A");

    // Reopen the same database file with a different key.
    let attacker = open(&dir, KEY_B);
    assert_eq!(
        attacker.get(Namespace::PersonalLm, b"model"),
        Err(StoreError::Crypto)
    );
}

// BR-8: the correct key still decrypts after the store is dropped and reopened,
// proving the data (not just an in-memory cache) is what round-trips.
#[test]
fn value_persists_across_reopen_with_the_same_key() {
    let dir = TempDir::new().expect("tempdir");
    open(&dir, KEY_A)
        .put(Namespace::TouchModel, b"k", b"v")
        .expect("put");

    let reopened = open(&dir, KEY_A);
    assert_eq!(
        reopened.get(Namespace::TouchModel, b"k").expect("get"),
        Some(b"v".to_vec())
    );
}

// BR-8: namespaces are separate tables; the same key in two namespaces holds
// two independent values.
#[test]
fn namespaces_are_isolated() {
    let dir = TempDir::new().expect("tempdir");
    let store = open(&dir, KEY_A);

    store.put(Namespace::UserDict, b"k", b"in-dict").expect("put1");
    store
        .put(Namespace::Clipboard, b"k", b"in-clip")
        .expect("put2");

    assert_eq!(
        store.get(Namespace::UserDict, b"k").expect("get1"),
        Some(b"in-dict".to_vec())
    );
    assert_eq!(
        store.get(Namespace::Clipboard, b"k").expect("get2"),
        Some(b"in-clip".to_vec())
    );
}

// The four namespaces each behave as an independent store.
#[test]
fn every_namespace_round_trips_independently() {
    let dir = TempDir::new().expect("tempdir");
    let store = open(&dir, KEY_A);
    let namespaces = [
        Namespace::TouchModel,
        Namespace::UserDict,
        Namespace::PersonalLm,
        Namespace::Clipboard,
    ];

    for (i, ns) in namespaces.iter().enumerate() {
        let val = [i as u8; 4];
        store.put(*ns, b"key", &val).expect("put");
        assert_eq!(store.get(*ns, b"key").expect("get"), Some(val.to_vec()));
    }
}
