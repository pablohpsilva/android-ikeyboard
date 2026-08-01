//! Key layouts and geometry.
//!
//! A `Layout` is a set of `Key` rectangles positioned on the keyboard surface.
//! This crate is pure data + geometry: no touch decoding (that is
//! `input-decoder`'s job), no I/O, no Android types (SEDD §5.2, §5.5 rule 2).

mod direction;
mod kind;
mod qwerty;
mod scripts;
mod standard;

pub use direction::Direction;
pub use kind::LayoutKind;
pub use scripts::LatinLayout;

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
        Self {
            id,
            x,
            y,
            width,
            height,
        }
    }

    /// The geometric center of the key, used by decoders as the key's
    /// representative point.
    #[must_use]
    pub fn center(&self) -> TouchPoint {
        TouchPoint::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// A positioned set of keys, tagged with the character family it presents
/// ([`LayoutKind`]) and the reading direction of its script ([`Direction`]).
///
/// The direction is a marker only: it makes the port RTL-ready (RTL locales can
/// be tagged) without implementing bidirectional reordering, which is deferred
/// until the launch language set is fixed (ADR-16, BR-53*).
#[derive(Debug, Clone, Default)]
pub struct Layout {
    keys: Vec<Key>,
    kind: LayoutKind,
    direction: Direction,
}

impl Layout {
    /// A left-to-right alphabetic layout from `keys`. Kind defaults to
    /// [`LayoutKind::Alpha`] and direction to [`Direction::Ltr`]; use
    /// [`Layout::with_direction`] to tag it RTL.
    #[must_use]
    pub fn new(keys: Vec<Key>) -> Self {
        Self {
            keys,
            kind: LayoutKind::Alpha,
            direction: Direction::Ltr,
        }
    }

    #[must_use]
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// Which character family this layout presents.
    #[must_use]
    pub const fn kind(&self) -> LayoutKind {
        self.kind
    }

    /// The reading direction of this layout's script. Marker only — no bidi
    /// reordering is performed (ADR-16, BR-53*).
    #[must_use]
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    /// Return this layout tagged with `direction`. This records the reading
    /// direction (making RTL locales expressible) but performs **no** glyph
    /// reordering; bidi rendering is deferred (ADR-16, BR-53*).
    #[must_use]
    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
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

    /// Logical bounds (far right/bottom edge over all keys), or `(0,0)` if empty.
    /// `Key` fields are public (`x, y, width, height`), so the true rect edge is
    /// `x + width` / `y + height` — NOT `2·center`, which overshoots off-origin keys.
    fn bounds(&self) -> (f32, f32) {
        self.keys.iter().fold((0.0_f32, 0.0_f32), |(mx, my), k| {
            (mx.max(k.x + k.width), my.max(k.y + k.height))
        })
    }

    /// Map a surface-local pixel to `[-1, 1]` per axis. `(0,0)` for an empty layout.
    #[must_use]
    pub fn normalize(&self, x: f32, y: f32) -> (f32, f32) {
        let (bx, by) = self.bounds();
        if bx <= 0.0 || by <= 0.0 {
            return (0.0, 0.0);
        }
        ((x / bx) * 2.0 - 1.0, (y / by) * 2.0 - 1.0)
    }

    /// Centre of the key that commits `ch` (matched via `KeyId::ch`), or `None` if
    /// no key on this page commits it.
    #[must_use]
    pub fn center_of(&self, ch: char) -> Option<TouchPoint> {
        self.keys.iter().find(|k| k.id.ch() == ch).map(Key::center)
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

    #[test]
    fn a_plain_layout_is_alpha_and_left_to_right_by_default() {
        // The additive kind/direction markers must not disturb existing callers:
        // `new` and `default` stay Alpha/Ltr so the tracer bullet is unaffected.
        assert_eq!(Layout::default().kind(), LayoutKind::Alpha);
        assert_eq!(Layout::default().direction(), Direction::Ltr);
        assert_eq!(Layout::qwerty_tracer_row().kind(), LayoutKind::Alpha);
        assert_eq!(Layout::qwerty_tracer_row().direction(), Direction::Ltr);
    }

    #[test]
    fn normalize_maps_bounds_to_unit_range() {
        let l = Layout::qwerty();
        // Far bottom-right corner (max key right/bottom edge) maps to ~(1,1).
        let (bx, by) = l.keys().iter().fold((0.0_f32, 0.0_f32), |(mx, my), k| {
            (mx.max(k.x + k.width), my.max(k.y + k.height))
        });
        let (ex, ey) = l.normalize(bx, by);
        assert!(
            (ex - 1.0).abs() < 1e-3 && (ey - 1.0).abs() < 1e-3,
            "corner near 1: {ex},{ey}"
        );
        let (cx, cy) = l.normalize(bx / 2.0, by / 2.0);
        assert!(
            cx.abs() < 0.05 && cy.abs() < 0.05,
            "centre near origin: {cx},{cy}"
        );
    }

    #[test]
    fn center_of_returns_a_known_key_and_none_for_absent() {
        let l = Layout::qwerty();
        assert!(l.center_of('q').is_some());
        assert_eq!(l.center_of('€'), None); // not on the qwerty alpha page
    }

    #[test]
    fn normalize_never_panics_on_empty_layout() {
        let l = Layout::default(); // empty
        assert_eq!(l.normalize(10.0, 10.0), (0.0, 0.0));
    }

    #[test]
    fn with_direction_tags_rtl_without_touching_the_keys() {
        // RTL-readiness (BR-53*): a layout can be *marked* RTL. The marker is the
        // only change — no keys are reordered (bidi deferred, ADR-16).
        let ltr = Layout::qwerty_tracer_row();
        let ids_before: Vec<char> = ltr.keys().iter().map(|k| k.id.ch()).collect();
        let rtl = ltr.with_direction(Direction::Rtl);
        assert_eq!(rtl.direction(), Direction::Rtl);
        assert!(rtl.direction().is_rtl());
        let ids_after: Vec<char> = rtl.keys().iter().map(|k| k.id.ch()).collect();
        assert_eq!(
            ids_before, ids_after,
            "with_direction must not reorder keys"
        );
    }
}
