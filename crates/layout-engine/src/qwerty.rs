//! The production alpha page: a real multi-row QWERTY letter block.
//!
//! Unlike [`crate::Layout::qwerty_tracer_row`] (a single 5-key fixture for the
//! decode tracer bullet), this is the full three-row letter layout the shell
//! renders and decodes against. Keys are 100×120 px; rows are staggered like a
//! physical keyboard (the home row is inset half a key, the bottom row a key and
//! a half). Non-character keys — space, backspace, enter, page-switch — are
//! **not** here: `KeyId` is a character today (kernel, v1.x adds variants), so the
//! shell owns those as function keys outside the decodable letter area.

use featherkey_kernel::KeyId;

use crate::{Key, Layout};

/// Key width/height in surface-local pixels (matches the other layouts).
const W: f32 = 100.0;
const H: f32 = 120.0;

impl Layout {
    /// The full QWERTY letter block: three staggered rows
    /// (`qwertyuiop` / `asdfghjkl` / `zxcvbnm`), 26 keys, laid out in a
    /// 1000×360 px logical space. This is the default alpha page.
    #[must_use]
    pub fn qwerty() -> Self {
        let mut keys = Vec::with_capacity(26);
        push_row(
            &mut keys,
            &['q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p'],
            0,
            0.0,
        );
        push_row(
            &mut keys,
            &['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'],
            1,
            W / 2.0,
        );
        push_row(&mut keys, &['z', 'x', 'c', 'v', 'b', 'n', 'm'], 2, W * 1.5);
        Self::new(keys)
    }
}

/// Append one staggered row of character keys at `row` (0-based), the first key
/// starting at horizontal offset `x0`.
fn push_row(keys: &mut Vec<Key>, chars: &[char], row: usize, x0: f32) {
    for (i, &c) in chars.iter().enumerate() {
        keys.push(Key::new(KeyId(c), x0 + i as f32 * W, row as f32 * H, W, H));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use featherkey_kernel::TouchPoint;

    #[test]
    fn qwerty_has_all_26_letters() {
        let l = Layout::qwerty();
        let mut chars: Vec<char> = l.keys().iter().map(|k| k.id.ch()).collect();
        chars.sort_unstable();
        assert_eq!(chars, ('a'..='z').collect::<Vec<_>>());
    }

    #[test]
    fn rows_are_staggered_like_a_physical_keyboard() {
        let l = Layout::qwerty();
        let find = |c: char| *l.keys().iter().find(|k| k.id.ch() == c).unwrap();
        // Top row starts at the origin; home row is inset half a key; bottom row
        // a key and a half.
        assert_eq!(find('q').x, 0.0);
        assert_eq!(find('a').x, 50.0);
        assert_eq!(find('z').x, 150.0);
        // Rows descend by one key height.
        assert_eq!(find('q').y, 0.0);
        assert_eq!(find('a').y, 120.0);
        assert_eq!(find('z').y, 240.0);
    }

    #[test]
    fn top_row_matches_the_tracer_prefix_so_decode_is_unchanged() {
        // q and w keep the tracer row's coordinates, so existing decode
        // behaviour (and the E-2 tap test) is preserved under the new default.
        let l = Layout::qwerty();
        let q = *l.keys().iter().find(|k| k.id.ch() == 'q').unwrap();
        let w = *l.keys().iter().find(|k| k.id.ch() == 'w').unwrap();
        assert_eq!(q.center(), TouchPoint::new(50.0, 60.0));
        assert_eq!(w.center(), TouchPoint::new(150.0, 60.0));
    }
}
