//! Encrypted persistence of personal data; implements the `SecureStore` port.
//!
//! This is the *only* component that persists or encrypts personal data
//! (SEDD §5.4 boundary invariant, ADR-14). It backs the [`SecureStore`] port
//! from `featherkey-contracts` with a [`redb`] key/value database whose values
//! are sealed with AES-256-GCM.
//!
//! # Threat model (BR-8, BR-23, BR-62)
//! Data at rest is authenticated ciphertext: every value is stored as
//! `nonce || ciphertext` where the 96-bit nonce is freshly random per write.
//! A wrong key or any tampering with the stored bytes fails the GCM tag check
//! and surfaces as [`StoreError::Crypto`] rather than returning forged
//! plaintext. Storage-engine failures surface as [`StoreError::Backend`]. A
//! read of an absent key (or a namespace never written) is `Ok(None)`.
//!
//! Every blob is additionally *bound to its `(namespace, key)` location*
//! (BR-62 positional integrity): the AES-GCM associated data is
//! `namespace-name || record-key`. Because that binding is authenticated but
//! not stored, an attacker with disk access who relocates a valid blob to a
//! different namespace or key cannot decrypt it — the GCM tag check fails and
//! surfaces as [`StoreError::Crypto`].
//!
//! The crate is host-testable core: it names no Android/JNI types and runs
//! fully offline. The composition root (`platform-services`) supplies the real
//! device key later; tests inject a fixed key.

use std::path::Path;

use aes_gcm::aead::{Aead, Generate, Nonce, Payload};
use aes_gcm::{Aes256Gcm, Key, KeyInit};
use featherkey_contracts::{Namespace, SecureStore, StoreError};
use redb::{Database, TableDefinition, TableError};
use zeroize::Zeroizing;

/// Length of the AES-GCM nonce, in bytes (96 bits, the standard GCM nonce).
const NONCE_LEN: usize = 12;

/// The redb key/value type for every namespace table: opaque byte keys mapped
/// to opaque `nonce || ciphertext` blobs.
type Blobs = TableDefinition<'static, &'static [u8], &'static [u8]>;

/// An encrypted [`SecureStore`] backed by a redb database.
///
/// Construct with [`RedbSecureStore::open`]. The 32-byte key is held in a
/// [`Zeroizing`] buffer so it is wiped from memory on drop (BR-62), is never
/// written to disk, and is redacted from `Debug` output.
pub struct RedbSecureStore {
    db: Database,
    key: Zeroizing<[u8; 32]>,
}

impl core::fmt::Debug for RedbSecureStore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never render the key material.
        f.debug_struct("RedbSecureStore")
            .field("key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl RedbSecureStore {
    /// Open (creating if absent) the redb database at `path`, sealing values
    /// with the supplied 32-byte AES-256 key.
    ///
    /// # Errors
    /// [`StoreError::Backend`] if the database cannot be opened or created.
    pub fn open(path: impl AsRef<Path>, key: [u8; 32]) -> Result<Self, StoreError> {
        let db = Database::create(path).map_err(|_| StoreError::Backend)?;
        Ok(Self {
            db,
            key: Zeroizing::new(key),
        })
    }

    /// Build the AES-256-GCM cipher from the stored key. Infallible: a 32-byte
    /// array is exactly a `Key<Aes256Gcm>`, so no length check can fail.
    fn cipher(&self) -> Aes256Gcm {
        let key: Key<Aes256Gcm> = (*self.key).into();
        Aes256Gcm::new(&key)
    }

    /// The AES-GCM associated data that binds a blob to its `(ns, key)` slot:
    /// the namespace's stable table name followed by the record key (BR-62).
    /// This data is authenticated but never persisted, so a relocated blob
    /// authenticates against different bytes and fails the tag check.
    fn location_aad(ns: Namespace, key: &[u8]) -> Vec<u8> {
        // Length-prefix the namespace so the `namespace || key` boundary is
        // unambiguous: without it, two distinct (ns, key) pairs could in
        // principle produce identical AAD bytes if one namespace name were a
        // prefix of another. The length prefix makes the binding injective.
        let name = ns.as_str().as_bytes();
        debug_assert!(name.len() <= u8::MAX as usize);
        let mut aad = Vec::with_capacity(1 + name.len() + key.len());
        aad.push(name.len() as u8);
        aad.extend_from_slice(name);
        aad.extend_from_slice(key);
        aad
    }

    /// Encrypt `plaintext` into a `nonce || ciphertext` blob with a fresh
    /// random 96-bit nonce, authenticating `aad` (the `(ns, key)` location) so
    /// the blob is cryptographically bound to where it is stored.
    ///
    /// # Errors
    /// [`StoreError::Crypto`] if encryption fails.
    fn seal(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, StoreError> {
        let nonce = Nonce::<Aes256Gcm>::generate();
        let ciphertext = self
            .cipher()
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| StoreError::Crypto)?;
        let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        blob.extend_from_slice(nonce.as_ref());
        blob.extend_from_slice(&ciphertext);
        Ok(blob)
    }

    /// Decrypt a `nonce || ciphertext` blob back into plaintext, requiring the
    /// same `aad` (the `(ns, key)` location) that was sealed in.
    ///
    /// # Errors
    /// [`StoreError::Crypto`] if the blob is truncated, the key is wrong, the
    /// `aad` (location) differs, or the ciphertext was tampered with (GCM tag
    /// mismatch).
    fn open_blob(&self, blob: &[u8], aad: &[u8]) -> Result<Vec<u8>, StoreError> {
        let (nonce_bytes, ciphertext) = blob
            .split_first_chunk::<NONCE_LEN>()
            .ok_or(StoreError::Crypto)?;
        let nonce = Nonce::<Aes256Gcm>::from(*nonce_bytes);
        self.cipher()
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| StoreError::Crypto)
    }
}

impl SecureStore for RedbSecureStore {
    fn put(&self, ns: Namespace, key: &[u8], val: &[u8]) -> Result<(), StoreError> {
        let blob = self.seal(val, &Self::location_aad(ns, key))?;
        let def: Blobs = TableDefinition::new(ns.as_str());
        let txn = self.db.begin_write().map_err(|_| StoreError::Backend)?;
        {
            let mut table = txn.open_table(def).map_err(|_| StoreError::Backend)?;
            table
                .insert(key, blob.as_slice())
                .map_err(|_| StoreError::Backend)?;
        }
        txn.commit().map_err(|_| StoreError::Backend)?;
        Ok(())
    }

    fn get(&self, ns: Namespace, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        let def: Blobs = TableDefinition::new(ns.as_str());
        let txn = self.db.begin_read().map_err(|_| StoreError::Backend)?;
        let table = match txn.open_table(def) {
            Ok(table) => table,
            // A namespace nobody has written yet has no table; that is simply an
            // absent key, not a backend failure.
            Err(TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(_) => return Err(StoreError::Backend),
        };
        match table.get(key).map_err(|_| StoreError::Backend)? {
            Some(guard) => self
                .open_blob(guard.value(), &Self::location_aad(ns, key))
                .map(Some),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const KEY_A: [u8; 32] = [7u8; 32];

    // A convenient location AAD for tests that seal/open blobs directly.
    fn aad(ns: Namespace, key: &[u8]) -> Vec<u8> {
        RedbSecureStore::location_aad(ns, key)
    }

    fn store_in(dir: &TempDir, key: [u8; 32]) -> RedbSecureStore {
        RedbSecureStore::open(dir.path().join("s.redb"), key).expect("open store")
    }

    #[test]
    fn open_fails_backend_on_unopenable_path() {
        // Portable failure: a regular file cannot serve as a parent directory,
        // so opening a db "inside" one fails — no reliance on absolute paths
        // that differ across machines/CI.
        let dir = TempDir::new().expect("tempdir");
        let blocker = dir.path().join("not_a_dir");
        std::fs::write(&blocker, b"x").expect("write blocker file");
        let err = RedbSecureStore::open(blocker.join("s.redb"), KEY_A);
        assert_eq!(err.err(), Some(StoreError::Backend));
    }

    #[test]
    fn debug_redacts_the_key_material() {
        let dir = TempDir::new().expect("tempdir");
        let store = store_in(&dir, KEY_A);
        let shown = format!("{store:?}");
        assert!(shown.contains("<redacted>"), "debug should redact: {shown}");
        // A non-redacted debug would print the key array as `[7, 7, 7, ...]`.
        assert!(
            !shown.contains("7, 7"),
            "key bytes leaked into debug: {shown}"
        );
    }

    #[test]
    fn seal_uses_a_fresh_nonce_each_time() {
        let dir = TempDir::new().expect("tempdir");
        let store = store_in(&dir, KEY_A);
        let ad = aad(Namespace::UserDict, b"k");
        let a = store.seal(b"same", &ad).expect("seal a");
        let b = store.seal(b"same", &ad).expect("seal b");
        // Identical plaintext under a random nonce yields distinct blobs.
        assert_ne!(a, b);
        // Both nonce-prefixed blobs still decrypt to the original plaintext.
        assert_eq!(store.open_blob(&a, &ad).expect("open a"), b"same");
        assert_eq!(store.open_blob(&b, &ad).expect("open b"), b"same");
    }

    #[test]
    fn open_blob_rejects_a_blob_too_short_for_a_nonce() {
        let dir = TempDir::new().expect("tempdir");
        let store = store_in(&dir, KEY_A);
        assert_eq!(
            store.open_blob(&[0u8; NONCE_LEN - 1], &aad(Namespace::UserDict, b"k")),
            Err(StoreError::Crypto)
        );
    }

    #[test]
    fn open_blob_rejects_tampered_ciphertext() {
        let dir = TempDir::new().expect("tempdir");
        let store = store_in(&dir, KEY_A);
        let ad = aad(Namespace::UserDict, b"k");
        let mut blob = store.seal(b"secret", &ad).expect("seal");
        // Flip a bit in the ciphertext body; the GCM tag check must reject it.
        let last = blob.len() - 1;
        blob[last] ^= 0x01;
        assert_eq!(store.open_blob(&blob, &ad), Err(StoreError::Crypto));
    }

    // BR-62 positional integrity: a blob sealed under one (ns, key) location
    // cannot be opened under any other location — relocating it (different
    // namespace or different key) fails the AEAD tag check, while the original
    // location still round-trips.
    #[test]
    fn open_blob_rejects_a_relocated_blob() {
        let dir = TempDir::new().expect("tempdir");
        let store = store_in(&dir, KEY_A);

        let blob = store
            .seal(b"weights", &aad(Namespace::PersonalLm, b"k1"))
            .expect("seal");

        // Same key, different namespace -> Crypto.
        assert_eq!(
            store.open_blob(&blob, &aad(Namespace::UserDict, b"k1")),
            Err(StoreError::Crypto)
        );
        // Same namespace, different key -> Crypto.
        assert_eq!(
            store.open_blob(&blob, &aad(Namespace::PersonalLm, b"k2")),
            Err(StoreError::Crypto)
        );
        // The correct location still decrypts.
        assert_eq!(
            store
                .open_blob(&blob, &aad(Namespace::PersonalLm, b"k1"))
                .expect("open at correct location"),
            b"weights"
        );
    }
}
