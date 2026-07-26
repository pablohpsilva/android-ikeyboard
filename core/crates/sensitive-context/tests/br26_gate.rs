//! BR-26 acceptance: the gate suppresses on a sensitive source and allows
//! otherwise, exercising `SensitivityPolicy` across the crate boundary (the
//! executable form of features/sensitive-context.feature).

use featherkey_contracts::SensitiveContextSource;
use featherkey_sensitive_context::SensitivityPolicy;

/// Stub standing in for the shell's password-field reading.
struct PasswordField;
impl SensitiveContextSource for PasswordField {
    fn is_sensitive(&self) -> bool {
        true
    }
}

/// Stub standing in for an ordinary text field.
struct MessageField;
impl SensitiveContextSource for MessageField {
    fn is_sensitive(&self) -> bool {
        false
    }
}

#[test]
fn suppresses_a_password_field() {
    let policy = SensitivityPolicy::new();
    assert!(
        policy.should_suppress(&PasswordField),
        "learning must be suppressed for a password field (BR-26)",
    );
}

#[test]
fn does_not_suppress_a_message_field() {
    let policy = SensitivityPolicy::new();
    assert!(
        !policy.should_suppress(&MessageField),
        "learning must run for an ordinary field",
    );
}
