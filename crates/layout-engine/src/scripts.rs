//! Non-Latin and Latin-variant alpha pages.
//!
//! The alpha page the shell renders (and the core decodes against) is chosen by
//! the primary active language, so a Cyrillic or Greek locale gets a native
//! letter block and a French/German one gets its national Latin variant. Unlike
//! [`crate::Layout::qwerty`] these rows are laid out on a plain left-aligned grid
//! (`grid_row`): the shell centres each row independently when rendering, so
//! stagger is a QWERTY-only cosmetic and not needed here. Keys are 100×120 px to
//! match every other page.

use featherkey_kernel::KeyId;

use crate::{Key, Layout};

/// Key width/height in surface-local pixels (matches the other layouts).
const W: f32 = 100.0;
const H: f32 = 120.0;

impl Layout {
    /// The Russian ЙЦУКЕН Cyrillic block: three rows (12 / 11 / 9 keys, 32 total).
    #[must_use]
    pub fn cyrillic() -> Self {
        let mut keys = Vec::new();
        grid_row(&mut keys, "йцукенгшщзхъ", 0);
        grid_row(&mut keys, "фывапролджэ", 1);
        grid_row(&mut keys, "ячсмитьбю", 2);
        Self::new(keys)
    }

    /// The Greek block: three rows (9 / 9 / 7 keys, 25 total).
    #[must_use]
    pub fn greek() -> Self {
        let mut keys = Vec::new();
        grid_row(&mut keys, "ςερτυθιοπ", 0);
        grid_row(&mut keys, "ασδφγηξκλ", 1);
        grid_row(&mut keys, "ζχψωβνμ", 2);
        Self::new(keys)
    }

    /// The French AZERTY Latin variant: three rows (10 / 10 / 6 keys, 26 total).
    #[must_use]
    pub fn azerty() -> Self {
        let mut keys = Vec::new();
        grid_row(&mut keys, "azertyuiop", 0);
        grid_row(&mut keys, "qsdfghjklm", 1);
        grid_row(&mut keys, "wxcvbn", 2);
        Self::new(keys)
    }

    /// The German QWERTZ Latin variant: three rows (10 / 9 / 7 keys, 26 total).
    #[must_use]
    pub fn qwertz() -> Self {
        let mut keys = Vec::new();
        grid_row(&mut keys, "qwertzuiop", 0);
        grid_row(&mut keys, "asdfghjkl", 1);
        grid_row(&mut keys, "yxcvbnm", 2);
        Self::new(keys)
    }

    /// The alpha page for a BCP-47 language `tag`, keyed on its primary subtag
    /// (so `ru-RU` and `ru` resolve alike). Cyrillic-script and Greek locales get
    /// a native block; French/German get their national Latin variant; every
    /// other Latin locale falls back to the default [`Layout::qwerty`].
    #[must_use]
    pub fn alpha_for(tag: &str) -> Self {
        match tag.split(['-', '_']).next().unwrap_or(tag) {
            "ru" | "uk" | "be" | "bg" | "sr" | "mk" => Layout::cyrillic(),
            "el" => Layout::greek(),
            "fr" => Layout::azerty(),
            "de" | "lb" => Layout::qwertz(),
            _ => Layout::qwerty(),
        }
    }
}

/// Append one left-aligned row of character keys at `row` (0-based); the `i`-th
/// character sits at `x = i * W`.
fn grid_row(keys: &mut Vec<Key>, chars: &str, row: usize) {
    for (i, c) in chars.chars().enumerate() {
        keys.push(Key::new(KeyId(c), i as f32 * W, row as f32 * H, W, H));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The characters of a layout, in row-major order.
    fn chars(l: &Layout) -> Vec<char> {
        l.keys().iter().map(|k| k.id.ch()).collect()
    }

    #[test]
    fn cyrillic_has_the_yctuken_rows() {
        let l = Layout::cyrillic();
        assert_eq!(chars(&l).len(), 32);
        assert_eq!(
            chars(&l).into_iter().collect::<String>(),
            "йцукенгшщзхъфывапролджэячсмитьбю"
        );
    }

    #[test]
    fn greek_has_twenty_five_letters() {
        let l = Layout::greek();
        assert_eq!(chars(&l).len(), 25);
        assert_eq!(
            chars(&l).into_iter().collect::<String>(),
            "ςερτυθιοπασδφγηξκλζχψωβνμ"
        );
    }

    #[test]
    fn azerty_and_qwertz_are_full_latin_variants() {
        // Both carry all 26 Latin letters, only rearranged.
        for l in [Layout::azerty(), Layout::qwertz()] {
            let mut cs = chars(&l);
            cs.sort_unstable();
            assert_eq!(cs, ('a'..='z').collect::<Vec<_>>());
        }
        assert_eq!(chars(&Layout::azerty())[0], 'a');
        assert_eq!(chars(&Layout::qwertz())[5], 'z');
    }

    #[test]
    fn rows_are_left_aligned_grids() {
        // grid_row starts every row at x=0 and steps by W; rows descend by H.
        let l = Layout::greek();
        let find = |c: char| *l.keys().iter().find(|k| k.id.ch() == c).unwrap();
        assert_eq!((find('ς').x, find('ς').y), (0.0, 0.0));
        assert_eq!((find('α').x, find('α').y), (0.0, H));
        assert_eq!((find('ζ').x, find('ζ').y), (0.0, 2.0 * H));
        assert_eq!(find('ρ').x, 2.0 * W); // third key of the top row
    }

    #[test]
    fn alpha_for_selects_by_primary_subtag() {
        // Cyrillic-script locales.
        for tag in ["ru", "uk", "be", "bg", "sr", "mk", "ru-RU", "sr_Cyrl"] {
            assert_eq!(chars(&Layout::alpha_for(tag)).len(), 32, "{tag}");
        }
        assert_eq!(chars(&Layout::alpha_for("el-GR")).len(), 25);
        // National Latin variants keep 26 letters but reorder the first key:
        // 'a' leads AZERTY, 'q' leads QWERTZ.
        assert_eq!(chars(&Layout::alpha_for("fr"))[0], 'a');
        assert_eq!(chars(&Layout::alpha_for("de_DE"))[0], 'q');
        // Luxembourgish shares the German QWERTZ block (Swiss national standard).
        assert_eq!(chars(&Layout::alpha_for("lb"))[0], 'q');
        assert_eq!(chars(&Layout::alpha_for("lb"))[5], 'z');
        assert_eq!(chars(&Layout::alpha_for("lb-LU"))[5], 'z');
        // Every other Latin locale, and a bare/empty tag, fall back to qwerty.
        assert_eq!(chars(&Layout::alpha_for("en"))[0], 'q');
        assert_eq!(chars(&Layout::alpha_for("pt-BR"))[0], 'q');
        assert_eq!(chars(&Layout::alpha_for("")).len(), 26);
    }
}
