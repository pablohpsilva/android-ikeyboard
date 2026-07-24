//! Byte encoding for the tap model's single blob, done by hand (no serialization
//! crate), mirroring `personalization`'s codec.
//!
//! The whole model is serialized into one newline-joined UTF-8 blob so it can be
//! persisted with a single atomic [`put`](featherkey_contracts::SecureStore::put).
//! Each line is one key's learned mean:
//!
//! ```text
//! <key char>\t<dx>\t<dy>\t<count>
//! ```
//!
//! e.g. `"a\t2.5\t-1\t37"`. Layout key characters are letters/digits/punctuation
//! and never contain the field separator (`\t`) or a newline, so records stay
//! unambiguous. Lines are emitted in ascending key-char order, so equal models
//! encode to identical bytes (deterministic). Floats use Rust's shortest
//! round-tripping `Display`/`FromStr`, so a decode reproduces the exact `f32`.
//!
//! An empty model encodes to zero bytes and back. Decoding returns a value on
//! every input: a corrupt blob yields [`StoreError::Backend`] rather than a
//! panic (SEDD §5.5 r3).

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use featherkey_contracts::StoreError;
use featherkey_kernel::KeyId;

use crate::Mean;

/// Field separator inside a line.
const SEP: char = '\t';

/// Encode every key's mean into one blob, ordered by key char for determinism.
pub(crate) fn encode(means: &HashMap<KeyId, Mean>) -> Vec<u8> {
    let ordered: BTreeMap<char, &Mean> = means.iter().map(|(k, m)| (k.0, m)).collect();
    let mut out = String::new();
    let mut first = true;
    for (ch, m) in ordered {
        if !first {
            out.push('\n');
        }
        first = false;
        // Writing to a `String` is infallible.
        let _ = write!(out, "{ch}{SEP}{}{SEP}{}{SEP}{}", m.dx, m.dy, m.count);
    }
    out.into_bytes()
}

/// Decode a blob back into the per-key means.
///
/// # Errors
/// [`StoreError::Backend`] if `bytes` is not valid UTF-8, a line is malformed
/// (wrong field count, key not exactly one char, unparseable number), or a
/// decoded offset is non-finite (a poisoned mean must never be admitted).
pub(crate) fn decode(bytes: &[u8]) -> Result<HashMap<KeyId, Mean>, StoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| StoreError::Backend)?;
    let mut means = HashMap::new();
    if text.is_empty() {
        return Ok(means);
    }
    for line in text.split('\n') {
        let mut parts = line.split(SEP);
        let key = parts.next().ok_or(StoreError::Backend)?;
        let mut chars = key.chars();
        let ch = chars.next().ok_or(StoreError::Backend)?;
        if chars.next().is_some() {
            return Err(StoreError::Backend); // key field must be exactly one char
        }
        let dx: f32 = parse(parts.next())?;
        let dy: f32 = parse(parts.next())?;
        let count: u64 = parse(parts.next())?;
        if parts.next().is_some() {
            return Err(StoreError::Backend); // trailing field
        }
        if !dx.is_finite() || !dy.is_finite() {
            return Err(StoreError::Backend);
        }
        means.insert(KeyId(ch), Mean { dx, dy, count });
    }
    Ok(means)
}

/// Parse a required numeric field, mapping absence or a bad value to a backend
/// corruption error.
fn parse<T: std::str::FromStr>(field: Option<&str>) -> Result<T, StoreError> {
    field
        .ok_or(StoreError::Backend)?
        .parse()
        .map_err(|_| StoreError::Backend)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn model(pairs: &[(char, f32, f32, u64)]) -> HashMap<KeyId, Mean> {
        pairs
            .iter()
            .map(|&(c, dx, dy, count)| (KeyId(c), Mean { dx, dy, count }))
            .collect()
    }

    #[test]
    fn empty_model_encodes_to_no_bytes_and_back() {
        let bytes = encode(&HashMap::new());
        assert!(bytes.is_empty());
        assert_eq!(decode(&bytes).unwrap(), HashMap::new());
    }

    #[test]
    fn encoding_is_deterministic_and_sorted_by_key() {
        let m = model(&[('b', 2.0, 0.0, 3), ('a', 1.0, -1.0, 2)]);
        assert_eq!(encode(&m), b"a\t1\t-1\t2\nb\t2\t0\t3");
    }

    #[test]
    fn model_round_trips() {
        let m = model(&[('a', 2.5, -3.25, 37), ('z', -0.5, 4.0, 1)]);
        let bytes = encode(&m);
        assert_eq!(decode(&bytes).unwrap(), m);
    }

    #[test]
    fn fractional_offsets_survive_the_round_trip_exactly() {
        // A running mean like 10/3 must decode back to the identical f32.
        let m = model(&[('q', 10.0_f32 / 3.0, 1.0_f32 / 7.0, 42)]);
        assert_eq!(decode(&encode(&m)).unwrap(), m);
    }

    #[test]
    fn decode_rejects_non_utf8() {
        assert_eq!(decode(&[0xff]).err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_multi_char_key() {
        assert_eq!(decode(b"ab\t1\t1\t1").err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_missing_field() {
        assert_eq!(decode(b"a\t1\t1").err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_trailing_field() {
        assert_eq!(decode(b"a\t1\t1\t1\tx").err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_non_numeric_offset() {
        assert_eq!(decode(b"a\tNaN?\t1\t1").err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_non_finite_offset() {
        // "inf" parses as f32::INFINITY but must not be admitted as a mean.
        assert_eq!(decode(b"a\tinf\t1\t1").err(), Some(StoreError::Backend));
    }
}
