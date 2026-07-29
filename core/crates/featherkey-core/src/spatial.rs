//! Spatial (noisy-channel) word decode: the composition half.
//!
//! `featherkey-tap-sequence` owns the search; this module owns the two things
//! only the composition root can supply — the tap buffer's synchronisation with
//! the word the shell says is in progress, and the lexicon the beam probes.
//!
//! Split from `rank.rs` so the strip blend and the spatial machinery stay
//! separately readable, and both files stay inside the no-god-file bound
//! (ARCH §6).

use featherkey_prediction::MAX_SUGGESTIONS;
use featherkey_tap_sequence::{hypotheses, Lexicon};

use crate::FeatherKeyCore;

/// How many spatial hypotheses may ever reach the strip. Two is enough to offer
/// the word the taps suggest plus one runner-up; more would let a search over
/// *possible* words crowd out the word actually typed.
pub(crate) const MAX_SPATIAL: usize = 2;

/// A spatial hypothesis must beat the best explanation of what was *literally*
/// typed by this margin (in log-probability) before it is offered at all — a
/// word that merely re-explains the typed prefix adds nothing to the strip.
const MIN_SPATIAL_MARGIN: f32 = 0.15;

impl FeatherKeyCore {
    /// Synchronise the tap buffer against the prefix the shell reports, then
    /// return the words those taps plausibly spell, best first.
    ///
    /// The core is never told about backspaces, commits or field changes — it is
    /// only ever told the current prefix — so the buffer reconciles itself
    /// against that. Anything it cannot explain (a long-press accent, a swiped
    /// word, a field switch) clears the buffer and degrades to exactly the
    /// prefix-only behaviour that came before.
    ///
    /// Hypotheses that merely re-explain what was literally typed are dropped:
    /// the strip already has those from the predictor. Only a word that beats
    /// the typed reading by [`MIN_SPATIAL_MARGIN`] is worth a slot, and at most
    /// [`MAX_SPATIAL`] ever are.
    pub(crate) fn spatial_hypotheses(&mut self, prefix: &str) -> Vec<(String, f32)> {
        if prefix.is_empty() {
            self.taps.clear();
            return Vec::new();
        }
        let typed: Vec<char> = prefix.chars().collect();
        if self.taps.len() > typed.len() {
            self.taps.truncate(typed.len()); // backspace
        }
        // The buffer must describe *this* prefix: same length, same committed
        // characters. Anything else and the taps belong to a different word.
        if self.taps.len() != typed.len() || self.taps.committed() != prefix.to_lowercase() {
            self.taps.clear();
            return Vec::new();
        }
        let lex = PackLexicon { packs: &self.packs };
        let all = hypotheses(&self.taps, &lex, MAX_SPATIAL + MAX_SUGGESTIONS);
        let literal = all
            .iter()
            .find(|h| h.word.to_lowercase().starts_with(prefix))
            .map_or(f32::NEG_INFINITY, |h| h.score);
        all.into_iter()
            .filter(|h| !h.word.to_lowercase().starts_with(prefix))
            .filter(|h| h.score > literal + MIN_SPATIAL_MARGIN)
            .take(MAX_SPATIAL)
            .map(|h| (h.word, h.score))
            .collect()
    }

    /// How many taps are buffered for the word in progress (inspection seam for
    /// the synchronisation tests).
    #[must_use]
    pub fn buffered_taps(&self) -> usize {
        self.taps.len()
    }

    /// The primary (first active) language tag, used to tag spatial candidates.
    pub(crate) fn primary_lang(&self) -> String {
        self.packs
            .first()
            .map(|p| p.lang.as_str().to_owned())
            .unwrap_or_default()
    }
}

/// The beam's view of the active lexicons.
///
/// `fold_prefix` answers in folded-key (alphabetical) order and truncates at
/// `MAX_COMPLETIONS`, so completions are re-ordered by **bundled rank** before
/// that cap can bite: the beam cannot recover a word it never sees, and dropping
/// the commonest continuation because of its spelling is the very defect this
/// ranking data was fixed to prevent.
struct PackLexicon<'a> {
    packs: &'a [crate::packs::Pack],
}

impl Lexicon for PackLexicon<'_> {
    fn is_live_prefix(&self, prefix: &str) -> bool {
        let folded = featherkey_fold::fold(prefix);
        self.packs
            .iter()
            .any(|p| !p.dict.fold_prefix(&folded).is_empty())
    }

    fn completions(&self, prefix: &str, limit: usize) -> Vec<String> {
        let folded = featherkey_fold::fold(prefix);
        let mut hits: Vec<(u32, String)> = Vec::new();
        for p in self.packs {
            for word in p.dict.fold_prefix(&folded) {
                let rank = p.rank.get(&word).copied().unwrap_or(u32::MAX);
                if !hits.iter().any(|(_, w)| w == &word) {
                    hits.push((rank, word));
                }
            }
        }
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        hits.into_iter().take(limit).map(|(_, w)| w).collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod spatial_tests {
    use crate::FeatherKeyCore;

    /// A core whose layout the tests drive by tapping key centres.
    fn en_core() -> FeatherKeyCore {
        FeatherKeyCore::new(vec![(
            "en".into(),
            vec![
                "the".into(),
                "then".into(),
                "rhythm".into(),
                "rhino".into(),
                "there".into(),
            ],
        )])
        .expect("core")
    }

    /// Tap the centre of `key`, as the shell does for every letter press.
    fn tap(core: &mut FeatherKeyCore, key: char) {
        let (x, y) = core
            .layout_keys()
            .iter()
            .find(|k| k.label == key.to_string())
            .map(|k| (k.x + k.width / 2.0, k.y + k.height / 2.0))
            .expect("key on the alpha page");
        core.decode(x, y).expect("decode");
    }

    #[test]
    fn taps_that_spell_a_near_word_surface_the_intended_word() {
        // "r" and "t" are neighbours on QWERTY, so a tap on their boundary keeps
        // both alive. Typing r-h-e must still offer "the".
        let mut core = en_core();
        let r = core
            .layout_keys()
            .iter()
            .find(|k| k.label == "r")
            .map(|k| (k.x + k.width * 0.95, k.y + k.height / 2.0))
            .expect("r key");
        core.decode(r.0, r.1).expect("decode"); // right edge of r: t is the rival
        tap(&mut core, 'h');
        tap(&mut core, 'e');
        let words: Vec<String> = core
            .rank_suggestions("", "rhe", vec![])
            .into_iter()
            .map(|c| c.word)
            .collect();
        assert!(words.iter().any(|w| w == "the"), "got {words:?}");
    }

    #[test]
    fn an_empty_prefix_clears_the_buffer() {
        let mut core = en_core();
        tap(&mut core, 't');
        tap(&mut core, 'h');
        let _ = core.rank_suggestions("", "", vec![]);
        assert_eq!(core.buffered_taps(), 0);
    }

    #[test]
    fn a_shorter_prefix_pops_the_buffer() {
        let mut core = en_core();
        tap(&mut core, 't');
        tap(&mut core, 'h');
        tap(&mut core, 'e');
        let _ = core.rank_suggestions("", "th", vec![]);
        assert_eq!(core.buffered_taps(), 2);
    }

    #[test]
    fn a_prefix_the_taps_do_not_explain_degrades_to_prefix_only() {
        // The shell can put characters in the pending word without a tap — a
        // long-press accent, or a whole swiped word. The buffer must give up.
        let mut core = en_core();
        tap(&mut core, 't');
        let with_taps: Vec<String> = core
            .rank_suggestions("", "thé", vec![])
            .into_iter()
            .map(|c| c.word)
            .collect();
        assert_eq!(core.buffered_taps(), 0, "buffer cleared");

        let mut plain = en_core();
        let without: Vec<String> = plain
            .rank_suggestions("", "thé", vec![])
            .into_iter()
            .map(|c| c.word)
            .collect();
        assert_eq!(with_taps, without, "identical to prefix-only behaviour");
    }

    #[test]
    fn a_cleanly_typed_prefix_still_leads_with_its_own_completion() {
        let mut core = en_core();
        tap(&mut core, 't');
        tap(&mut core, 'h');
        tap(&mut core, 'e');
        let words: Vec<String> = core
            .rank_suggestions("", "the", vec![])
            .into_iter()
            .map(|c| c.word)
            .collect();
        assert_eq!(words.first().map(String::as_str), Some("the"), "{words:?}");
    }

    #[test]
    fn spatial_hypotheses_are_capped() {
        let mut core = en_core();
        let r = core
            .layout_keys()
            .iter()
            .find(|k| k.label == "r")
            .map(|k| (k.x + k.width * 0.95, k.y + k.height / 2.0))
            .expect("r key");
        core.decode(r.0, r.1).expect("decode");
        tap(&mut core, 'h');
        tap(&mut core, 'e');
        let words: Vec<String> = core
            .rank_suggestions("", "rhe", vec![])
            .into_iter()
            .map(|c| c.word)
            .collect();
        // Words the typed prefix cannot produce are the spatial ones.
        let spatial = words.iter().filter(|w| !w.starts_with("rhe")).count();
        assert!(spatial <= crate::spatial::MAX_SPATIAL, "{words:?}");
    }
}
