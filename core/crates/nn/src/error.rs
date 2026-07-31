//! Error values for the neural codec. Errors are values here, never panics:
//! deserializing an untrusted or stale blob returns `Err`, it never aborts.

use core::fmt;

/// A neural-model (de)serialization failure. The blob was the wrong magic,
/// an unknown version, truncated, of a declared shape that did not match its
/// byte length, or carried trailing garbage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NnError {
    /// The serialized model blob is corrupt or unreadable.
    Blob,
}

impl fmt::Display for NnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blob => f.write_str("neural model blob is corrupt or unreadable"),
        }
    }
}

impl std::error::Error for NnError {}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::NnError;

    #[test]
    fn blob_is_cloneable_comparable_and_displayable() {
        let e = NnError::Blob;
        assert_eq!(e.clone(), NnError::Blob);
        assert_eq!(format!("{e}"), "neural model blob is corrupt or unreadable");
        assert_eq!(format!("{e:?}"), "Blob");
    }
}
