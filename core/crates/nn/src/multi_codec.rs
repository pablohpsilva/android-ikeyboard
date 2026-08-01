//! Versioned, dependency-free serialization for [`MlpMulti`]. Hand-rolled
//! little-endian bytes, mirroring `codec.rs` (the scalar [`Mlp`] codec) but
//! with a **distinct magic** (`FKNM` vs `Mlp`'s `FKNN`) so the two blob types
//! can never be confused. Deserialization is total: any wrong magic, unknown
//! version, truncated header, shape/length mismatch, or trailing garbage
//! yields `Err(NnError::Blob)`, never a panic.

use super::MlpMulti;
use crate::NnError;

/// Blob magic: identifies a FeatherKey multi-output neural-model blob.
const MAGIC: [u8; 4] = *b"FKNM";
/// Current on-disk format version.
const VERSION: u16 = 1;
/// Fixed header: magic (4) + version (2) + inputs (2) + hidden (2) + outputs (2).
const HEADER_LEN: usize = 12;
/// Bytes per serialized weight (`f32` little-endian).
const F32_LEN: usize = 4;

impl MlpMulti {
    /// Serialize to a self-describing, versioned little-endian blob:
    /// magic `FKNM` + `u16` version + `u16` inputs + `u16` hidden + `u16`
    /// outputs, then every `f32` in `[w1.., b1.., w2.., b2..]` order. The
    /// inverse is [`MlpMulti::from_bytes`].
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let float_count = self.w1.len() + self.b1.len() + self.w2.len() + self.b2.len();
        let mut out = Vec::with_capacity(HEADER_LEN + float_count * F32_LEN);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(self.inputs() as u16).to_le_bytes());
        out.extend_from_slice(&(self.hidden() as u16).to_le_bytes());
        out.extend_from_slice(&(self.outputs() as u16).to_le_bytes());
        for &v in self
            .w1
            .iter()
            .chain(&self.b1)
            .chain(&self.w2)
            .chain(&self.b2)
        {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// Reconstruct an [`MlpMulti`] from a blob produced by
    /// [`MlpMulti::to_bytes`].
    ///
    /// Total and panic-free: returns `Err(NnError::Blob)` for a wrong magic,
    /// an unknown version, a truncated header, a declared shape whose implied
    /// byte length does not match the body, or any trailing garbage. Every
    /// slice read is bounds-checked.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NnError> {
        let header: [u8; HEADER_LEN] = bytes
            .get(..HEADER_LEN)
            .and_then(|h| h.try_into().ok())
            .ok_or(NnError::Blob)?;
        if header[0..4] != MAGIC {
            return Err(NnError::Blob);
        }
        if u16::from_le_bytes([header[4], header[5]]) != VERSION {
            return Err(NnError::Blob);
        }
        let inputs = u16::from_le_bytes([header[6], header[7]]) as usize;
        let hidden = u16::from_le_bytes([header[8], header[9]]) as usize;
        let outputs = u16::from_le_bytes([header[10], header[11]]) as usize;

        let (w1_len, b1_len, w2_len, b2_len, float_count) = shape(inputs, hidden, outputs)?;
        let expected_body = float_count.checked_mul(F32_LEN).ok_or(NnError::Blob)?;
        let body = bytes.get(HEADER_LEN..).unwrap_or(&[]);
        // A single exact-length check rejects both truncation and trailing bytes.
        if body.len() != expected_body {
            return Err(NnError::Blob);
        }

        let floats: Vec<f32> = body
            .chunks_exact(F32_LEN)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        let end_b1 = w1_len + b1_len;
        let end_w2 = end_b1 + w2_len;
        let end_b2 = end_w2 + b2_len;
        let w1 = floats.get(..w1_len).ok_or(NnError::Blob)?.to_vec();
        let b1 = floats.get(w1_len..end_b1).ok_or(NnError::Blob)?.to_vec();
        let w2 = floats.get(end_b1..end_w2).ok_or(NnError::Blob)?.to_vec();
        let b2 = floats.get(end_w2..end_b2).ok_or(NnError::Blob)?.to_vec();

        Ok(Self::with_weights(w1, b1, w2, b2, inputs, hidden, outputs))
    }
}

/// Compute `(w1_len, b1_len, w2_len, b2_len, total_float_count)` for a
/// declared shape, rejecting any arithmetic overflow rather than panicking.
fn shape(
    inputs: usize,
    hidden: usize,
    outputs: usize,
) -> Result<(usize, usize, usize, usize, usize), NnError> {
    let w1_len = hidden.checked_mul(inputs).ok_or(NnError::Blob)?;
    let w2_len = outputs.checked_mul(hidden).ok_or(NnError::Blob)?;
    let b1_len = hidden;
    let b2_len = outputs;
    let float_count = w1_len
        .checked_add(b1_len)
        .and_then(|n| n.checked_add(w2_len))
        .and_then(|n| n.checked_add(b2_len))
        .ok_or(NnError::Blob)?;
    Ok((w1_len, b1_len, w2_len, b2_len, float_count))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::MlpMulti;
    use crate::NnError;

    #[test]
    fn codec_round_trips() {
        let m = MlpMulti::with_weights(
            vec![0.1, 0.2, 0.3, 0.4],
            vec![0.5, 0.6],
            vec![0.7, 0.8, 0.9, 1.0, 1.1, 1.2],
            vec![0.1, 0.2, 0.3],
            2,
            2,
            3,
        );
        assert_eq!(MlpMulti::from_bytes(&m.to_bytes()).unwrap(), m);
    }

    #[test]
    fn codec_rejects_bad_magic_wrong_version_and_shape() {
        let m = MlpMulti::with_weights(
            vec![1.0],
            vec![0.0],
            vec![0.0, 0.0],
            vec![0.0, 0.0],
            1,
            1,
            2,
        );
        let mut bad = m.to_bytes();
        bad[0] = b'X';
        assert_eq!(MlpMulti::from_bytes(&bad).unwrap_err(), NnError::Blob);
        let mut ver = m.to_bytes();
        ver[4] = 0xFF;
        assert_eq!(MlpMulti::from_bytes(&ver).unwrap_err(), NnError::Blob);
        assert_eq!(
            MlpMulti::from_bytes(b"FKNM\x01\x00").unwrap_err(),
            NnError::Blob
        ); // truncated
    }

    #[test]
    fn from_bytes_rejects_full_length_bad_magic() {
        // A full, well-sized blob whose only defect is the magic — exercises
        // the magic-mismatch branch specifically (not the truncated-header
        // branch).
        let m = MlpMulti::with_weights(vec![1.0], vec![0.0], vec![1.0], vec![0.0], 1, 1, 1);
        let mut b = m.to_bytes();
        b[0] = b'Z';
        assert_eq!(MlpMulti::from_bytes(&b).unwrap_err(), NnError::Blob);
    }

    #[test]
    fn from_bytes_rejects_shape_length_mismatch() {
        // Valid 1x1x1 net, but rewrite the declared `inputs` to 2 so the
        // declared shape no longer matches the body's byte length.
        let m = MlpMulti::with_weights(vec![1.0], vec![0.0], vec![1.0], vec![0.0], 1, 1, 1);
        let mut b = m.to_bytes();
        b[6] = 2;
        b[7] = 0;
        assert_eq!(MlpMulti::from_bytes(&b).unwrap_err(), NnError::Blob);
    }

    #[test]
    fn from_bytes_rejects_trailing_garbage() {
        let m = MlpMulti::with_weights(vec![1.0, 2.0], vec![0.0], vec![1.0], vec![0.0], 2, 1, 1);
        let mut b = m.to_bytes();
        b.push(0x00);
        assert_eq!(MlpMulti::from_bytes(&b).unwrap_err(), NnError::Blob);
    }

    #[test]
    fn from_bytes_rejects_truncated_blob() {
        assert_eq!(MlpMulti::from_bytes(b"FKNM").unwrap_err(), NnError::Blob);
    }
}
