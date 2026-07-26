//! Language activation: turning the shell's `(tag, words)` lists into the
//! validated lexicon packs the core ranks against.
//!
//! The one non-obvious job here is **carrying bundled frequency across the
//! `fst`**. A [`Dictionary`] is byte-sorted and holds no frequency of its own, so
//! the commonness a word carried in the shipped asset list would be lost the
//! moment it is sorted. Recording each word's input position *before* sorting is
//! what preserves it (DECISION option A); `rank.rs` then consumes that as
//! `dict_rank` so common words still outrank rare ones.

use std::collections::HashMap;

use featherkey_dictionary::Dictionary;
use featherkey_locale_manager::{LangId, LocaleManager};

use crate::FeatherKeyError;

/// One active language: its tag, its validated (byte-sorted) lexicon, and the
/// bundled per-word frequency **rank** recovered from the activation order.
///
/// The [`Dictionary`] is a byte-sorted `fst` with no frequency of its own, so the
/// commonness a word carried in the shipped list would be lost the moment it is
/// sorted. [`build_packs`] therefore records each word's *input position* — the
/// shell activates languages in frequency order, most-common first — as its
/// `rank` (`0` = commonest) **before** sorting for the `fst`. The predictor
/// consumes this as `dict_rank` so common words still rank ahead of rare ones
/// (DECISION option A: carry frequency into the Rust core).
#[derive(Debug, Clone)]
pub(crate) struct Pack {
    pub(crate) lang: LangId,
    pub(crate) dict: Dictionary,
    /// `word -> rank` (`0` = commonest). A word absent here sorts last.
    pub(crate) rank: HashMap<String, u32>,
}

/// Validate a `(tag, words)` language list into lexicon packs, recording each
/// word's bundled frequency **rank** from the activation order. Shared by
/// construction and language switching so both apply the identical contract.
///
/// `words` arrive in **frequency order** (most-common first — the shell's asset
/// order; DECISION option A). Each word's input position becomes its `rank`
/// (`0` = commonest) *before* the list is byte-sorted for the `fst`, so the
/// bundled commonness survives sorting. A repeated word keeps its earliest (most
/// frequent) position. The set is still validated as non-empty with no duplicate
/// tag; ordering is no longer a rejection reason (the core sorts internally).
pub(crate) fn build_packs(
    languages: Vec<(String, Vec<String>)>,
) -> Result<Vec<Pack>, FeatherKeyError> {
    let mut packs = Vec::with_capacity(languages.len());
    for (tag, words) in languages {
        let mut rank: HashMap<String, u32> = HashMap::with_capacity(words.len());
        for (position, word) in words.iter().enumerate() {
            // First (most-frequent) occurrence wins; `position` never exceeds the
            // input length, far below `u32::MAX`.
            rank.entry(word.clone()).or_insert(position as u32);
        }
        // The `fst` needs non-decreasing byte order; sort a copy of the
        // frequency-ordered input (adjacent duplicates are merged by the
        // dictionary itself).
        let mut sorted = words;
        sorted.sort();
        let dict = Dictionary::from_sorted_words(sorted)?;
        packs.push(Pack {
            lang: LangId::new(tag),
            dict,
            rank,
        });
    }
    // Build a real LocaleManager purely to validate the set — it rejects an
    // empty set (→ NoLanguages) and a duplicate tag (→ Locale). Discarded;
    // `correct` rebuilds one on demand.
    let locale_pairs: Vec<(LangId, Dictionary)> = packs
        .iter()
        .map(|p| (p.lang.clone(), p.dict.clone()))
        .collect();
    LocaleManager::new(locale_pairs)?;
    Ok(packs)
}

/// The primary (first) active language tag — the one whose script drives the
/// alpha page. Falls back to `"en"` (QWERTY) when the set is empty, which the
/// public API never produces (`build_packs` rejects an empty set).
pub(crate) fn primary_tag(packs: &[Pack]) -> String {
    packs
        .first()
        .map_or_else(|| "en".to_owned(), |p| p.lang.as_str().to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    use crate::FeatherKeyCore;

    #[test]
    fn primary_tag_falls_back_to_en_on_an_empty_set() {
        assert_eq!(primary_tag(&[]), "en");
    }

    #[test]
    fn primary_tag_is_the_first_pack_in_preference_order() {
        let core = FeatherKeyCore::new(vec![
            ("ru".to_owned(), vec!["да".to_owned()]),
            ("en".to_owned(), vec!["cat".to_owned()]),
        ])
        .expect("valid core");
        assert_eq!(primary_tag(&core.packs), "ru");
    }
    #[test]
    fn build_packs_records_frequency_rank_from_input_position() {
        // Words arrive in frequency order; rank = input position even though the
        // fst stores them alphabetically (aardvark < cat < the).
        let core = FeatherKeyCore::new(vec![(
            "en".into(),
            vec!["the".into(), "cat".into(), "aardvark".into()],
        )])
        .expect("core");
        let rank = &core.packs[0].rank;
        assert_eq!(rank.get("the"), Some(&0));
        assert_eq!(rank.get("cat"), Some(&1));
        assert_eq!(rank.get("aardvark"), Some(&2));
    }
}
