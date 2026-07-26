//! Byte encoding for the correction model's single blob, done by hand (no
//! serialization crate, per the crate brief), mirroring `personalization`'s and
//! `touch-model`'s codecs.
//!
//! Both learned maps — the strip-pick preferences *and* the unwanted words — are
//! serialized into **one** newline-joined UTF-8 blob so they can be persisted
//! with a single atomic [`put`](featherkey_contracts::SecureStore::put). Each
//! line is one record, classified purely by its field (`\t`) count:
//!
//! * a **preference** record has three fields:
//!   `"<count>\t<prefix>\t<picked>"`, e.g. `"2\tte\tteh"`;
//! * an **unwanted** record has two fields: `"<count>\t<word>"`, e.g.
//!   `"1\tducking"`.
//!
//! Prefixes and words never contain a tab or newline (the model rejects any that
//! do on the way in), so the two record kinds stay unambiguous within the one
//! blob. Preference lines are emitted first (in `(prefix, picked)` `BTreeMap`
//! order), then unwanted lines (in `BTreeMap` order), so equal models encode to
//! identical bytes (deterministic).
//!
//! An empty model encodes to zero bytes, and zero bytes decodes back to an empty
//! model — the round-trip the model relies on.
//!
//! Decoding returns a value on every input: a corrupt blob yields
//! [`StoreError::Backend`] rather than a panic (SEDD §5.5 r3).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use featherkey_contracts::StoreError;

/// The field separator inside a line. Its count on a line also distinguishes a
/// preference record (three fields) from an unwanted record (two fields).
const FIELD_SEP: char = '\t';

/// Encode the whole model — preferences then unwanted words — into one blob.
///
/// Preference entries are emitted in `(prefix, picked)` `BTreeMap` order and
/// unwanted entries in `BTreeMap` order, so the encoding is deterministic (stable
/// bytes for equal models).
pub(crate) fn encode_model(
    prefs: &BTreeMap<String, BTreeMap<String, u32>>,
    unwanted: &BTreeMap<String, u32>,
) -> Vec<u8> {
    let mut out = String::new();
    let mut first = true;
    for (prefix, picks) in prefs {
        for (picked, count) in picks {
            if !first {
                out.push('\n');
            }
            first = false;
            // Writing to a `String` is infallible; the result is only ever `Ok`.
            let _ = write!(out, "{count}{FIELD_SEP}{prefix}{FIELD_SEP}{picked}");
        }
    }
    for (word, count) in unwanted {
        if !first {
            out.push('\n');
        }
        first = false;
        let _ = write!(out, "{count}{FIELD_SEP}{word}");
    }
    out.into_bytes()
}

/// Decode a model blob back into its preference and unwanted maps.
///
/// # Errors
/// [`StoreError::Backend`] if `bytes` is not valid UTF-8, a line has the wrong
/// number of fields, or a count field is not a `u32`.
#[allow(clippy::type_complexity)]
pub(crate) fn decode_model(
    bytes: &[u8],
) -> Result<(BTreeMap<String, BTreeMap<String, u32>>, BTreeMap<String, u32>), StoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| StoreError::Backend)?;
    let mut prefs: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
    let mut unwanted: BTreeMap<String, u32> = BTreeMap::new();
    if text.is_empty() {
        return Ok((prefs, unwanted));
    }
    for line in text.split('\n') {
        let mut parts = line.split(FIELD_SEP);
        let count = parts.next().ok_or(StoreError::Backend)?;
        let count: u32 = count.parse().map_err(|_| StoreError::Backend)?;
        let second = parts.next().ok_or(StoreError::Backend)?;
        match parts.next() {
            Some(picked) => {
                // Three fields => preference record; reject a fourth field.
                if parts.next().is_some() {
                    return Err(StoreError::Backend);
                }
                prefs
                    .entry(second.to_owned())
                    .or_default()
                    .insert(picked.to_owned(), count);
            }
            None => {
                // Two fields => unwanted record.
                unwanted.insert(second.to_owned(), count);
            }
        }
    }
    Ok((prefs, unwanted))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn prefs(entries: &[(&str, &str, u32)]) -> BTreeMap<String, BTreeMap<String, u32>> {
        let mut m: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
        for (prefix, picked, count) in entries {
            m.entry((*prefix).to_owned())
                .or_default()
                .insert((*picked).to_owned(), *count);
        }
        m
    }

    fn unwanted(entries: &[(&str, u32)]) -> BTreeMap<String, u32> {
        entries.iter().map(|(w, c)| ((*w).to_owned(), *c)).collect()
    }

    #[test]
    fn model_round_trips() {
        let p = prefs(&[("te", "teh", 2), ("ca", "cat", 1)]);
        let u = unwanted(&[("ducking", 1), ("teh", 3)]);
        let bytes = encode_model(&p, &u);
        assert_eq!(decode_model(&bytes).unwrap(), (p, u));
    }

    #[test]
    fn empty_model_encodes_to_no_bytes_and_back() {
        let bytes = encode_model(&BTreeMap::new(), &BTreeMap::new());
        assert!(bytes.is_empty());
        assert_eq!(
            decode_model(&bytes).unwrap(),
            (BTreeMap::new(), BTreeMap::new())
        );
    }

    #[test]
    fn encoding_is_deterministic_and_sorted() {
        // Preferences first, ordered by (prefix, picked); then unwanted, ordered
        // by word.
        let p = prefs(&[("te", "the", 1), ("te", "teh", 2), ("ca", "cat", 4)]);
        let u = unwanted(&[("z", 1), ("a", 5)]);
        assert_eq!(
            encode_model(&p, &u),
            b"4\tca\tcat\n2\tte\tteh\n1\tte\tthe\n5\ta\n1\tz".to_vec()
        );
    }

    #[test]
    fn prefs_only_model_round_trips() {
        let p = prefs(&[("te", "teh", 2)]);
        let bytes = encode_model(&p, &BTreeMap::new());
        assert_eq!(bytes, b"2\tte\tteh");
        assert_eq!(decode_model(&bytes).unwrap(), (p, BTreeMap::new()));
    }

    #[test]
    fn unwanted_only_model_round_trips() {
        let u = unwanted(&[("ducking", 3)]);
        let bytes = encode_model(&BTreeMap::new(), &u);
        assert_eq!(bytes, b"3\tducking");
        assert_eq!(decode_model(&bytes).unwrap(), (BTreeMap::new(), u));
    }

    #[test]
    fn a_word_can_be_both_a_pick_and_unwanted() {
        // The same string can be a picked completion and, elsewhere, unwanted.
        let p = prefs(&[("te", "teh", 2)]);
        let u = unwanted(&[("teh", 1)]);
        let bytes = encode_model(&p, &u);
        assert_eq!(decode_model(&bytes).unwrap(), (p, u));
    }

    #[test]
    fn decode_rejects_non_utf8() {
        assert_eq!(decode_model(&[0xff]).err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_non_numeric_count() {
        assert_eq!(
            decode_model(b"NaN\tte\tteh").err(),
            Some(StoreError::Backend)
        );
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
    fn decode_rejects_a_line_with_only_a_count() {
        assert_eq!(decode_model(b"1").err(), Some(StoreError::Backend));
    }

    #[test]
    fn decode_rejects_a_four_field_line() {
        assert_eq!(
            decode_model(b"1\ta\tb\tc").err(),
            Some(StoreError::Backend)
        );
    }

    proptest! {
        /// The round-trip invariant: for any storable tokens (no `\n`/`\t`,
        /// non-empty) with arbitrary counts, encode-then-decode reproduces the
        /// exact same model.
        #[test]
        fn encode_then_decode_is_identity(
            raw_prefs in prop::collection::vec(
                ("[^\n\t]{1,6}", "[^\n\t]{1,6}", any::<u32>()), 0..8),
            unwanted in prop::collection::btree_map("[^\n\t]{1,6}", any::<u32>(), 0..8),
        ) {
            let mut prefs: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
            for (prefix, picked, count) in raw_prefs {
                prefs.entry(prefix).or_default().insert(picked, count);
            }
            let bytes = encode_model(&prefs, &unwanted);
            let decoded = decode_model(&bytes).expect("valid blob decodes");
            prop_assert_eq!(decoded, (prefs, unwanted));
        }
    }
}
