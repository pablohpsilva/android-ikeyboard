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

use featherkey_context::Context;
use featherkey_contracts::{SecureStore, SensitiveContextSource};
use featherkey_corrections::Corrections;
use featherkey_kernel::KeyId;
use featherkey_personalization::Personalization;
use featherkey_touch_model::TouchModel;

use crate::error::FeatherKeyError;
use crate::FeatherKeyCore;

impl FeatherKeyCore {
    /// Fold a committed `word` (typed after `preceding`) into learned state —
    /// **unless** `field` is sensitive, in which case both the word and the
    /// `preceding -> word` transition are dropped unlearned (E-2, BR-26).
    ///
    /// The single gate covers *both* learners so a password/OTP keystroke can
    /// never reach [`Personalization`] **or** the next-word [`Context`] model.
    /// `preceding` may be empty (sentence start); the context model itself skips
    /// transitions with a too-short previous token.
    pub fn learn_word(&mut self, preceding: &str, word: &str, field: &dyn SensitiveContextSource) {
        if self.sensitivity.should_suppress(field) {
            return;
        }
        self.personalization.observe(word);
        self.context.record(preceding, word);
    }

    /// Record that, for the typed `prefix`, the user picked `picked` from the
    /// suggestion strip (a correction signal that can promote a lower-ranked but
    /// repeatedly-chosen completion) — **unless** `field` is sensitive (E-2,
    /// BR-26), in which case the signal is dropped.
    pub fn observe_strip_pick(
        &mut self,
        prefix: &str,
        picked: &str,
        field: &dyn SensitiveContextSource,
    ) {
        if self.sensitivity.should_suppress(field) {
            return;
        }
        self.corrections.note_pick(prefix, picked);
    }

    /// Record one low-weight `unwanted` signal for `word` (reverted/deleted right
    /// after being offered) — **unless** `field` is sensitive (E-2, BR-26).
    pub fn observe_delete_retype(&mut self, word: &str, field: &dyn SensitiveContextSource) {
        if self.sensitivity.should_suppress(field) {
            return;
        }
        self.corrections.note_unwanted(word);
    }

    /// Bulk-load pre-computed `(prev, next, count)` bigram transitions into the
    /// next-word model (migrating the legacy Kotlin `context.tsv`). Set-semantics,
    /// so re-running a migration is idempotent. Not gated: migration is a
    /// deliberate one-time import of the user's own prior data.
    pub fn import_context<I: IntoIterator<Item = (String, String, u32)>>(
        &mut self,
        transitions: I,
    ) {
        self.context.import(transitions);
    }

    /// Bulk-load pre-computed `(word, count)` learned frequencies into the
    /// personalization model (migrating the legacy Kotlin `usage.tsv`).
    /// Set-semantics, so re-running a migration is idempotent. Not gated: same
    /// rationale as [`import_context`] — a deliberate one-time import of the
    /// user's own prior data.
    pub fn import_frequencies<I: IntoIterator<Item = (String, u32)>>(&mut self, frequencies: I) {
        self.personalization.import(frequencies);
    }

    /// Every learned word paired with its observed frequency, for the shell's
    /// swipe/gesture decoder (which ranks gesture paths by learned usage).
    #[must_use]
    pub fn learned_frequencies(&self) -> Vec<(String, u32)> {
        self.personalization
            .frequencies()
            .iter()
            .map(|(w, c)| (w.clone(), *c))
            .collect()
    }

    /// Every observed key's learned tap-offset bias, as `(key, dx, dy)`, for the
    /// shell to re-centre gesture key positions before swipe decoding.
    #[must_use]
    pub fn tap_offsets(&self) -> Vec<(String, f32, f32)> {
        self.touch_model
            .offsets()
            .into_iter()
            .map(|(ch, dx, dy)| (ch.to_string(), dx, dy))
            .collect()
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

    /// The likeliest next words after `prev`, most-frequent first (inspection
    /// seam over the learned next-word model; mirrors [`Self::word_frequency`]).
    #[must_use]
    pub fn context_next_words(&self, prev: &str, limit: usize) -> Vec<String> {
        self.context.next_words(prev, limit)
    }

    /// How often `picked` was chosen from the strip for the typed `prefix`
    /// (inspection seam over the correction-signal model).
    #[must_use]
    pub fn correction_pref_count(&self, prefix: &str, picked: &str) -> u32 {
        self.corrections.pref_count(prefix, picked)
    }

    /// How often `word` was flagged unwanted (inspection seam).
    #[must_use]
    pub fn correction_unwanted_count(&self, word: &str) -> u32 {
        self.corrections.unwanted_count(word)
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
        self.context.persist(store)?;
        self.corrections.persist(store)?;
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
        self.context = Context::load(store)?;
        self.corrections = Corrections::load(store)?;
        Ok(())
    }
}
