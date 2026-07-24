//! `LearnFromInput` + `ManageUserDictionary` use-cases (ARCH §9.1) and
//! persistence, wired to the `SecureStore` port.
//!
//! # E-2 — the sensitive-context gate (BR-26)
//! `learn_word` and `observe_tap` are the *only* paths that fold observed input
//! into learned state, and each consults [`SensitivityPolicy`] **first**. A
//! sensitive field short-circuits before any `observe`, so a password/OTP
//! keystroke can never reach [`Personalization`] or [`TouchModel`]. This is the
//! ordering the `tests/e2_sensitive_ordering.rs` property test pins down.
//!
//! Explicit dictionary edits (`add_to_dictionary`) are deliberate user actions,
//! not passive learning, so they are intentionally *not* gated — the user asked
//! for that word to be remembered.

use featherkey_contracts::{SecureStore, SensitiveContextSource};
use featherkey_kernel::KeyId;
use featherkey_personalization::Personalization;
use featherkey_touch_model::TouchModel;

use crate::error::FeatherKeyError;
use crate::FeatherKeyCore;

impl FeatherKeyCore {
    /// Fold a committed `word` into the learned vocabulary — **unless** `field`
    /// is sensitive, in which case the word is dropped unlearned (E-2, BR-26).
    pub fn learn_word(&mut self, word: &str, field: &dyn SensitiveContextSource) {
        if self.sensitivity.should_suppress(field) {
            return;
        }
        self.personalization.observe(word);
    }

    /// Fold one tap's offset `(dx, dy)` for `key` into the per-user tap model —
    /// **unless** `field` is sensitive, in which case the tap is dropped
    /// unlearned (E-2, BR-26).
    ///
    /// # Errors
    /// [`FeatherKeyError::TouchModel`] if the offset is non-finite (the model is
    /// left unchanged).
    pub fn observe_tap(
        &mut self,
        key: char,
        dx: f32,
        dy: f32,
        field: &dyn SensitiveContextSource,
    ) -> Result<(), FeatherKeyError> {
        if self.sensitivity.should_suppress(field) {
            return Ok(());
        }
        self.touch_model.observe(KeyId(key), dx, dy)?;
        Ok(())
    }

    /// Add `word` to the user dictionary as always-correct (ARCH §9.1
    /// `ManageUserDictionary`). A deliberate user action, so it is not gated by
    /// field sensitivity.
    pub fn add_to_dictionary(&mut self, word: &str) {
        self.personalization.whitelist(word);
    }

    /// Whether `word` is known — learned by frequency or whitelisted.
    #[must_use]
    pub fn knows_word(&self, word: &str) -> bool {
        self.personalization.is_known(word)
    }

    /// How many times `word` has been observed (0 if never or only whitelisted).
    #[must_use]
    pub fn word_frequency(&self, word: &str) -> u32 {
        self.personalization.frequency(word)
    }

    /// Encrypt and persist all learned state — the vocabulary *and* the per-user
    /// tap-geometry model — through the `SecureStore` port (the `secure-store`
    /// adapter at the composition root), so both survive across sessions.
    ///
    /// # Errors
    /// [`FeatherKeyError::Store`] if the backend or crypto layer fails.
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), FeatherKeyError> {
        self.personalization.persist(store)?;
        self.touch_model.persist(store)?;
        Ok(())
    }

    /// Reload all learned state — vocabulary and tap model — from the
    /// `SecureStore`, replacing the in-memory models. An absent blob restores an
    /// empty/unbiased model (first run).
    ///
    /// # Errors
    /// [`FeatherKeyError::Store`] if the backend or crypto layer fails.
    pub fn restore(&mut self, store: &impl SecureStore) -> Result<(), FeatherKeyError> {
        self.personalization = Personalization::load(store)?;
        self.touch_model = TouchModel::load(store)?;
        Ok(())
    }
}
