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

    /// Rebuild the merged proper-noun caser from the bundled per-language lists.
    /// (The personal learned set joins here in a later slice.)
    fn build_proper_caser(&self) -> ProperCaser {
        let bundled = self.packs.iter().flat_map(|p| p.proper.iter().cloned());
        ProperCaser::new(bundled, std::iter::empty::<String>())
    }
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
