//! Port traits: the interfaces domain crates depend on instead of adapters.
//!
//! Per ADR-12, every port (driven and driving) lives here so a domain crate can
//! depend on the *trait* while the concrete adapter stays invisible to it
//! (ARCH §3.2 Dependency Rule / DIP). This crate has no logic and depends only
//! on `kernel`.
//!
//! Only the ports expressible with today's types are defined. Ports whose
//! signatures need domain types that don't exist yet — `Predictor`
//! (`TypingContext`/`Suggestions`), `AutoCorrect` (`Token`/`Correction`),
//! `Personalization` (`TypingEvent`) — are added alongside the crates that
//! introduce those types (Waves 2–3), keeping this crate honest rather than
//! full of placeholder types.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

/// The persistence namespaces, one per data domain (SEDD §7.2). Kept here so the
/// `SecureStore` port is fully typed without depending on any writer crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Namespace {
    /// Per-user tap-geometry model (sole writer: `touch-model`, ADR-14).
    TouchModel,
    /// Lexical learning: user dictionary (sole writer: `personalization`).
    UserDict,
    /// Lexical learning: personal n-gram counts (sole writer: `personalization`).
    PersonalLm,
    /// Clipboard history (sole writer: `clipboard-core`).
    Clipboard,
}

impl Namespace {
    /// A stable string key for the namespace, used as the storage table name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Namespace::TouchModel => "touch_model",
            Namespace::UserDict => "user_dict",
            Namespace::PersonalLm => "personal_lm",
            Namespace::Clipboard => "clipboard",
        }
    }
}

/// Errors a [`SecureStore`] adapter may return. Errors are values, never panics
/// (SEDD §5.5 r3).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StoreError {
    /// The underlying storage engine failed (I/O, corruption).
    Backend,
    /// Encryption or decryption failed (bad key, tampered ciphertext).
    Crypto,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Backend => f.write_str("secure store backend failure"),
            StoreError::Crypto => f.write_str("secure store crypto failure"),
        }
    }
}

/// Driven port: the *only* component that persists/encrypts personal data
/// (`secure-store` implements it; SEDD §5.4 boundary invariant).
pub trait SecureStore {
    /// Encrypt and store `val` under `(ns, key)`.
    ///
    /// # Errors
    /// [`StoreError`] if the backend or crypto layer fails.
    fn put(&self, ns: Namespace, key: &[u8], val: &[u8]) -> Result<(), StoreError>;

    /// Fetch and decrypt the value at `(ns, key)`, or `None` if absent.
    ///
    /// # Errors
    /// [`StoreError`] if the backend or crypto layer fails.
    fn get(&self, ns: Namespace, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError>;
}

/// Driven port: reports whether the current editor field is sensitive (password,
/// OTP, …). The shell supplies this from `EditorInfo`; the composition root
/// consults it *before* any learning/prediction runs so password fields
/// structurally cannot be learned (BR-26, SEDD §5.4).
pub trait SensitiveContextSource {
    /// `true` if learning and prediction must be suppressed for this field.
    fn is_sensitive(&self) -> bool;
}

/// Driven port: a monotonic millisecond time source. Injecting time keeps
/// core logic (clipboard expiry, diagnostics timestamps) deterministic and
/// host-testable rather than reading the wall clock directly.
pub trait Clock {
    /// Milliseconds since an arbitrary but monotonic epoch.
    fn now_millis(&self) -> u64;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_keys_are_stable_and_distinct() {
        let all = [
            Namespace::TouchModel,
            Namespace::UserDict,
            Namespace::PersonalLm,
            Namespace::Clipboard,
        ];
        let keys: Vec<&str> = all.iter().map(|n| n.as_str()).collect();
        assert_eq!(
            keys,
            ["touch_model", "user_dict", "personal_lm", "clipboard"]
        );
        // Distinct table names — no two namespaces collide in storage.
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn store_error_displays_human_messages() {
        extern crate alloc;
        assert_eq!(
            alloc::format!("{}", StoreError::Backend),
            "secure store backend failure"
        );
        assert_eq!(
            alloc::format!("{}", StoreError::Crypto),
            "secure store crypto failure"
        );
    }

    // Stub adapters prove the port shapes are implementable and exercise the
    // trait methods (coverage of the contract surface).
    struct InMemory;
    impl SecureStore for InMemory {
        fn put(&self, _ns: Namespace, _k: &[u8], _v: &[u8]) -> Result<(), StoreError> {
            Ok(())
        }
        fn get(&self, _ns: Namespace, _k: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
            Ok(None)
        }
    }

    struct NotSensitive;
    impl SensitiveContextSource for NotSensitive {
        fn is_sensitive(&self) -> bool {
            false
        }
    }

    struct FixedClock(u64);
    impl Clock for FixedClock {
        fn now_millis(&self) -> u64 {
            self.0
        }
    }

    #[test]
    fn ports_are_implementable_and_callable() {
        let store = InMemory;
        assert_eq!(store.put(Namespace::UserDict, b"k", b"v"), Ok(()));
        assert_eq!(store.get(Namespace::UserDict, b"k"), Ok(None));
        assert!(!NotSensitive.is_sensitive());
        assert_eq!(FixedClock(42).now_millis(), 42);
    }
}
