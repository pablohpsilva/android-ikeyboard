//! Byte encoding for the single lexical blob, done by hand (no serialization
//! crate, per the crate brief).
//!
//! The whole model — the frequency-counted dictionary *and* the whitelist — is
//! serialized into **one** newline-joined UTF-8 blob so it can be persisted with
//! a single atomic [`put`](featherkey_contracts::SecureStore::put). Each line is
//! one record, classified purely by the presence of the field separator (`\t`):
//!
//! * a line **with** a tab is a frequency entry: `"<count>\t<word>"`, e.g.
//!   `"3\thello"`;
//! * a line **without** a tab is a whitelist entry: `"<word>"`.
//!
//! Words never contain a tab or newline (the model rejects any that do on the
//! way in), so a whitelist word can never masquerade as a frequency line and the
//! two record kinds stay unambiguous within the one blob. Frequency lines are
//! emitted first (in `BTreeMap` order), then whitelist lines (in `BTreeSet`
//! order), so equal models encode to identical bytes.
//!
//! An empty model encodes to zero bytes, and zero bytes decodes back to an empty
//! model — the round-trip the model relies on. Because the empty string can never
//! be a stored word, that mapping is unambiguous.
//!
//! Decoding returns a value on every input: a corrupt blob yields
//! [`StoreError::Backend`] rather than a panic (SEDD §5.5 r3).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use featherkey_contracts::StoreError;

/// The count/word separator inside a frequency line. Its presence on a line is
/// also what distinguishes a frequency record from a whitelist record.
const FIELD_SEP: char = '\t';

/// Encode the whole model — frequencies then whitelist — into one blob.
///
/// Frequency entries are emitted in `BTreeMap` order and whitelist entries in
/// `BTreeSet` order, so the encoding is deterministic (stable bytes for equal
/// models).
pub(crate) fn encode_model(freqs: &BTreeMap<String, u32>, whitelist: &BTreeSet<String>) -> Vec<u8> {
    let mut out = String::new();
    let mut first = true;
    for (word, count) in freqs {
        if !first {
            out.push('\n');
        }
        first = false;
        // Writing to a `String` is infallible; the result is only ever `Ok`.
        let _ = write!(out, "{count}{FIELD_SEP}{word}");
    }
    for word in whitelist {
        if !first {
            out.push('\n');
        }
        first = false;
        out.push_str(word);
    }
    out.into_bytes()
}

/// Decode a model blob back into its frequency map and whitelist.
///
/// # Errors
/// [`StoreError::Backend`] if `bytes` is not valid UTF-8, or a frequency line's
/// count (the part before the tab) is not a `u32`.
pub(crate) fn decode_model(
    bytes: &[u8],
) -> Result<(BTreeMap<String, u32>, BTreeSet<String>), StoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| StoreError::Backend)?;
    let mut freqs = BTreeMap::new();
    let mut whitelist = BTreeSet::new();
    if text.is_empty() {
        return Ok((freqs, whitelist));
    }
    for line in text.split('\n') {
        match line.split_once(FIELD_SEP) {
            Some((count, word)) => {
                let count: u32 = count.parse().map_err(|_| StoreError::Backend)?;
                freqs.insert(word.to_owned(), count);
            }
            None => {
                whitelist.insert(line.to_owned());
            }
        }
    }
    Ok((freqs, whitelist))
}

/// Encode the personal proper-noun map (folded key → canonical spelling) into
/// its own blob (BR-69), one `"<folded>\t<canonical>"` line per entry, in
/// `BTreeMap` order. Kept separate from the frequency/whitelist blob: a
/// tab-bearing proper-noun line would be misread as a frequency record if it
/// shared that blob. An empty map encodes to zero bytes.
pub(crate) fn encode_proper(map: &BTreeMap<String, String>) -> Vec<u8> {
    let mut out = String::new();
    let mut first = true;
    for (folded, canonical) in map {
        if !first {
            out.push('\n');
        }
        first = false;
        // Writing to a `String` is infallible; the result is only ever `Ok`.
        let _ = write!(out, "{folded}{FIELD_SEP}{canonical}");
    }
    out.into_bytes()
}

/// Decode a proper-noun blob written by [`encode_proper`].
///
/// # Errors
/// [`StoreError::Backend`] if `bytes` is not valid UTF-8, or a line is missing
/// the field separator (a corrupt blob is a backend fault, not a value).
pub(crate) fn decode_proper(bytes: &[u8]) -> Result<BTreeMap<String, String>, StoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| StoreError::Backend)?;
    let mut map = BTreeMap::new();
    if text.is_empty() {
        return Ok(map);
    }
    for line in text.split('\n') {
        let (folded, canonical) = line.split_once(FIELD_SEP).ok_or(StoreError::Backend)?;
        map.insert(folded.to_owned(), canonical.to_owned());
    }
    Ok(map)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn freq(pairs: &[(&str, u32)]) -> BTreeMap<String, u32> {
        pairs.iter().map(|(w, c)| ((*w).to_owned(), *c)).collect()
    }

    fn wl(words: &[&str]) -> BTreeSet<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn model_round_trips() {
        let freqs = freq(&[("hello", 3), ("world", 1)]);
        let whitelist = wl(&["acme", "zeta"]);
        let bytes = encode_model(&freqs, &whitelist);
        assert_eq!(decode_model(&bytes).unwrap(), (freqs, whitelist));
    }

    #[test]
    fn empty_model_encodes_to_no_bytes_and_back() {
        let bytes = encode_model(&BTreeMap::new(), &BTreeSet::new());
        assert!(bytes.is_empty());
        assert_eq!(
            decode_model(&bytes).unwrap(),
            (BTreeMap::new(), BTreeSet::new())
        );
    }

    #[test]
    fn encoding_is_deterministic_and_sorted() {
        let freqs = freq(&[("b", 2), ("a", 1)]);
        let whitelist = wl(&["z", "m"]);
        // Frequencies first (BTreeMap order), then whitelist (BTreeSet order).
        assert_eq!(encode_model(&freqs, &whitelist), b"1\ta\n2\tb\nm\nz");
    }

    #[test]
    fn frequency_only_model_round_trips() {
        let freqs = freq(&[("only", 5)]);
        let bytes = encode_model(&freqs, &BTreeSet::new());
        assert_eq!(bytes, b"5\tonly");
        assert_eq!(decode_model(&bytes).unwrap(), (freqs, BTreeSet::new()));
    }

    #[test]
    fn whitelist_only_model_round_trips() {
        let whitelist = wl(&["brand"]);
        let bytes = encode_model(&BTreeMap::new(), &whitelist);
        assert_eq!(bytes, b"brand");
        assert_eq!(decode_model(&bytes).unwrap(), (BTreeMap::new(), whitelist));
    }

    #[test]
    fn a_word_can_be_both_learned_and_whitelisted() {
        let freqs = freq(&[("dup", 2)]);
        let whitelist = wl(&["dup"]);
        let bytes = encode_model(&freqs, &whitelist);
        assert_eq!(decode_model(&bytes).unwrap(), (freqs, whitelist));
    }

    #[test]
    fn decode_rejects_non_utf8() {
        assert_eq!(decode_model(&[0xff]).err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_non_numeric_count() {
        assert_eq!(decode_model(b"NaN\tword").err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_an_overflowing_count() {
        // 2^32 does not fit in u32.
        assert_eq!(
            decode_model(b"4294967296\tw").err(),
            Some(StoreError::Backend)
        );
    }

    #[test]
    fn proper_map_round_trips() {
        let mut m = BTreeMap::new();
        m.insert("joao".to_owned(), "João".to_owned());
        m.insert("zoe".to_owned(), "Zoë".to_owned());
        let bytes = encode_proper(&m);
        assert_eq!(decode_proper(&bytes).unwrap(), m);
    }

    #[test]
    fn empty_proper_map_encodes_to_no_bytes_and_back() {
        let bytes = encode_proper(&BTreeMap::new());
        assert!(bytes.is_empty());
        assert_eq!(decode_proper(&bytes).unwrap(), BTreeMap::new());
    }

    #[test]
    fn decode_proper_rejects_non_utf8() {
        assert_eq!(decode_proper(&[0xff]).err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_proper_rejects_a_line_without_a_separator() {
        assert_eq!(
            decode_proper(b"noseparator").err(),
            Some(StoreError::Backend)
        );
    }

    proptest! {
        /// The round-trip invariant: for any set of storable words (no `\n`/`\t`,
        /// non-empty) with arbitrary frequencies, encode-then-decode reproduces
        /// the exact same model.
        #[test]
        fn encode_then_decode_is_identity(
            freqs in prop::collection::btree_map("[^\n\t]{1,8}", any::<u32>(), 0..8),
            whitelist in prop::collection::btree_set("[^\n\t]{1,8}", 0..8),
        ) {
            let bytes = encode_model(&freqs, &whitelist);
            let decoded = decode_model(&bytes).expect("valid blob decodes");
            prop_assert_eq!(decoded, (freqs, whitelist));
        }
    }
}
