//! Port traits: the interfaces domain crates depend on instead of adapters.
//!
//! Per ADR-12, every port (driven and driving) lives here so a domain crate can
//! depend on the *trait* while the concrete adapter stays invisible to it
//! (ARCH §3.2 Dependency Rule / DIP). This crate has no logic and depends only
//! on `kernel`.
//!
//! Every port whose signature is expressible today is defined here — the driven
//! `SecureStore`, `SensitiveContextSource`, and `Clock`, and the driving
//! `Predictor` (`TypingContext`/`Suggestions`) and `AutoCorrect`
//! (`Token`/`Correction`). A `Personalization` port (over a `TypingEvent`) is
//! deliberately still deferred: it is added alongside the crate that introduces
//! its types rather than seeded here as a placeholder.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// The persistence namespaces, one per data domain (SEDD §7.2). Kept here so the
/// `SecureStore` port is fully typed without depending on any writer crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Namespace {
    /// Per-user tap-geometry model (sole writer: `touch-model`, ADR-14).
    TouchModel,
    /// The user's lexical model — learned words + whitelist, persisted as one
    /// atomic blob (sole writer: `personalization`).
    UserDict,
    /// Reserved for a future dedicated personal n-gram store; not currently
    /// written by any crate.
    PersonalLm,
    /// Clipboard history (sole writer: `clipboard-core`).
    Clipboard,
    /// Per-user correction signals — strip-pick prefs + unwanted words (sole
    /// writer: `corrections`).
    Corrections,
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
            Namespace::Corrections => "corrections",
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

/// The textual context around the token the user is typing: the text already
/// committed before it (for next-word/n-gram scoring and capitalization) and the
/// in-progress `prefix` being completed. Language handling is internal to the
/// implementation, so this port stays language-agnostic.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TypingContext {
    /// Committed text immediately before the current token.
    pub preceding: String,
    /// The in-progress token being typed (may be empty at a word boundary).
    pub prefix: String,
}

/// One ranked suggestion: a candidate word and an opaque score (higher is
/// better; the scale is the producer's private business).
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub word: String,
    pub score: u32,
}

/// Ranked suggestions, best first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Suggestions {
    pub items: Vec<Suggestion>,
}

/// Driving port: completions / next-word predictions for the current context
/// (SEDD §5.4). Statistical at MVP, neural behind the *same* trait in v1.x, so
/// callers never change when the engine is swapped (ADR-3).
pub trait Predictor {
    fn suggest(&self, ctx: &TypingContext) -> Suggestions;
}

/// A single typed token considered for correction.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub text: String,
}

/// The outcome of an autocorrect decision. `applied` is `false` when the token
/// was left unchanged — an exact/whitelisted word is never clobbered (BR-12).
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    /// The word to commit (equal to the input when `applied` is false).
    pub primary: String,
    /// Other plausible words the user may pick instead.
    pub alternatives: Vec<String>,
    /// Whether the primary differs from the typed token.
    pub applied: bool,
}

/// Driving port: decide whether/how to correct a token (SEDD §5.4). It MUST
/// never replace a word the user clearly intended — an exact dictionary match or
/// a whitelisted word is returned unchanged with `applied == false` (BR-12, the
/// no-clobber rule, verified by a property test in the `autocorrect` crate).
pub trait AutoCorrect {
    fn correct(&self, token: &Token, ctx: &TypingContext) -> Correction;
}

/// Where a candidate came from — used only to weight sources against each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Derived from a bundled per-language lexicon/freq list.
    Lexicon,
    /// Derived from the device spell-checker.
    Device,
}

/// One correction/suggestion candidate, tagged by language and by its rank
/// *within its own source and language* (0 = best). The ranker converts
/// `source_rank` to a commensurable score, so sources with different internal
/// scales combine cleanly.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub word: String,
    pub lang: String,
    pub source: Source,
    pub source_rank: u32,
}

/// A candidate after ranking, carrying its final blended score.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    pub word: String,
    pub lang: String,
    pub score: f64,
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
            Namespace::Corrections,
        ];
        let keys: Vec<&str> = all.iter().map(|n| n.as_str()).collect();
        assert_eq!(
            keys,
            [
                "touch_model",
                "user_dict",
                "personal_lm",
                "clipboard",
                "corrections"
            ]
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

    // Stub prediction/autocorrect adapters prove the Wave-3 port shapes are
    // implementable and their value types compose.
    struct EchoPredictor;
    impl Predictor for EchoPredictor {
        fn suggest(&self, ctx: &TypingContext) -> Suggestions {
            Suggestions {
                items: alloc::vec![Suggestion {
                    word: ctx.prefix.clone(),
                    score: 1
                }],
            }
        }
    }

    struct NoClobber;
    impl AutoCorrect for NoClobber {
        fn correct(&self, token: &Token, _ctx: &TypingContext) -> Correction {
            Correction {
                primary: token.text.clone(),
                alternatives: Vec::new(),
                applied: false,
            }
        }
    }

    #[test]
    fn prediction_and_autocorrect_ports_compose() {
        let ctx = TypingContext {
            preceding: String::new(),
            prefix: alloc::string::String::from("he"),
        };
        let s = EchoPredictor.suggest(&ctx);
        assert_eq!(s.items.len(), 1);
        assert_eq!(s.items[0].word, "he");
        assert_eq!(s.items[0].score, 1);

        let token = Token {
            text: alloc::string::String::from("cat"),
        };
        let c = NoClobber.correct(&token, &ctx);
        assert_eq!(c.primary, "cat");
        assert!(!c.applied);
        assert!(c.alternatives.is_empty());
        // Default TypingContext / Suggestions are usable.
        assert_eq!(TypingContext::default().prefix, "");
        assert!(Suggestions::default().items.is_empty());
    }

    #[test]
    fn candidate_is_constructible_and_comparable() {
        let a = Candidate {
            word: "hola".into(),
            lang: "es".into(),
            source: Source::Lexicon,
            source_rank: 0,
        };
        let b = a.clone();
        assert_eq!(a, b);
        assert_eq!(a.source, Source::Lexicon);
    }

    #[test]
    fn wave3_value_types_derive_expected_behaviour() {
        // Exercise Clone + PartialEq (== and !=) + Debug on every Wave-3 value
        // type so the derived surface consumers rely on is actually covered.
        let ctx = TypingContext {
            preceding: String::from("a"),
            prefix: String::from("b"),
        };
        assert_eq!(ctx, ctx.clone());
        assert_ne!(ctx, TypingContext::default());

        let sug = Suggestion {
            word: String::from("hi"),
            score: 3,
        };
        assert_eq!(sug, sug.clone());
        assert_ne!(
            sug,
            Suggestion {
                word: String::from("hi"),
                score: 4
            }
        );

        let sugs = Suggestions {
            items: alloc::vec![sug.clone()],
        };
        assert_eq!(sugs, sugs.clone());
        assert_ne!(sugs, Suggestions::default());

        let tok = Token {
            text: String::from("x"),
        };
        assert_eq!(tok, tok.clone());
        assert_ne!(
            tok,
            Token {
                text: String::from("y")
            }
        );

        let cor = Correction {
            primary: String::from("x"),
            alternatives: alloc::vec![String::from("y")],
            applied: true,
        };
        assert_eq!(cor, cor.clone());
        assert_ne!(
            cor,
            Correction {
                primary: String::from("x"),
                alternatives: Vec::new(),
                applied: true
            }
        );

        // Debug is usable on every type (non-empty, no panic).
        assert!(!alloc::format!("{ctx:?}{sug:?}{sugs:?}{tok:?}{cor:?}").is_empty());
    }
}
