//! Correction candidates with a no-clobber policy; implements the AutoCorrect port.
//!
//! This crate owns **one** decision (SEDD §5.2 single responsibility): given a
//! typed [`Token`], should it be replaced, and if so by what? It carries the
//! strict **no-clobber** rule (BR-12): a word the user clearly intended is
//! *never* rewritten. A token is intended — and returned verbatim with
//! `applied == false` — whenever it is a real word:
//!
//! * exactly present in the corrector's own [`Dictionary`], **or**
//! * recognised by any active language through [`LocaleManager::detect`], **or**
//! * known to the user via [`Personalization::is_known`] (learned or
//!   whitelisted).
//!
//! Validity is therefore checked across **all** active languages, not one
//! (BR-18): a genuine word in *any* active language survives untouched, so
//! mixed-language typing is not "corrected" into a single language.
//!
//! Only a token that clears none of those bars may be corrected. Then the
//! corrector offers the dictionary's edit-distance-1 neighbours
//! ([`Dictionary::fuzzy`]): the first is `primary`, the rest are `alternatives`,
//! and `applied` is `true`. With no neighbours there is nothing to offer, so the
//! token is again returned unchanged.
//!
//! It carries no other policy — no ranking model, no learning, no persistence.
//! Errors are values, never panics (SEDD §5.5 r3):
//! [`correct`](AutoCorrect::correct) is total and returns plain data on every
//! path; no `unwrap`/`expect`/`panic!` appears in this crate.

use featherkey_contracts::{AutoCorrect, Correction, Token, TypingContext};
use featherkey_dictionary::Dictionary;
use featherkey_locale_manager::LocaleManager;
use featherkey_personalization::Personalization;

/// A no-clobber corrector over one fuzzy [`Dictionary`], the user's
/// [`Personalization`] model, and the active-language [`LocaleManager`].
///
/// The dictionary plays two roles: it is one source of validity
/// ([`contains`](Dictionary::contains)) and the sole source of correction
/// candidates ([`fuzzy`](Dictionary::fuzzy)). The locale manager widens validity
/// to every active language (BR-18); personalization widens it to the user's own
/// vocabulary. None of the three can *cause* a correction — they can only veto
/// one — which is exactly the no-clobber guarantee (BR-12).
#[derive(Debug)]
pub struct NoClobberCorrector {
    dictionary: Dictionary,
    personalization: Personalization,
    locales: LocaleManager,
}

impl NoClobberCorrector {
    /// Assemble a corrector from the lexical substrate it consults: the fuzzy
    /// dictionary, the user's learned/whitelisted vocabulary, and the active
    /// languages. All three are owned so the corrector is self-contained and
    /// `correct` needs no further wiring.
    #[must_use]
    pub fn new(
        dictionary: Dictionary,
        personalization: Personalization,
        locales: LocaleManager,
    ) -> Self {
        Self {
            dictionary,
            personalization,
            locales,
        }
    }

    /// Is `word` a real word the user clearly intended (BR-12)? True when it is
    /// exactly in the dictionary, known to the user, or recognised by *any*
    /// active language (BR-18). An empty token has no word to correct, so it is
    /// treated as intended (returned unchanged) rather than run through fuzzy
    /// generation, which would otherwise invent a single-letter "correction".
    fn is_intended(&self, word: &str) -> bool {
        if word.is_empty() || self.personalization.is_known(word) {
            return true;
        }
        // Validity is case-insensitive so a legitimately capitalized word (e.g.
        // sentence-initial "Cat") is never clobbered to "cat" (BR-12). Casing is
        // smart-typing's concern, not ours — we only refuse to destroy a word.
        let lower = word.to_lowercase();
        self.dictionary.contains(word)
            || self.dictionary.contains(&lower)
            || self.locales.detect(word).is_some()
            || self.locales.detect(&lower).is_some()
    }
}

/// The no-clobber outcome: `word` committed verbatim, nothing applied.
fn unchanged(word: &str) -> Correction {
    Correction {
        primary: word.to_owned(),
        alternatives: Vec::new(),
        applied: false,
    }
}

impl AutoCorrect for NoClobberCorrector {
    fn correct(&self, token: &Token, _ctx: &TypingContext) -> Correction {
        let word = token.text.as_str();
        // BR-12: a word the user clearly intended is never clobbered.
        if self.is_intended(word) {
            return unchanged(word);
        }
        // Not a real word anywhere — offer edit-distance-1 neighbours. `fuzzy`
        // excludes the query itself, so any candidate genuinely differs and
        // `applied` is honestly `true`.
        let candidates = self.dictionary.fuzzy(word);
        match candidates.split_first() {
            // No neighbour to suggest: leave the token as typed rather than
            // guess, keeping the no-clobber promise even for a non-word.
            None => unchanged(word),
            Some((primary, rest)) => Correction {
                primary: primary.clone(),
                alternatives: rest.to_vec(),
                applied: true,
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use featherkey_locale_manager::LangId;
    use proptest::prelude::*;

    /// Build a dictionary from pre-sorted fixture words. `expect` is confined to
    /// tests, never library code (SEDD §5.5 r3).
    fn dict(words: &[&str]) -> Dictionary {
        Dictionary::from_sorted_words(words.iter().copied()).expect("fixture is sorted")
    }

    /// A single active language over the given lexicon.
    fn locales(tag: &str, words: &[&str]) -> LocaleManager {
        LocaleManager::new(vec![(LangId::new(tag), dict(words))]).expect("valid active set")
    }

    /// A corrector whose validity comes solely from its own dictionary: the
    /// personalization model is empty and the sole active language has an empty
    /// lexicon, so neither can veto a correction on its own.
    fn corrector_over(words: &[&str]) -> NoClobberCorrector {
        NoClobberCorrector::new(dict(words), Personalization::new(), locales("xx", &[]))
    }

    fn token(text: &str) -> Token {
        Token {
            text: text.to_owned(),
        }
    }

    fn correct(c: &NoClobberCorrector, text: &str) -> Correction {
        c.correct(&token(text), &TypingContext::default())
    }

    // --- BR-12 no-clobber: a valid word is returned verbatim ------------------

    #[test]
    fn a_dictionary_word_is_never_clobbered() {
        let c = corrector_over(&["cat", "cot", "hat"]);
        let got = correct(&c, "cat");
        assert_eq!(got.primary, "cat");
        assert!(!got.applied);
        assert!(got.alternatives.is_empty());
    }

    #[test]
    fn a_whitelisted_word_is_never_clobbered() {
        // "acme" is in no lexicon, but the user whitelisted it: no-clobber holds
        // even though fuzzy neighbours ("acre"/"acne") exist in the dictionary.
        let mut personal = Personalization::new();
        personal.whitelist("acme");
        let c = NoClobberCorrector::new(dict(&["acne", "acre"]), personal, locales("xx", &[]));
        let got = correct(&c, "acme");
        assert_eq!(got.primary, "acme");
        assert!(!got.applied);
        assert!(got.alternatives.is_empty());
    }

    #[test]
    fn a_learned_word_is_never_clobbered() {
        // A word the user has typed before (observed, not whitelisted) is known
        // and therefore intended.
        let mut personal = Personalization::new();
        personal.observe("brb");
        let c = NoClobberCorrector::new(dict(&["bra", "orb"]), personal, locales("xx", &[]));
        let got = correct(&c, "brb");
        assert_eq!(got.primary, "brb");
        assert!(!got.applied);
    }

    // --- BR-18 validity spans every active language ---------------------------

    #[test]
    fn a_word_valid_only_in_a_second_active_language_is_not_corrected() {
        // The fuzzy dictionary is English and does NOT contain "mundo"; the
        // corrector must still leave it alone because Portuguese — the *second*
        // active language — recognises it (BR-18: all active languages count).
        let both = LocaleManager::new(vec![
            (LangId::new("en"), dict(&["hello", "world"])),
            (LangId::new("pt"), dict(&["mundo", "olph"])),
        ])
        .expect("valid active set");
        let c = NoClobberCorrector::new(dict(&["hello", "world"]), Personalization::new(), both);
        let got = correct(&c, "mundo");
        assert_eq!(got.primary, "mundo");
        assert!(!got.applied);
    }

    // --- correction path: only a non-word is corrected ------------------------

    #[test]
    fn a_non_word_is_corrected_to_its_best_neighbour_with_the_rest_as_alternatives() {
        // "zat" is a non-word one substitution from each of "bat"/"cat"/"hat";
        // lexicographic order makes "bat" the primary and the rest alternatives.
        let c = corrector_over(&["bat", "cat", "hat"]);
        let got = correct(&c, "zat");
        assert!(got.applied);
        assert_eq!(got.primary, "bat");
        assert_eq!(got.alternatives, ["cat", "hat"]);
    }

    #[test]
    fn a_non_word_with_a_single_neighbour_has_no_alternatives() {
        let c = corrector_over(&["cat"]);
        let got = correct(&c, "caz");
        assert!(got.applied);
        assert_eq!(got.primary, "cat");
        assert!(got.alternatives.is_empty());
        assert_ne!(got.primary, "caz");
    }

    #[test]
    fn a_non_word_with_no_neighbours_is_left_unchanged() {
        // "qqqq" is far from every dictionary word: nothing to offer, so the
        // token is returned as typed (no-clobber even for a non-word).
        let c = corrector_over(&["cat", "dog"]);
        let got = correct(&c, "qqqq");
        assert_eq!(got.primary, "qqqq");
        assert!(!got.applied);
        assert!(got.alternatives.is_empty());
    }

    #[test]
    fn an_empty_token_is_returned_unchanged() {
        // An empty token has no word to correct; it must not be turned into a
        // single-letter dictionary word via fuzzy insertion.
        let c = corrector_over(&["a", "i"]);
        let got = correct(&c, "");
        assert_eq!(got.primary, "");
        assert!(!got.applied);
        assert!(got.alternatives.is_empty());
    }

    #[test]
    fn a_capitalized_valid_word_is_not_clobbered() {
        // "Cat" is "cat" capitalized — a word the user clearly intended. Case-
        // insensitive validity keeps it unchanged (BR-12); without it, "Cat"
        // would be "corrected" to a lowercase neighbour.
        let c = corrector_over(&["cat", "cot", "cut"]);
        let got = correct(&c, "Cat");
        assert_eq!(got.primary, "Cat");
        assert!(!got.applied);
        assert!(got.alternatives.is_empty());
    }

    #[test]
    fn debug_is_implemented() {
        // missing_debug_implementations is a workspace lint; prove Debug renders.
        let c = corrector_over(&["cat"]);
        assert!(format!("{c:?}").contains("NoClobberCorrector"));
    }

    // --- MANDATORY property test (ARCH §8): the BR-12 no-clobber invariant -----

    proptest! {
        /// BR-12 headline invariant: for *any* word present in the dictionary,
        /// `correct` returns it UNCHANGED with `applied == false` and no
        /// alternatives — a real word is never clobbered, whatever it is.
        #[test]
        fn dictionary_words_are_never_clobbered(
            words in prop::collection::btree_set("[a-z]{1,8}", 1..12),
        ) {
            // A `btree_set` yields ASCII-lowercase words in sorted (byte) order,
            // exactly the contract `from_sorted_words` requires.
            let sorted: Vec<String> = words.iter().cloned().collect();
            let dictionary = Dictionary::from_sorted_words(&sorted).expect("btree_set is sorted");
            let corrector = NoClobberCorrector::new(
                dictionary,
                Personalization::new(),
                locales("xx", &[]),
            );
            for word in &sorted {
                let got = corrector.correct(&token(word), &TypingContext::default());
                prop_assert_eq!(&got.primary, word);
                prop_assert!(!got.applied);
                prop_assert!(got.alternatives.is_empty());
            }
        }

        /// The same no-clobber guarantee via personalization: any whitelisted
        /// word is returned unchanged, even against a dictionary that could
        /// otherwise suggest neighbours.
        #[test]
        fn whitelisted_words_are_never_clobbered(
            words in prop::collection::btree_set("[a-z]{1,8}", 1..12),
        ) {
            let mut personal = Personalization::new();
            for word in &words {
                personal.whitelist(word);
            }
            let corrector = NoClobberCorrector::new(
                dict(&["cat", "cot", "dog"]),
                personal,
                locales("xx", &[]),
            );
            for word in &words {
                let got = corrector.correct(&token(word), &TypingContext::default());
                prop_assert_eq!(&got.primary, word);
                prop_assert!(!got.applied);
                prop_assert!(got.alternatives.is_empty());
            }
        }
    }
}
