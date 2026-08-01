//! Byte encoding for the `Vocab` sub-blob inside a persisted `NextWordLm`,
//! done by hand (no serialization crate), mirroring `featherkey_context`'s
//! `codec.rs`: one newline-joined UTF-8 blob, one line per learned entry:
//!
//! ```text
//! <index>\t<freq>\t<word>
//! ```
//!
//! e.g. `"2\t7\tcat"`. Tokens never contain the field separator (`\t`) or a
//! newline — `Vocab::intern` only ever registers a token that passes
//! `featherkey_context::is_learnable` (which implies `is_storable`), so no
//! entry this module encodes can contain one. `decode` re-checks
//! [`featherkey_context::is_storable`] on the way back in anyway, so a
//! corrupt or hand-crafted blob can never smuggle a separator-bearing "word"
//! past this module into `Vocab::from_entries`.
//!
//! Lines are emitted in ascending word order (`Vocab::entries` iterates its
//! `BTreeMap`), so equal vocabularies encode to identical bytes
//! (deterministic). An empty vocabulary encodes to zero bytes, and zero
//! bytes decodes back to an empty entry list.
//!
//! Decoding is total and panic-free: any malformed line yields `None` rather
//! than a panic or a partially-built result, so `persist::decode` can fall
//! back to cold-start on any corruption (SEDD §5.5 r3).

use std::fmt::Write as _;

use featherkey_context::is_storable;

use crate::Vocab;

/// Field separator inside a line (between index, freq and word).
const SEP: char = '\t';

/// Encode every learned entry into one blob, in ascending word order (the
/// order `Vocab::entries` already iterates in) for determinism.
pub(crate) fn encode(vocab: &Vocab) -> Vec<u8> {
    let mut out = String::new();
    let mut first = true;
    for (word, index, freq) in vocab.entries() {
        if !first {
            out.push('\n');
        }
        first = false;
        // Writing to a `String` is infallible.
        let _ = write!(out, "{index}{SEP}{freq}{SEP}{word}");
    }
    out.into_bytes()
}

/// Decode a blob back into `(word, index, freq)` entries, in the order they
/// appear in `bytes`. Domain validation (index range, duplicates) is
/// `Vocab::from_entries`'s job, not this module's — this module only
/// guarantees well-formed, storable fields.
///
/// Returns `None` (never panics) for non-UTF-8 bytes, a line with the wrong
/// field count, a non-numeric index/freq, or a word containing a codec
/// separator.
pub(crate) fn decode(bytes: &[u8]) -> Option<Vec<(String, usize, u32)>> {
    let text = std::str::from_utf8(bytes).ok()?;
    if text.is_empty() {
        return Some(Vec::new());
    }
    let mut entries = Vec::new();
    for line in text.split('\n') {
        let mut parts = line.split(SEP);
        let index: usize = parts.next()?.parse().ok()?;
        let freq: u32 = parts.next()?.parse().ok()?;
        let word = parts.next()?;
        if parts.next().is_some() {
            return None; // trailing field
        }
        if word.is_empty() || !is_storable(word) {
            return None;
        }
        entries.push((word.to_owned(), index, freq));
    }
    Some(entries)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn empty_vocab_encodes_to_no_bytes_and_back() {
        let v = Vocab::new();
        let bytes = encode(&v);
        assert!(bytes.is_empty());
        assert_eq!(decode(&bytes).unwrap(), Vec::new());
    }

    #[test]
    fn entries_round_trip_including_frequency() {
        let mut v = Vocab::new();
        v.intern("cat");
        v.intern("cat");
        v.intern("dog");
        let bytes = encode(&v);
        let mut decoded = decode(&bytes).unwrap();
        decoded.sort_by(|a, b| a.0.cmp(&b.0));
        let mut expected: Vec<(String, usize, u32)> =
            v.entries().map(|(w, i, f)| (w.to_owned(), i, f)).collect();
        expected.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(decoded, expected);
        // "cat" was interned twice -> freq 2.
        let (_, _, cat_freq) = decoded.iter().find(|(w, ..)| w == "cat").unwrap();
        assert_eq!(*cat_freq, 2);
    }

    #[test]
    fn decode_rejects_non_utf8() {
        assert!(decode(&[0xff]).is_none());
    }

    #[test]
    fn decode_rejects_a_missing_field() {
        assert!(decode(b"2\t3").is_none());
    }

    #[test]
    fn decode_rejects_a_trailing_field() {
        assert!(decode(b"2\t3\tcat\tx").is_none());
    }

    #[test]
    fn decode_rejects_a_non_numeric_index_or_freq() {
        assert!(decode(b"NaN\t3\tcat").is_none());
        assert!(decode(b"2\tNaN\tcat").is_none());
    }
}
