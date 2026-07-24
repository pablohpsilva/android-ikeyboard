//! `Correct` use-case (ARCH §9.1): decide whether/how to correct a typed token
//! without ever clobbering a word the user clearly intended (BR-12).
//!
//! The corrector owns its lexical substrate by value, so the façade rebuilds one
//! on demand from its authoritative state — a clone of each lexicon, a clone of
//! the current [`Personalization`] snapshot, and a fresh [`LocaleManager`] over
//! the active languages. Rebuilding (rather than caching) keeps correction
//! consulting the *current* learned vocabulary with no cache to invalidate;
//! correction is off the sub-millisecond decode hot path, so the clone cost is
//! acceptable at MVP (a materialized read model is a v1.x optimization).

use featherkey_autocorrect::NoClobberCorrector;
use featherkey_contracts::{AutoCorrect, Correction, Token, TypingContext};
use featherkey_locale_manager::LocaleManager;

use crate::error::FeatherKeyError;
use crate::FeatherKeyCore;

impl FeatherKeyCore {
    /// Correct `text` in its `(preceding, prefix)` context. A word already known
    /// — in a lexicon, whitelisted/learned, or valid in any active language — is
    /// returned verbatim with `applied == false` (the no-clobber rule, BR-12).
    ///
    /// # Errors
    /// [`FeatherKeyError::Locale`] / [`FeatherKeyError::NoLanguages`] if the
    /// active language set cannot form a corrector (structurally prevented by the
    /// constructor's validation, but surfaced rather than panicked).
    pub fn correct(
        &self,
        text: &str,
        preceding: &str,
        prefix: &str,
    ) -> Result<Correction, FeatherKeyError> {
        let corrector = self.build_corrector()?;
        let ctx = TypingContext {
            preceding: preceding.to_owned(),
            prefix: prefix.to_owned(),
        };
        Ok(corrector.correct(
            &Token {
                text: text.to_owned(),
            },
            &ctx,
        ))
    }

    /// Assemble a `NoClobberCorrector` from a clone of the current state: the
    /// primary (first active) lexicon for fuzzy candidates, the live learned
    /// vocabulary, and a locale manager over every active language.
    fn build_corrector(&self) -> Result<NoClobberCorrector, FeatherKeyError> {
        let locales = LocaleManager::new(self.packs.clone())?;
        let (_, primary) = self.packs.first().ok_or(FeatherKeyError::NoLanguages)?;
        Ok(NoClobberCorrector::new(
            primary.clone(),
            self.personalization.clone(),
            locales,
        ))
    }
}
