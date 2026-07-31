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
}

/// The Latin key arrangements a user can pick, independent of language
/// (design D1/D3). Extend here — Dvorak, Colemak — one variant each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatinLayout {
    Qwerty,
    Qwertz,
    Azerty,
}

impl LatinLayout {
    /// Build the concrete [`Layout`] for this arrangement.
    #[must_use]
    pub fn build(self) -> Layout {
        match self {
            LatinLayout::Qwerty => Layout::qwerty(),
            LatinLayout::Qwertz => Layout::qwertz(),
            LatinLayout::Azerty => Layout::azerty(),
        }
    }
}

/// The three scripts the alpha page can present.
enum Script {
    Cyrillic,
    Greek,
    Latin,
}

/// Classify a BCP-47 `tag` by its primary subtag (so `ru-RU` and `ru` agree).
fn script_of(tag: &str) -> Script {
    match tag.split(['-', '_']).next().unwrap_or(tag) {
        "ru" | "uk" | "be" | "bg" | "sr" | "mk" => Script::Cyrillic,
        "el" => Script::Greek,
        _ => Script::Latin,
    }
}

/// Today's per-locale Latin default (used when no override is set).
fn default_latin_for(tag: &str) -> Layout {
    match tag.split(['-', '_']).next().unwrap_or(tag) {
        "fr" => Layout::azerty(),
        "de" | "lb" => Layout::qwertz(),
        _ => Layout::qwerty(),
    }
}

impl Layout {
    /// The alpha page for a BCP-47 language `tag`. Cyrillic/Greek locales always
    /// get their native block (`latin_override` is ignored — forcing Latin keys
    /// onto them would make the script untypable, design D2). For a Latin locale,
    /// an explicit `latin_override` wins; otherwise the per-locale default applies.
    #[must_use]
    pub fn alpha_for(tag: &str, latin_override: Option<LatinLayout>) -> Self {
        match script_of(tag) {
            Script::Cyrillic => Layout::cyrillic(),
            Script::Greek => Layout::greek(),
            Script::Latin => {
                latin_override.map_or_else(|| default_latin_for(tag), LatinLayout::build)
            }
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
            assert_eq!(chars(&Layout::alpha_for(tag, None)).len(), 32, "{tag}");
        }
        assert_eq!(chars(&Layout::alpha_for("el-GR", None)).len(), 25);
        // National Latin variants keep 26 letters but reorder the first key:
        // 'a' leads AZERTY, 'q' leads QWERTZ.
        assert_eq!(chars(&Layout::alpha_for("fr", None))[0], 'a');
        assert_eq!(chars(&Layout::alpha_for("de_DE", None))[0], 'q');
        // Luxembourgish shares the German QWERTZ block (Swiss national standard).
        assert_eq!(chars(&Layout::alpha_for("lb", None))[0], 'q');
        assert_eq!(chars(&Layout::alpha_for("lb", None))[5], 'z');
        assert_eq!(chars(&Layout::alpha_for("lb-LU", None))[5], 'z');
        // Every other Latin locale, and a bare/empty tag, fall back to qwerty.
        assert_eq!(chars(&Layout::alpha_for("en", None))[0], 'q');
        assert_eq!(chars(&Layout::alpha_for("pt-BR", None))[0], 'q');
        assert_eq!(chars(&Layout::alpha_for("", None)).len(), 26);
    }

    #[test]
    fn latin_override_replaces_the_language_default() {
        // English defaults to QWERTY, but an AZERTY override wins.
        assert_eq!(
            chars(&Layout::alpha_for("en", Some(LatinLayout::Azerty)))[0],
            'a'
        );
        // German defaults to QWERTZ; a QWERTY override wins (row "qwertyuiop", so [5]='y',
        // distinguishing QWERTY from QWERTZ where [5]='z').
        assert_eq!(
            chars(&Layout::alpha_for("de", Some(LatinLayout::Qwerty)))[0],
            'q'
        );
        assert_eq!(
            chars(&Layout::alpha_for("de", Some(LatinLayout::Qwerty)))[5],
            'y'
        );
    }

    #[test]
    fn no_override_reproduces_the_language_default() {
        assert_eq!(chars(&Layout::alpha_for("en", None))[0], 'q'); // qwerty
        assert_eq!(chars(&Layout::alpha_for("fr", None))[0], 'a'); // azerty
        assert_eq!(chars(&Layout::alpha_for("de", None))[5], 'z'); // qwertz
    }

    #[test]
    fn non_latin_script_ignores_the_override() {
        // Forcing Latin onto Cyrillic/Greek would strand the user (design D2).
        assert_eq!(
            chars(&Layout::alpha_for("ru", Some(LatinLayout::Qwerty))).len(),
            32
        );
        assert_eq!(
            chars(&Layout::alpha_for("el", Some(LatinLayout::Azerty))).len(),
            25
        );
    }

    #[test]
    fn latin_layout_build_maps_each_variant() {
        assert_eq!(chars(&LatinLayout::Qwerty.build())[0], 'q');
        assert_eq!(chars(&LatinLayout::Qwertz.build())[5], 'z');
        assert_eq!(chars(&LatinLayout::Azerty.build())[0], 'a');
    }
}
