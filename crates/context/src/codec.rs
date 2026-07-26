//! Byte encoding for the bigram model's single blob, done by hand (no
//! serialization crate), mirroring `touch-model`'s and `personalization`'s
//! codecs.
//!
//! The whole model is serialized into one newline-joined UTF-8 blob so it can be
//! persisted with a single atomic [`put`](featherkey_contracts::SecureStore::put).
//! Each line is one transition:
//!
//! ```text
//! <count>\t<prev>\t<next>
//! ```
//!
//! e.g. `"2\tthe\tcat"`. Tokens never contain the field separator (`\t`) or a
//! newline — the model rejects any that do on the way in ([`crate::is_storable`])
//! — so a transition's three fields stay unambiguous. Lines are emitted in
//! ascending `(prev, next)` order (the model stores them in nested `BTreeMap`s),
//! so equal models encode to identical bytes (deterministic).
//!
//! An empty model encodes to zero bytes, and zero bytes decodes back to an empty
//! model — the round-trip the model relies on. Because a stored token can never
//! be empty, that mapping is unambiguous.
//!
//! Decoding returns a value on every input: a corrupt blob yields
//! [`StoreError::Backend`] rather than a panic (SEDD §5.5 r3).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use featherkey_contracts::StoreError;

/// Field separator inside a line (between count, prev and next).
const SEP: char = '\t';

/// Encode every transition into one blob, ordered by `(prev, next)` for
/// determinism.
pub(crate) fn encode(frequencies: &BTreeMap<String, BTreeMap<String, u32>>) -> Vec<u8> {
    let mut out = String::new();
    let mut first = true;
    for (prev, inner) in frequencies {
        for (next, count) in inner {
            if !first {
                out.push('\n');
            }
            first = false;
            // Writing to a `String` is infallible.
            let _ = write!(out, "{count}{SEP}{prev}{SEP}{next}");
        }
    }
    out.into_bytes()
}

/// Decode a blob back into the nested transition counts.
///
/// # Errors
/// [`StoreError::Backend`] if `bytes` is not valid UTF-8, a line is malformed
/// (wrong field count), or a count is not a `u32`.
pub(crate) fn decode(bytes: &[u8]) -> Result<BTreeMap<String, BTreeMap<String, u32>>, StoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| StoreError::Backend)?;
    let mut frequencies: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
    if text.is_empty() {
        return Ok(frequencies);
    }
    for line in text.split('\n') {
        let mut parts = line.split(SEP);
        let count: u32 = parts
            .next()
            .ok_or(StoreError::Backend)?
            .parse()
            .map_err(|_| StoreError::Backend)?;
        let prev = parts.next().ok_or(StoreError::Backend)?;
        let next = parts.next().ok_or(StoreError::Backend)?;
        if parts.next().is_some() {
            return Err(StoreError::Backend); // trailing field
        }
        frequencies
            .entry(prev.to_owned())
            .or_default()
            .insert(next.to_owned(), count);
    }
    Ok(frequencies)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn model(
        rows: &[(&str, &str, u32)],
    ) -> BTreeMap<String, BTreeMap<String, u32>> {
        let mut m: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
        for &(prev, next, count) in rows {
            m.entry(prev.to_owned())
                .or_default()
                .insert(next.to_owned(), count);
        }
        m
    }

    #[test]
    fn empty_model_encodes_to_no_bytes_and_back() {
        let bytes = encode(&BTreeMap::new());
        assert!(bytes.is_empty());
        assert_eq!(decode(&bytes).unwrap(), BTreeMap::new());
    }

    #[test]
    fn encoding_is_deterministic_and_sorted_by_prev_then_next() {
        let m = model(&[("the", "dog", 1), ("the", "cat", 2), ("big", "dog", 3)]);
        // Ordered by (prev, next): big/dog, the/cat, the/dog.
        assert_eq!(encode(&m), b"3\tbig\tdog\n2\tthe\tcat\n1\tthe\tdog");
    }

    #[test]
    fn model_round_trips() {
        let m = model(&[("the", "cat", 2), ("the", "dog", 1), ("go", "north", 5)]);
        let bytes = encode(&m);
        assert_eq!(decode(&bytes).unwrap(), m);
    }

    #[test]
    fn decode_rejects_non_utf8() {
        assert_eq!(decode(&[0xff]).err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_non_numeric_count() {
        assert_eq!(decode(b"NaN\tthe\tcat").err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_an_overflowing_count() {
        // 2^32 does not fit in u32.
        assert_eq!(
            decode(b"4294967296\tthe\tcat").err(),
            Some(StoreError::Backend)
        );
    }

    #[test]
    fn decode_rejects_a_missing_field() {
        assert_eq!(decode(b"2\tthe").err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_trailing_field() {
        assert_eq!(decode(b"2\tthe\tcat\tx").err(), Some(StoreError::Backend));
    }

    proptest! {
        /// The round-trip invariant: for any set of storable transitions (tokens
        /// with no `\n`/`\t`, non-empty) with arbitrary counts, encode-then-decode
        /// reproduces the exact same model. Inner maps are always non-empty so no
        /// `prev` with zero transitions can be dropped by the encoder.
        #[test]
        fn encode_then_decode_is_identity(
            m in prop::collection::btree_map(
                "[^\n\t]{1,8}",
                prop::collection::btree_map("[^\n\t]{1,8}", any::<u32>(), 1..6),
                0..6,
            ),
        ) {
            let bytes = encode(&m);
            let decoded = decode(&bytes).expect("valid blob decodes");
            prop_assert_eq!(decoded, m);
        }
    }
}
