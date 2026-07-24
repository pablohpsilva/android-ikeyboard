//! The built-in non-alphabetic pages: numbers, symbols, punctuation (BR-47).
//!
//! Like [`crate::Layout::qwerty_tracer_row`], these are deterministic, data-only
//! fixtures — a single edge-to-edge row of 100×120 px keys laid out from the
//! origin. They are geometry, not locale policy; production pages are loaded
//! per-locale. Each page is tagged with its [`LayoutKind`] and left-to-right
//! [`Direction`]; RTL locales tag an existing page via
//! [`crate::Layout::with_direction`] (bidi reordering deferred, ADR-16 / BR-53*).

use featherkey_kernel::KeyId;

use crate::{Direction, Key, Layout, LayoutKind};

impl Layout {
    /// The numeric page: `1 2 3 4 5 6 7 8 9 0` in a single left-to-right row of
    /// 100×120 px keys, laid out edge-to-edge from the origin (BR-47).
    #[must_use]
    pub fn numeric() -> Self {
        Self::single_row(
            &['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'],
            LayoutKind::Numeric,
        )
    }

    /// The symbols page: common symbols and punctuation
    /// (`. , ? ! ' @ # $ & *`) in a single left-to-right row of 100×120 px keys
    /// laid out edge-to-edge from the origin (BR-47).
    #[must_use]
    pub fn symbols() -> Self {
        Self::single_row(
            &['.', ',', '?', '!', '\'', '@', '#', '$', '&', '*'],
            LayoutKind::Symbols,
        )
    }

    /// Build a single left-to-right row of 100×120 px keys from `chars`, tagged
    /// with `kind`. Shared by the built-in non-alphabetic pages.
    fn single_row(chars: &[char], kind: LayoutKind) -> Self {
        let keys = chars
            .iter()
            .enumerate()
            .map(|(i, &c)| Key::new(KeyId(c), i as f32 * 100.0, 0.0, 100.0, 120.0))
            .collect();
        Self { keys, kind, direction: Direction::Ltr }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use featherkey_kernel::TouchPoint;

    #[test]
    fn numeric_page_is_the_digit_row_left_to_right() {
        let l = Layout::numeric();
        assert_eq!(l.kind(), LayoutKind::Numeric);
        assert_eq!(l.direction(), Direction::Ltr);
        assert!(!l.is_empty());
        let digits: Vec<char> = l.keys().iter().map(|k| k.id.ch()).collect();
        assert_eq!(digits, vec!['1', '2', '3', '4', '5', '6', '7', '8', '9', '0']);
    }

    #[test]
    fn symbols_page_carries_punctuation_and_symbols() {
        let l = Layout::symbols();
        assert_eq!(l.kind(), LayoutKind::Symbols);
        assert_eq!(l.direction(), Direction::Ltr);
        let syms: Vec<char> = l.keys().iter().map(|k| k.id.ch()).collect();
        assert_eq!(syms, vec!['.', ',', '?', '!', '\'', '@', '#', '$', '&', '*']);
    }

    #[test]
    fn standard_pages_keep_keys_edge_to_edge() {
        // The third numeric key ('3') occupies x in [200, 300); center (250, 60).
        let l = Layout::numeric();
        let third = l.keys()[2];
        assert_eq!(third.id, KeyId('3'));
        assert_eq!(third.x, 200.0);
        assert_eq!(third.center(), TouchPoint::new(250.0, 60.0));
    }
}
