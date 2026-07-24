//! Key layouts and geometry.
//!
//! A `Layout` is a set of `Key` rectangles positioned on the keyboard surface.
//! This crate is pure data + geometry: no touch decoding (that is
//! `input-decoder`'s job), no I/O, no Android types (SEDD §5.2, §5.5 rule 2).

use featherkey_kernel::{KeyId, TouchPoint};

/// A single key: its identity and its rectangle on the surface.
///
/// The rectangle is `[x, x+width) × [y, y+height)` in surface-local pixels,
/// matching `TouchPoint`'s coordinate system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    pub id: KeyId,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Key {
    #[must_use]
    pub const fn new(id: KeyId, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { id, x, y, width, height }
    }

    /// The geometric center of the key, used by decoders as the key's
    /// representative point.
    #[must_use]
    pub fn center(&self) -> TouchPoint {
        TouchPoint::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// A positioned set of keys.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    keys: Vec<Key>,
}

impl Layout {
    #[must_use]
    pub fn new(keys: Vec<Key>) -> Self {
        Self { keys }
    }

    #[must_use]
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// A minimal single-row QWERTY fragment (`q w e r t`) used by the keystroke
    /// tracer bullet. Each key is 100×120 px, laid out edge-to-edge from the
    /// origin. Real layouts are data-driven and loaded per-locale; this is a
    /// deterministic fixture, not the production layout.
    #[must_use]
    pub fn qwerty_tracer_row() -> Self {
        let keys = ['q', 'w', 'e', 'r', 't']
            .into_iter()
            .enumerate()
            .map(|(i, c)| Key::new(KeyId(c), i as f32 * 100.0, 0.0, 100.0, 120.0))
            .collect();
        Self::new(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_center_is_the_rectangle_midpoint() {
        let k = Key::new(KeyId('e'), 200.0, 0.0, 100.0, 120.0);
        assert_eq!(k.center(), TouchPoint::new(250.0, 60.0));
    }

    #[test]
    fn empty_layout_reports_empty() {
        let l = Layout::default();
        assert!(l.is_empty());
        assert_eq!(l.keys().len(), 0);
    }

    #[test]
    fn tracer_row_has_five_keys_in_order() {
        let l = Layout::qwerty_tracer_row();
        assert!(!l.is_empty());
        let ids: Vec<char> = l.keys().iter().map(|k| k.id.ch()).collect();
        assert_eq!(ids, vec!['q', 'w', 'e', 'r', 't']);
    }

    #[test]
    fn tracer_row_keys_are_edge_to_edge() {
        let l = Layout::qwerty_tracer_row();
        // 'e' is the third key: x in [200, 300).
        let e = l.keys()[2];
        assert_eq!(e.x, 200.0);
        assert_eq!(e.center(), TouchPoint::new(250.0, 60.0));
    }
}
