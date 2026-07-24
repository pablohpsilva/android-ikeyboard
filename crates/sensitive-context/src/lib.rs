//! The BR-26 gate: decide when to suppress learning/prediction for a field.
//!
//! Single responsibility (SEDD §5.2): this crate is the *one* place that turns a
//! [`SensitiveContextSource`] reading into a suppress/allow decision. It holds no
//! state and performs no I/O — it is a pure predicate over the port.
//!
//! **E-2 ordering invariant**: the composition root MUST consult this gate
//! BEFORE any learning/prediction runs, so password fields structurally cannot be
//! learned (BR-26). Because suppression is decided up front rather than filtered
//! after the fact, a sensitive field never reaches the learner or predictor at
//! all — there is no code path that could persist a keystroke typed into a
//! password box.

#![no_std]

use featherkey_contracts::SensitiveContextSource;

/// The BR-26 suppression gate.
///
/// Stateless by construction: the decision depends only on the field the
/// [`SensitiveContextSource`] describes, never on history, so the same source
/// always yields the same verdict (referential transparency, SEDD §5.4).
#[derive(Debug, Clone, Copy, Default)]
pub struct SensitivityPolicy;

impl SensitivityPolicy {
    /// Construct the policy. There is nothing to configure — the gate is a pure
    /// function of the source — but a constructor keeps call sites uniform with
    /// the other domain types.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// `true` when learning/prediction MUST be suppressed for the field `src`
    /// describes (i.e. the field is sensitive: password, OTP, …).
    ///
    /// The composition root calls this BEFORE invoking any learner or predictor
    /// (the E-2 ordering invariant); a `true` result means the keystroke is
    /// dropped before it can be observed, so sensitive input structurally cannot
    /// be learned (BR-26).
    #[must_use]
    pub fn should_suppress(&self, src: &dyn SensitiveContextSource) -> bool {
        src.is_sensitive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source that reports a sensitive field (e.g. a password box).
    struct Sensitive;
    impl SensitiveContextSource for Sensitive {
        fn is_sensitive(&self) -> bool {
            true
        }
    }

    /// A source that reports an ordinary field (e.g. a chat message box).
    struct Ordinary;
    impl SensitiveContextSource for Ordinary {
        fn is_sensitive(&self) -> bool {
            false
        }
    }

    #[test]
    fn suppresses_learning_for_a_sensitive_field() {
        let policy = SensitivityPolicy::new();
        assert!(policy.should_suppress(&Sensitive));
    }

    #[test]
    fn allows_learning_for_an_ordinary_field() {
        let policy = SensitivityPolicy::new();
        assert!(!policy.should_suppress(&Ordinary));
    }

    #[test]
    fn default_and_new_agree() {
        // Default is the same stateless gate as `new`; both must decide alike so
        // construction style never changes the verdict.
        let default_policy: SensitivityPolicy = Default::default();
        assert_eq!(
            default_policy.should_suppress(&Sensitive),
            SensitivityPolicy::new().should_suppress(&Sensitive),
        );
        assert_eq!(
            default_policy.should_suppress(&Ordinary),
            SensitivityPolicy::new().should_suppress(&Ordinary),
        );
    }
}
