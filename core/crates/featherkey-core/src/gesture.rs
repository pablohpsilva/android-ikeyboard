//! Swipe/glide decode use-case — the composition that relocates the Android
//! Kotlin gesture pipeline into the core (design `2026-08-04-ios-gesture-into-core`).
//!
//! The core owns the vocabulary, ranks, learned usage, tap-offset bias, and key
//! geometry; the shell passes only the finger path (in the layout's logical frame,
//! like `decode`). The pure scorer lives in `featherkey-gesture`; this module wires
//! the core's own data into it, so both iOS (now) and — later — Android reuse one
//! engine. The gesture index is prebuilt once per language set (never per gesture).

use std::collections::HashMap;

use featherkey_gesture::{decode, GestureIndex, Point};

use crate::packs::{primary_tag, Pack};
use crate::{FeatherKeyCore, Layout};

/// The number of swipe candidates returned (mirrors the Kotlin decoder's default).
const GESTURE_LIMIT: usize = 4;

/// Build the swipe-gesture index over the active lexicons, plus the merged
/// `word → rank` map the scorer discounts with. A word appearing in several active
/// languages keeps its **best** (lowest) rank. Learned words are *not* indexed here
/// — they are folded in at decode time via the learned-usage discount — so the
/// index is a pure function of the bundled lexicons and is rebuilt only on a
/// language switch.
pub(crate) fn build_gesture_index(packs: &[Pack]) -> (GestureIndex, HashMap<String, u32>) {
    let mut rank: HashMap<String, u32> = HashMap::new();
    for pack in packs {
        for (word, r) in &pack.rank {
            rank.entry(word.clone())
                .and_modify(|best| {
                    if r < best {
                        *best = *r;
                    }
                })
                .or_insert(*r);
        }
    }
    let words: Vec<&str> = rank.keys().map(String::as_str).collect();
    (GestureIndex::build(&words), rank)
}

impl FeatherKeyCore {
    /// Decode a swipe/glide path into ranked words, best first. `points` are in the
    /// layout's logical frame (the same frame `layout_keys` reports and `decode`
    /// resolves taps against). An empty return means "not a gesture" (too few
    /// points, or no vocabulary word matched). Read-only: no learned state changes.
    #[must_use]
    pub fn decode_gesture(&self, points: &[Point]) -> Vec<String> {
        if points.is_empty() {
            return Vec::new();
        }
        let centers = self.gesture_centers();
        let learned: HashMap<String, u32> = self.learned_frequencies().into_iter().collect();
        decode(
            points,
            &centers,
            &self.gesture_index,
            |w| self.gesture_rank.get(w).copied().unwrap_or(u32::MAX),
            &learned,
            GESTURE_LIMIT,
        )
    }

    /// The alpha-page letter centres, re-centred by the user's learned per-key tap
    /// bias — this absorbs the Kotlin `GestureGeometry.shiftCenters` step into the
    /// core. Built from the alpha layout for the primary language (respecting the
    /// Latin arrangement) so a swipe always scores against letters even if a
    /// numeric/symbol page happens to be live.
    fn gesture_centers(&self) -> HashMap<char, Point> {
        let layout = Layout::alpha_for(&primary_tag(&self.packs), self.latin_override);
        let mut offsets: HashMap<char, (f32, f32)> = HashMap::new();
        for (key, dx, dy) in self.tap_offsets() {
            if let Some(ch) = key.chars().next() {
                offsets.insert(ch, (dx, dy));
            }
        }
        let mut centers = HashMap::with_capacity(layout.keys().len());
        for k in layout.keys() {
            let ch = k.id.ch();
            let (dx, dy) = offsets.get(&ch).copied().unwrap_or((0.0, 0.0));
            centers.insert(
                ch,
                Point {
                    x: k.x + k.width / 2.0 + dx,
                    y: k.y + k.height / 2.0 + dy,
                },
            );
        }
        centers
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn en_core() -> FeatherKeyCore {
        FeatherKeyCore::new(vec![(
            "en".to_owned(),
            ["hello", "help", "hero", "world", "the", "cat"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        )])
        .expect("valid core")
    }

    /// The polyline through a word's on-screen key centres, read from the real alpha
    /// layout the core exposes — a perfect swipe of that word.
    fn trace(core: &FeatherKeyCore, word: &str) -> Vec<Point> {
        let keys = core.layout_keys();
        word.chars()
            .filter_map(|ch| {
                keys.iter()
                    .find(|k| k.label == ch.to_string())
                    .map(|k| Point {
                        x: k.x + k.width / 2.0,
                        y: k.y + k.height / 2.0,
                    })
            })
            .collect()
    }

    #[test]
    fn a_swipe_over_the_letters_decodes_to_that_word() {
        let core = en_core();
        let path = trace(&core, "hello");
        assert!(path.len() >= 3, "hello traces enough points");
        let out = core.decode_gesture(&path);
        assert_eq!(out.first().map(String::as_str), Some("hello"));
    }

    #[test]
    fn an_empty_path_is_not_a_gesture() {
        let core = en_core();
        assert!(core.decode_gesture(&[]).is_empty());
    }

    #[test]
    fn the_gesture_index_is_rebuilt_on_a_language_switch() {
        let mut core = en_core();
        assert!(!core.gesture_index.is_empty());
        // Switch to a set that still contains a swipeable word.
        core.set_active_languages(vec![(
            "en".to_owned(),
            vec!["cat".to_owned(), "car".to_owned()],
        )])
        .expect("switch ok");
        let path = trace(&core, "cat");
        assert_eq!(
            core.decode_gesture(&path).first().map(String::as_str),
            Some("cat")
        );
    }
}
