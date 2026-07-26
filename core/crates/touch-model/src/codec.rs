//! Byte encoding for the tap model's single blob, done by hand (no serialization
//! crate), mirroring `personalization`'s codec.
//!
//! The whole model is serialized into one newline-joined UTF-8 blob so it can be
//! persisted with a single atomic [`put`](featherkey_contracts::SecureStore::put).
//! Each line is one key's learned mean and covariance co-moments (the `v2`
//! encoding):
//!
//! ```text
//! <key char>\t<dx>\t<dy>\t<count>\t<m2xx>\t<m2yy>\t<m2xy>
//! ```
//!
//! e.g. `"a\t2.5\t-1\t37\t4\t9\t0"`. Layout key characters are
//! letters/digits/punctuation and never contain the field separator (`\t`) or a
//! newline, so records stay unambiguous. Lines are emitted in ascending key-char
//! order, so equal models encode to identical bytes (deterministic). Floats use
//! Rust's shortest round-tripping `Display`/`FromStr`, so a decode reproduces the
//! exact `f32`.
//!
//! The prior `v1` encoding was mean-only (`<ch>\t<dx>\t<dy>\t<count>`, four
//! fields). [`decode_v1`] still reads it so a model persisted before covariance
//! landed loads with zero co-moments; the crate only ever *writes* `v2`.
//!
//! An empty model encodes to zero bytes and back. Decoding returns a value on
//! every input: a corrupt blob yields [`StoreError::Backend`] rather than a
//! panic (SEDD §5.5 r3).

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use featherkey_contracts::StoreError;
use featherkey_kernel::KeyId;

use crate::mean::Mean;

/// Field separator inside a line.
const SEP: char = '\t';

/// Encode every key's mean and covariance co-moments into one `v2` blob, ordered
/// by key char for determinism.
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
        let _ = write!(
            out,
            "{ch}{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{}",
            m.dx, m.dy, m.count, m.m2xx, m.m2yy, m.m2xy
        );
    }
    out.into_bytes()
}

/// Decode a `v2` blob back into the per-key means and covariance co-moments.
///
/// # Errors
/// [`StoreError::Backend`] if `bytes` is not valid UTF-8, a line is malformed
/// (wrong field count, key not exactly one char, unparseable number), or a
/// decoded offset or co-moment is non-finite (a poisoned accumulator must never
/// be admitted).
pub(crate) fn decode(bytes: &[u8]) -> Result<HashMap<KeyId, Mean>, StoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| StoreError::Backend)?;
    let mut means = HashMap::new();
    if text.is_empty() {
        return Ok(means);
    }
    for line in text.split('\n') {
        let mut parts = line.split(SEP);
        let ch = key_char(parts.next())?;
        let dx: f32 = parse(parts.next())?;
        let dy: f32 = parse(parts.next())?;
        let count: u64 = parse(parts.next())?;
        let m2xx: f32 = parse(parts.next())?;
        let m2yy: f32 = parse(parts.next())?;
        let m2xy: f32 = parse(parts.next())?;
        if parts.next().is_some() {
            return Err(StoreError::Backend); // trailing field
        }
        if !dx.is_finite() || !dy.is_finite() {
            return Err(StoreError::Backend);
        }
        if !m2xx.is_finite() || !m2yy.is_finite() || !m2xy.is_finite() {
            return Err(StoreError::Backend);
        }
        means.insert(
            KeyId(ch),
            Mean {
                dx,
                dy,
                count,
                m2xx,
                m2yy,
                m2xy,
            },
        );
    }
    Ok(means)
}

/// Decode a legacy `v1` blob (mean + count only, four fields per line) into the
/// per-key means, leaving every covariance co-moment zero. Used as the fallback
/// when no `v2` blob is present so a model persisted before covariance landed
/// still loads.
///
/// # Errors
/// [`StoreError::Backend`] on the same conditions as [`decode`] (non-UTF-8,
/// malformed line, non-finite offset).
pub(crate) fn decode_v1(bytes: &[u8]) -> Result<HashMap<KeyId, Mean>, StoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| StoreError::Backend)?;
    let mut means = HashMap::new();
    if text.is_empty() {
        return Ok(means);
    }
    for line in text.split('\n') {
        let mut parts = line.split(SEP);
        let ch = key_char(parts.next())?;
        let dx: f32 = parse(parts.next())?;
        let dy: f32 = parse(parts.next())?;
        let count: u64 = parse(parts.next())?;
        if parts.next().is_some() {
            return Err(StoreError::Backend); // trailing field
        }
        if !dx.is_finite() || !dy.is_finite() {
            return Err(StoreError::Backend);
        }
        // A v1 blob carried no spread: co-moments load as zero.
        means.insert(
            KeyId(ch),
            Mean {
                dx,
                dy,
                count,
                ..Default::default()
            },
        );
    }
    Ok(means)
}

/// Parse the key field: exactly one char, else a backend corruption error.
fn key_char(field: Option<&str>) -> Result<char, StoreError> {
    let key = field.ok_or(StoreError::Backend)?;
    let mut chars = key.chars();
    let ch = chars.next().ok_or(StoreError::Backend)?;
    if chars.next().is_some() {
        return Err(StoreError::Backend); // key field must be exactly one char
    }
    Ok(ch)
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

    /// Build a model of mean-only keys (co-moments zero), matching a fresh or
    /// single-observation key.
    fn model(pairs: &[(char, f32, f32, u64)]) -> HashMap<KeyId, Mean> {
        pairs
            .iter()
            .map(|&(c, dx, dy, count)| {
                (
                    KeyId(c),
                    Mean {
                        dx,
                        dy,
                        count,
                        ..Default::default()
                    },
                )
            })
            .collect()
    }

    /// Build a model whose keys carry covariance co-moments as well.
    fn model_cov(pairs: &[(char, f32, f32, u64, f32, f32, f32)]) -> HashMap<KeyId, Mean> {
        pairs
            .iter()
            .map(|&(c, dx, dy, count, m2xx, m2yy, m2xy)| {
                (
                    KeyId(c),
                    Mean {
                        dx,
                        dy,
                        count,
                        m2xx,
                        m2yy,
                        m2xy,
                    },
                )
            })
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
        assert_eq!(encode(&m), b"a\t1\t-1\t2\t0\t0\t0\nb\t2\t0\t3\t0\t0\t0");
    }

    #[test]
    fn model_round_trips() {
        let m = model_cov(&[
            ('a', 2.5, -3.25, 37, 4.0, 9.0, -1.5),
            ('z', -0.5, 4.0, 2, 1.0, 2.0, 0.5),
        ]);
        let bytes = encode(&m);
        assert_eq!(decode(&bytes).unwrap(), m);
    }

    #[test]
    fn fractional_offsets_and_comoments_survive_the_round_trip_exactly() {
        // A running mean/co-moment like 10/3 must decode back to the identical f32.
        let m = model_cov(&[(
            'q',
            10.0_f32 / 3.0,
            1.0_f32 / 7.0,
            42,
            2.0_f32 / 3.0,
            5.0_f32 / 9.0,
            -1.0_f32 / 11.0,
        )]);
        assert_eq!(decode(&encode(&m)).unwrap(), m);
    }

    #[test]
    fn decode_rejects_non_utf8() {
        assert_eq!(decode(&[0xff]).err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_multi_char_key() {
        assert_eq!(
            decode(b"ab\t1\t1\t1\t0\t0\t0").err(),
            Some(StoreError::Backend)
        );
    }

    #[test]
    fn decode_rejects_a_missing_field() {
        assert_eq!(decode(b"a\t1\t1\t1\t0\t0").err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_trailing_field() {
        assert_eq!(
            decode(b"a\t1\t1\t1\t0\t0\t0\tx").err(),
            Some(StoreError::Backend)
        );
    }

    #[test]
    fn decode_rejects_a_non_numeric_offset() {
        assert_eq!(
            decode(b"a\tNaN?\t1\t1\t0\t0\t0").err(),
            Some(StoreError::Backend)
        );
    }

    #[test]
    fn decode_rejects_a_non_finite_offset() {
        // "inf" parses as f32::INFINITY but must not be admitted as a mean.
        assert_eq!(
            decode(b"a\tinf\t1\t1\t0\t0\t0").err(),
            Some(StoreError::Backend)
        );
    }

    #[test]
    fn decode_rejects_a_non_finite_comoment() {
        // A poisoned co-moment must never be admitted either.
        assert_eq!(
            decode(b"a\t1\t1\t2\tinf\t0\t0").err(),
            Some(StoreError::Backend)
        );
    }

    #[test]
    fn decode_v1_reads_a_legacy_mean_only_blob_with_zero_comoments() {
        // v1 lines are four fields; co-moments load as zero.
        let decoded = decode_v1(b"a\t1\t-1\t2\nb\t2\t0\t3").unwrap();
        assert_eq!(decoded, model(&[('a', 1.0, -1.0, 2), ('b', 2.0, 0.0, 3)]));
    }

    #[test]
    fn decode_v1_empty_blob_is_an_empty_model() {
        assert_eq!(decode_v1(b"").unwrap(), HashMap::new());
    }

    #[test]
    fn decode_v1_rejects_a_v2_shaped_line() {
        // A seven-field v2 line has a trailing field under the v1 reader.
        assert_eq!(
            decode_v1(b"a\t1\t1\t1\t0\t0\t0").err(),
            Some(StoreError::Backend)
        );
    }

    #[test]
    fn decode_v1_rejects_non_utf8() {
        assert_eq!(decode_v1(&[0xff]).err(), Some(StoreError::Backend));
    }
}
