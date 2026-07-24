//! The family of characters a layout presents (its "page").

/// Which family of characters a [`crate::Layout`] presents.
///
/// A soft keyboard cycles between these pages — letters, digits, symbols — via
/// layout-switch keys. Tagging the page lets callers pick the right one and lets
/// tests assert which page a `Layout` represents without inspecting its keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutKind {
    /// Letters — the default alphabetic page (e.g. the QWERTY tracer row).
    #[default]
    Alpha,
    /// Digits `0`–`9`.
    Numeric,
    /// Punctuation and symbols.
    Symbols,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_defaults_to_alpha() {
        assert_eq!(LayoutKind::default(), LayoutKind::Alpha);
    }

    #[test]
    fn kinds_are_distinct() {
        assert_ne!(LayoutKind::Alpha, LayoutKind::Numeric);
        assert_ne!(LayoutKind::Numeric, LayoutKind::Symbols);
        assert_ne!(LayoutKind::Alpha, LayoutKind::Symbols);
    }
}
