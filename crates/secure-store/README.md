# featherkey-secure-store

## Its ONE job

Encrypt and persist all personal data — implements the `SecureStore` port; redb + AES-256-GCM.

## Layer

**adapter** (`[package.metadata.featherkey] layer = "adapter"`). It is the *only* component that persists or encrypts personal data (SEDD §5.4 boundary invariant, ADR-14).

## Ports

Implements the driven `SecureStore` port from `featherkey-contracts` — `put(ns, key, val)` and `get(ns, key)`. There is no `delete` or enumerate on the port surface today; that is deferred to a later version.

Dependencies (`[dependencies]`):
- `featherkey-contracts` — the port trait, `Namespace`, `StoreError`.
- `redb` 2.6 — embedded key/value database (one table per namespace).
- `aes-gcm` 0.11 — AES-256-GCM AEAD.
- `zeroize` 1.9 — wipes key material on drop.

Concrete type: `RedbSecureStore`, opened via `RedbSecureStore::open(path, key: [u8; 32])`. The composition root (`platform-services`) supplies the real device key; tests inject a fixed key.

## Invariants

- **Authenticated ciphertext at rest.** Every value is stored as `nonce || ciphertext`; a wrong key or any tampering fails the GCM tag check and surfaces as `StoreError::Crypto`, never forged plaintext.
- **Fresh per-write nonce.** A freshly random 96-bit nonce is generated on every write.
- **Positional integrity (BR-62).** The AES-GCM associated data is `len(namespace_name) || namespace_name || key`, binding each blob to its `(namespace, key)` slot; a relocated blob fails to decrypt. The AAD is authenticated but never persisted.
- **Key zeroization.** The 32-byte key lives in a `Zeroizing` buffer, is wiped on drop, is never written to disk, and is redacted from `Debug` output.
- **Namespace isolation.** Each `Namespace` is a separate redb table; the same key in two namespaces holds independent values.
- **Absent is not an error.** Reading a missing key — or a namespace whose table was never created — returns `Ok(None)`; storage-engine failures surface as `StoreError::Backend`.
- **Host-testable core.** Names no Android/JNI types and runs fully offline.

## Serves (BRs)

BR-8, BR-23, BR-62.

## Tests

Inline `#[cfg(test)]` unit tests in `src/lib.rs` (nonce freshness, tamper/short-blob rejection, relocation rejection, `Debug` redaction, backend-open failure) and behavioural round-trip tests in `tests/roundtrip.rs` (round-trip, overwrite, absent-key, wrong-key, reopen persistence, namespace isolation), bound to the `features/secure-store.feature` scenarios. No proptests.
