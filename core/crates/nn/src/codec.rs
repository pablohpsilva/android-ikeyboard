//! Versioned, dependency-free serialization for [`Mlp`]. Hand-rolled little-
//! endian bytes — no serde, no bincode. Deserialization is total: any wrong
//! magic, unknown version, truncated header, shape/length mismatch, or trailing
//! garbage yields `Err(NnError::Blob)`, never a panic.

use super::Mlp;
use crate::NnError;

/// Blob magic: identifies a FeatherKey neural-model blob.
const MAGIC: [u8; 4] = *b"FKNN";
/// Current on-disk format version.
const VERSION: u16 = 1;
/// Fixed header: magic (4) + version (2) + inputs (2) + hidden (2).
const HEADER_LEN: usize = 10;
/// Bytes per serialized weight (`f32` little-endian).
const F32_LEN: usize = 4;

impl Mlp {
    /// Serialize to a self-describing, versioned little-endian blob:
    /// magic `FKNN` + `u16` version + `u16` inputs + `u16` hidden, then every
    /// `f32` in `[w1.., b1.., w2.., b2]` order. The inverse is [`Mlp::from_bytes`].
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let float_count = self.w1.len() + self.b1.len() + self.w2.len() + 1;
        let mut out = Vec::with_capacity(HEADER_LEN + float_count * F32_LEN);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(self.inputs as u16).to_le_bytes());
        out.extend_from_slice(&(self.hidden as u16).to_le_bytes());
        for &v in self.w1.iter().chain(&self.b1).chain(&self.w2) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.b2.to_le_bytes());
        out
    }

    /// Reconstruct an [`Mlp`] from a blob produced by [`Mlp::to_bytes`].
    ///
    /// Total and panic-free: returns `Err(NnError::Blob)` for a wrong magic, an
    /// unknown version, a truncated header, a declared shape whose implied byte
    /// length does not match the body, or any trailing garbage. Every slice read
    /// is bounds-checked.
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

        let (w1_len, float_count) = shape(inputs, hidden)?;
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

        let end_b1 = w1_len + hidden;
        let end_w2 = end_b1 + hidden;
        let w1 = floats.get(..w1_len).ok_or(NnError::Blob)?.to_vec();
        let b1 = floats.get(w1_len..end_b1).ok_or(NnError::Blob)?.to_vec();
        let w2 = floats.get(end_b1..end_w2).ok_or(NnError::Blob)?.to_vec();
        let b2 = *floats.get(end_w2).ok_or(NnError::Blob)?;

        Ok(Self::with_weights(w1, b1, w2, b2, inputs, hidden))
    }
}

/// Compute `(w1_len, total_float_count)` for a declared shape, rejecting any
/// arithmetic overflow rather than panicking. `float_count == w1_len + 2*hidden + 1`.
fn shape(inputs: usize, hidden: usize) -> Result<(usize, usize), NnError> {
    let w1_len = hidden.checked_mul(inputs).ok_or(NnError::Blob)?;
    let float_count = hidden
        .checked_mul(2)
        .and_then(|h2| h2.checked_add(w1_len))
        .and_then(|n| n.checked_add(1))
        .ok_or(NnError::Blob)?;
    Ok((w1_len, float_count))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::Mlp;
    use crate::NnError;

    #[test]
    fn bytes_round_trip() {
        let m = Mlp::from_linear(&[1.0, -2.0, 0.5], 0.3, 1.0, 100.0);
        assert_eq!(Mlp::from_bytes(&m.to_bytes()).unwrap(), m);
    }

    #[test]
    fn from_bytes_rejects_bad_magic() {
        assert_eq!(Mlp::from_bytes(b"XXXX\x01\x00").unwrap_err(), NnError::Blob);
    }

    #[test]
    fn from_bytes_rejects_full_length_bad_magic() {
        // A full, well-sized blob whose only defect is the magic — exercises the
        // magic-mismatch branch specifically (not the truncated-header branch).
        let mut b = Mlp::from_linear(&[1.0], 0.0, 1.0, 50.0).to_bytes();
        b[0] = b'Z';
        assert_eq!(Mlp::from_bytes(&b).unwrap_err(), NnError::Blob);
    }

    #[test]
    fn from_bytes_rejects_wrong_version() {
        let mut b = Mlp::from_linear(&[1.0], 0.0, 1.0, 50.0).to_bytes();
        b[4] = 0xFF; // corrupt version
        assert_eq!(Mlp::from_bytes(&b).unwrap_err(), NnError::Blob);
    }

    #[test]
    fn from_bytes_rejects_truncated_blob() {
        assert_eq!(Mlp::from_bytes(b"FKNN").unwrap_err(), NnError::Blob);
    }

    #[test]
    fn from_bytes_rejects_shape_length_mismatch() {
        // Valid 1x1 net, but rewrite the declared `inputs` to 2 so the declared
        // shape no longer matches the body's byte length.
        let mut b = Mlp::from_linear(&[1.0], 0.0, 1.0, 50.0).to_bytes();
        b[6] = 2;
        b[7] = 0;
        assert_eq!(Mlp::from_bytes(&b).unwrap_err(), NnError::Blob);
    }

    #[test]
    fn from_bytes_rejects_trailing_garbage() {
        let mut b = Mlp::from_linear(&[1.0, 2.0], 0.0, 1.0, 50.0).to_bytes();
        b.push(0x00);
        assert_eq!(Mlp::from_bytes(&b).unwrap_err(), NnError::Blob);
    }
}
