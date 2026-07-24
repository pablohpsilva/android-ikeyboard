//! Reading direction of a layout's script (a marker, not an algorithm).

/// The reading direction a layout's script runs in.
///
/// This is a **marker only**. It records which way the script reads so callers
/// and any future renderer can branch on it, and so RTL locales can be *tagged*
/// today. Bidirectional (bidi) reordering of mixed LTR/RTL runs is **not**
/// implemented here: per ADR-16 it is deferred until the launch language set is
/// fixed (BR-53*). Storing the direction now keeps the port RTL-ready without
/// committing the domain to a reordering algorithm it cannot yet validate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// Left-to-right scripts (Latin, digits, the symbol pages).
    #[default]
    Ltr,
    /// Right-to-left scripts (Arabic, Hebrew). Marker only — see the type docs:
    /// no glyph reordering happens here yet (ADR-16).
    Rtl,
}

impl Direction {
    /// `true` for a right-to-left script.
    #[must_use]
    pub const fn is_rtl(self) -> bool {
        matches!(self, Direction::Rtl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_defaults_to_left_to_right() {
        assert_eq!(Direction::default(), Direction::Ltr);
    }

    #[test]
    fn is_rtl_is_true_only_for_rtl() {
        assert!(Direction::Rtl.is_rtl());
        assert!(!Direction::Ltr.is_rtl());
    }
}
