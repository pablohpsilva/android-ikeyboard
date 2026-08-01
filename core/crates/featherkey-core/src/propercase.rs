//! Proper-noun capitalization wiring (BR-69): builds a `ProperCaser` from the
//! bundled per-language proper lists, injects the common-word guard from the
//! active dictionaries, and answers `proper_case`. The personal learned set
//! joins the merged caser in a later slice.

use featherkey_propercase::ProperCaser;

impl crate::FeatherKeyCore {
    /// The canonical proper-noun spelling to apply for `word`, or `None`.
    /// Builds (and caches) the merged caser lazily; the guard answers "is `word`
    /// a common lowercase word in any active lexicon?".
    pub fn proper_case(&mut self, word: &str, is_sentence_start: bool) -> Option<String> {
        if self.proper_caser.is_none() {
            self.proper_caser = Some(self.build_proper_caser());
        }
        let packs = &self.packs;
        let is_common = |w: &str| packs.iter().any(|p| p.dict.contains(w));
        self.proper_caser
            .as_ref()
            .and_then(|c| c.case(word, is_sentence_start, &is_common))
    }

    /// Rebuild the merged proper-noun caser from the bundled per-language lists
    /// plus the learned personal set (personal wins on a fold-key collision).
    fn build_proper_caser(&self) -> ProperCaser {
        let bundled: Vec<String> = self
            .packs
            .iter()
            .flat_map(|p| p.proper.iter().cloned())
            .collect();
        let personal: Vec<String> = self
            .personalization
            .proper_nouns()
            .values()
            .cloned()
            .collect();
        ProperCaser::new(bundled, personal)
    }

    /// Record `word` as a personal proper noun if it is a habitual mid-sentence
    /// capital (BR-69): title-case, not a sentence start, not a common lowercase
    /// word, and the field permits learning (BR-22/BR-26 — gated exactly where
    /// `learn_word` gates). Invalidates the cached caser so the next lookup sees it.
    pub fn observe_proper_noun(
        &mut self,
        word: &str,
        is_sentence_start: bool,
        field: &dyn featherkey_contracts::SensitiveContextSource,
    ) {
        if is_sentence_start || self.sensitivity.should_suppress(field) {
            return;
        }
        if !is_title_case(word) {
            return;
        }
        let lower = word.to_lowercase();
        if self.packs.iter().any(|p| p.dict.contains(&lower)) {
            return;
        }
        self.personalization
            .observe_proper_noun(&featherkey_fold::fold(&lower), word);
        self.proper_caser = None;
    }
}

/// True if `word` is title-case with length ≥ 2: first letter upper, rest lower.
/// A lone "I" is not a learnable proper noun.
fn is_title_case(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_uppercase() {
        return false;
    }
    let rest: String = chars.collect();
    !rest.is_empty() && rest == rest.to_lowercase()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::packs::LangInput;
    use crate::FeatherKeyCore;

    fn core_with(words: &[&str], proper: &[&str]) -> FeatherKeyCore {
        let input = LangInput {
            tag: "en".to_owned(),
            words: words.iter().map(|s| (*s).to_owned()).collect(),
            proper: proper.iter().map(|s| (*s).to_owned()).collect(),
        };
        FeatherKeyCore::new_with_proper(vec![input]).unwrap()
    }

    #[test]
    fn recases_a_bundled_proper_noun() {
        let mut core = core_with(&["apple", "rose"], &["Paris"]);
        assert_eq!(core.proper_case("paris", false), Some("Paris".to_owned()));
    }

    #[test]
    fn guard_blocks_a_common_word_twin() {
        // "rose" is in the common lexicon → never recased even though bundled.
        let mut core = core_with(&["apple", "rose"], &["Rose"]);
        assert_eq!(core.proper_case("rose", false), None);
    }

    #[test]
    fn sentence_start_is_left_to_auto_caps() {
        let mut core = core_with(&["apple"], &["Paris"]);
        assert_eq!(core.proper_case("paris", true), None);
    }

    #[test]
    fn unknown_word_is_left_alone() {
        let mut core = core_with(&["apple"], &["Paris"]);
        assert_eq!(core.proper_case("florp", false), None);
    }

    use featherkey_contracts::SensitiveContextSource;

    struct NotSensitive;
    impl SensitiveContextSource for NotSensitive {
        fn is_sensitive(&self) -> bool {
            false
        }
    }
    struct Sensitive;
    impl SensitiveContextSource for Sensitive {
        fn is_sensitive(&self) -> bool {
            true
        }
    }

    #[test]
    fn learns_a_habitual_mid_sentence_title_case_name() {
        let mut core = core_with(&["apple"], &[]);
        core.observe_proper_noun("Zoe", false, &NotSensitive);
        // Typing it lowercase mid-sentence now recases it from the learned set.
        assert_eq!(core.proper_case("zoe", false), Some("Zoe".to_owned()));
    }

    #[test]
    fn does_not_learn_at_a_sentence_start() {
        let mut core = core_with(&["apple"], &[]);
        core.observe_proper_noun("Zoe", true, &NotSensitive);
        assert_eq!(core.proper_case("zoe", false), None);
    }

    #[test]
    fn does_not_learn_a_common_word() {
        let mut core = core_with(&["apple", "rose"], &[]);
        core.observe_proper_noun("Rose", false, &NotSensitive);
        assert_eq!(core.proper_case("rose", false), None);
    }

    #[test]
    fn does_not_learn_in_a_sensitive_field() {
        let mut core = core_with(&["apple"], &[]);
        core.observe_proper_noun("Zoe", false, &Sensitive);
        assert_eq!(core.proper_case("zoe", false), None);
    }

    #[test]
    fn does_not_learn_a_non_title_case_word() {
        let mut core = core_with(&["apple"], &[]);
        core.observe_proper_noun("zoe", false, &NotSensitive); // all-lower: no signal
        core.observe_proper_noun("ZOE", false, &NotSensitive); // all-caps: no signal
        assert_eq!(core.proper_case("zoe", false), None);
    }

    #[test]
    fn cache_is_rebuilt_after_a_language_switch() {
        let mut core = core_with(&["apple"], &["Paris"]);
        assert_eq!(core.proper_case("paris", false), Some("Paris".to_owned()));
        // Switch to a language whose bundled set has a different proper noun.
        core.set_active_languages_with_proper(vec![LangInput {
            tag: "en".to_owned(),
            words: vec!["apple".to_owned()],
            proper: vec!["Berlin".to_owned()],
        }])
        .unwrap();
        assert_eq!(core.proper_case("paris", false), None);
        assert_eq!(core.proper_case("berlin", false), Some("Berlin".to_owned()));
    }
}
