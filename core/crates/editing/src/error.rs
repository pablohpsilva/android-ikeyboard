//! The error type shared by every editing operation.
//!
//! Cursor and selection helpers index `text` by byte offset. A caller can hand
//! us an offset that is past the end of the string or that lands in the middle
//! of a multi-byte UTF-8 scalar; both are *values*, not panics (SEDD §5.5
//! rule 3). Every public function returns `Result<_, EditError>` so the FFI seam
//! never has to unwind across a slicing panic.

use core::fmt;

/// Why an editing operation could not be performed for a given `(text, idx)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditError {
    /// The byte index was greater than `text.len()`.
    OutOfBounds,
    /// The byte index was within range but did not fall on a UTF-8 character
    /// boundary, so it could not name a valid cursor position.
    NotCharBoundary,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::OutOfBounds => f.write_str("cursor index past end of text"),
            EditError::NotCharBoundary => f.write_str("cursor index not on a char boundary"),
        }
    }
}

/// Validate that `idx` names a usable cursor position inside `text`.
///
/// Returns [`EditError::OutOfBounds`] when `idx > text.len()` and
/// [`EditError::NotCharBoundary`] when `idx` splits a multi-byte scalar. On
/// success `text` may be sliced at `idx` without panicking.
///
/// # Errors
/// See the variants above.
pub(crate) fn validate(text: &str, idx: usize) -> Result<(), EditError> {
    if idx > text.len() {
        return Err(EditError::OutOfBounds);
    }
    if !text.is_char_boundary(idx) {
        return Err(EditError::NotCharBoundary);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_zero_end_and_interior_boundaries() {
        // "café" with a precomposed é: bytes c a f é(2 bytes) => len 5.
        let text = "café";
        assert_eq!(validate(text, 0), Ok(()));
        assert_eq!(validate(text, 3), Ok(())); // just before 'é'
        assert_eq!(validate(text, text.len()), Ok(())); // end is valid
    }

    #[test]
    fn validate_rejects_index_past_the_end() {
        let text = "hi";
        assert_eq!(validate(text, 3), Err(EditError::OutOfBounds));
    }

    #[test]
    fn validate_rejects_a_mid_scalar_index() {
        // 'é' (U+00E9) occupies bytes 3..5; index 4 splits it.
        let text = "café";
        assert_eq!(validate(text, 4), Err(EditError::NotCharBoundary));
    }

    #[test]
    fn edit_error_displays_human_messages() {
        extern crate alloc;
        assert_eq!(
            alloc::format!("{}", EditError::OutOfBounds),
            "cursor index past end of text"
        );
        assert_eq!(
            alloc::format!("{}", EditError::NotCharBoundary),
            "cursor index not on a char boundary"
        );
    }
}
