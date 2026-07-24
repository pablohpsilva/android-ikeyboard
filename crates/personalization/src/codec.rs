//! Byte encoding for the lexical blobs, done by hand (no serialization crate,
//! per the crate brief).
//!
//! Both blobs are newline-joined UTF-8 records:
//!
//! * frequency dictionary — one `"<count>\t<word>"` line per entry, e.g.
//!   `"3\thello"`. A tab separates the decimal count from the word; words never
//!   contain a tab or newline (they are single lexical tokens).
//! * whitelist — one `"<word>"` line per entry.
//!
//! An empty collection encodes to zero bytes, and zero bytes decodes back to an
//! empty collection — the round-trip the model relies on. Because the empty
//! string can never be a stored word (the model rejects it on the way in), that
//! mapping is unambiguous.
//!
//! Decoding is total on the happy path and returns a value on every malformed
//! input: a corrupt blob yields [`StoreError::Backend`] rather than a panic
//! (SEDD §5.5 r3).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use featherkey_contracts::StoreError;

/// The count/word separator inside a frequency line.
const FIELD_SEP: char = '\t';

/// Encode the frequency dictionary. Entries are emitted in `BTreeMap` order, so
/// the encoding is deterministic (stable bytes for equal models).
pub(crate) fn encode_frequencies(freqs: &BTreeMap<String, u32>) -> Vec<u8> {
    let mut out = String::new();
    for (i, (word, count)) in freqs.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        // Writing to a `String` is infallible; the result is only ever `Ok`.
        let _ = write!(out, "{count}{FIELD_SEP}{word}");
    }
    out.into_bytes()
}

/// Decode a frequency-dictionary blob.
///
/// # Errors
/// [`StoreError::Backend`] if `bytes` is not valid UTF-8, a line lacks the
/// count/word separator, or the count is not a `u32`.
pub(crate) fn decode_frequencies(bytes: &[u8]) -> Result<BTreeMap<String, u32>, StoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| StoreError::Backend)?;
    let mut map = BTreeMap::new();
    if text.is_empty() {
        return Ok(map);
    }
    for line in text.split('\n') {
        let (count, word) = line.split_once(FIELD_SEP).ok_or(StoreError::Backend)?;
        let count: u32 = count.parse().map_err(|_| StoreError::Backend)?;
        map.insert(word.to_owned(), count);
    }
    Ok(map)
}

/// Encode the whitelist as newline-joined words, in `BTreeSet` order.
pub(crate) fn encode_whitelist(words: &BTreeSet<String>) -> Vec<u8> {
    // `join` over sorted set members is deterministic and allocation-simple.
    words
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

/// Decode a whitelist blob.
///
/// # Errors
/// [`StoreError::Backend`] if `bytes` is not valid UTF-8.
pub(crate) fn decode_whitelist(bytes: &[u8]) -> Result<BTreeSet<String>, StoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| StoreError::Backend)?;
    if text.is_empty() {
        return Ok(BTreeSet::new());
    }
    Ok(text.split('\n').map(str::to_owned).collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn frequencies_round_trip() {
        let mut freqs = BTreeMap::new();
        freqs.insert("hello".to_owned(), 3u32);
        freqs.insert("world".to_owned(), 1u32);
        let bytes = encode_frequencies(&freqs);
        assert_eq!(decode_frequencies(&bytes).unwrap(), freqs);
    }

    #[test]
    fn empty_frequencies_encode_to_no_bytes_and_back() {
        let empty = BTreeMap::new();
        let bytes = encode_frequencies(&empty);
        assert!(bytes.is_empty());
        assert_eq!(decode_frequencies(&bytes).unwrap(), empty);
    }

    #[test]
    fn frequencies_encoding_is_deterministic_and_sorted() {
        let mut freqs = BTreeMap::new();
        freqs.insert("b".to_owned(), 2u32);
        freqs.insert("a".to_owned(), 1u32);
        // BTreeMap order => "a" before "b".
        assert_eq!(encode_frequencies(&freqs), b"1\ta\n2\tb");
    }

    #[test]
    fn decode_frequencies_rejects_non_utf8() {
        assert_eq!(decode_frequencies(&[0xff]).err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_frequencies_rejects_a_line_without_separator() {
        assert_eq!(
            decode_frequencies(b"noseparator").err(),
            Some(StoreError::Backend)
        );
    }

    #[test]
    fn decode_frequencies_rejects_a_non_numeric_count() {
        assert_eq!(
            decode_frequencies(b"NaN\tword").err(),
            Some(StoreError::Backend)
        );
    }

    #[test]
    fn decode_frequencies_rejects_an_overflowing_count() {
        // 2^32 does not fit in u32.
        assert_eq!(
            decode_frequencies(b"4294967296\tw").err(),
            Some(StoreError::Backend)
        );
    }

    #[test]
    fn whitelist_round_trips() {
        let mut wl = BTreeSet::new();
        wl.insert("acme".to_owned());
        wl.insert("zeta".to_owned());
        let bytes = encode_whitelist(&wl);
        assert_eq!(decode_whitelist(&bytes).unwrap(), wl);
    }

    #[test]
    fn empty_whitelist_encodes_to_no_bytes_and_back() {
        let empty = BTreeSet::new();
        let bytes = encode_whitelist(&empty);
        assert!(bytes.is_empty());
        assert_eq!(decode_whitelist(&bytes).unwrap(), empty);
    }

    #[test]
    fn decode_whitelist_rejects_non_utf8() {
        assert_eq!(decode_whitelist(&[0xff]).err(), Some(StoreError::Backend));
    }

    #[test]
    fn whitelist_encoding_is_sorted() {
        let mut wl = BTreeSet::new();
        wl.insert("z".to_owned());
        wl.insert("a".to_owned());
        assert_eq!(encode_whitelist(&wl), b"a\nz");
    }
}
