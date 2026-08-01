//! Proper-noun capitalization decision (BR-69). Pure: no I/O, no Android, no
//! panics (SEDD §5.5 — errors are values). Given a typed word and an
//! "is this a common lowercase word?" predicate, decides whether to recase the
//! word to a known proper noun's canonical (already-accented, already-cased)
//! spelling. The guard is load-bearing: a word that is also a common lowercase
//! word is never rewritten.

use featherkey_fold::fold;
use std::collections::BTreeMap;

/// A merged proper-noun set: fold-key → canonical-cased spelling.
#[derive(Debug, Clone, Default)]
pub struct ProperCaser {
    map: BTreeMap<String, String>,
}

impl ProperCaser {
    /// Build from bundled + personal canonical-cased words. Personal entries are
    /// inserted last, so they win over bundled on a fold-key collision. Empty
    /// words are skipped.
    #[must_use]
    pub fn new<I, J, S>(bundled: I, personal: J) -> Self
    where
        I: IntoIterator<Item = S>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut map = BTreeMap::new();
        let mut insert = |w: &str| {
            if !w.is_empty() {
                map.insert(fold(w), w.to_owned());
            }
        };
        for w in bundled {
            insert(w.as_ref());
        }
        for w in personal {
            insert(w.as_ref());
        }
        Self { map }
    }

    /// The canonical proper-noun spelling to apply for `word`, or `None` to
    /// leave it as typed.
    ///
    /// `None` when: the word is empty; `is_sentence_start` (auto-caps owns that
    /// position); the token is neither all-lowercase nor title-case (ALLCAPS and
    /// interior-caps are deliberate); `is_common(lower)` (the guard); the folded
    /// word is not in the set; or the canonical equals the word as typed.
    #[must_use]
    pub fn case(
        &self,
        word: &str,
        is_sentence_start: bool,
        is_common: &dyn Fn(&str) -> bool,
    ) -> Option<String> {
        if word.is_empty() || is_sentence_start {
            return None;
        }
        let lower = word.to_lowercase();
        if !is_eligible(word, &lower) {
            return None;
        }
        if is_common(&lower) {
            return None;
        }
        let canon = self.map.get(&fold(&lower))?;
        if canon == word {
            None
        } else {
            Some(canon.clone())
        }
    }
}

/// True if `word` is all-lowercase, or title-case (first letter upper, the rest
/// lowercase). ALLCAPS and interior-caps return false.
fn is_eligible(word: &str, lower: &str) -> bool {
    if word == lower {
        return true;
    }
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => {
            let rest: String = chars.collect();
            first.is_uppercase() && rest == rest.to_lowercase()
        }
        None => false,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn never(_: &str) -> bool { false }
    fn always(_: &str) -> bool { true }

    fn caser(words: &[&str]) -> ProperCaser {
        ProperCaser::new(words.iter().copied(), std::iter::empty::<&str>())
    }

    #[test]
    fn recases_a_known_proper_noun_typed_lowercase() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("paris", false, &never), Some("Paris".to_owned()));
    }

    #[test]
    fn leaves_a_common_lowercase_word_alone() {
        let c = caser(&["Rose"]);
        assert_eq!(c.case("rose", false, &always), None);
    }

    #[test]
    fn restores_accents_and_case_together() {
        let c = caser(&["João"]);
        assert_eq!(c.case("joao", false, &never), Some("João".to_owned()));
    }

    #[test]
    fn returns_none_at_a_sentence_start() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("paris", true, &never), None);
    }

    #[test]
    fn never_rewrites_all_caps() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("PARIS", false, &never), None);
    }

    #[test]
    fn never_rewrites_interior_caps() {
        let c = caser(&["Iphone"]);
        assert_eq!(c.case("iPhone", false, &never), None);
    }

    #[test]
    fn title_case_input_already_canonical_is_left_alone() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("Paris", false, &never), None);
    }

    #[test]
    fn title_case_input_upgraded_to_accented_canonical() {
        let c = caser(&["João"]);
        assert_eq!(c.case("Joao", false, &never), Some("João".to_owned()));
    }

    #[test]
    fn unknown_word_is_left_alone() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("florp", false, &never), None);
    }

    #[test]
    fn empty_word_is_left_alone() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("", false, &never), None);
    }

    #[test]
    fn personal_entry_overrides_bundled_on_fold_collision() {
        // Bundled "Paris"; personal "PARÍS"-style canonical wins on same fold key.
        let c = ProperCaser::new(["Paris"], ["Párís"]);
        assert_eq!(c.case("paris", false, &never), Some("Párís".to_owned()));
    }

    #[test]
    fn empty_bundled_words_are_skipped() {
        let c = caser(&["", "Paris"]);
        assert_eq!(c.case("paris", false, &never), Some("Paris".to_owned()));
    }
}
