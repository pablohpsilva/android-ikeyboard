//! Language momentum: a recency-weighted per-language weight that tracks which
//! active language the user is currently writing in. Pure and deterministic —
//! no I/O, no clock. One responsibility: hold weights, decay them each word,
//! bump the languages that recognised the word.

use std::collections::HashMap;

/// Multiplicative decay applied to every language on each observed word.
pub const DECAY: f64 = 0.9;
/// Lower bound on any weight, so a dormant language is never fully silenced.
pub const FLOOR: f64 = 0.05;
/// Extra initial weight the primary language starts with (cold-start bias).
pub const HEAD_START: f64 = 1.0;

/// Per-language momentum weights. `weight_of` clamps to [`FLOOR`].
#[derive(Debug, Clone)]
pub struct Momentum {
    weights: HashMap<String, f64>,
}

impl Momentum {
    /// Seed weights for `langs`, giving `primary` a [`HEAD_START`].
    #[must_use]
    pub fn new(primary: &str, langs: &[String]) -> Self {
        let mut weights = HashMap::new();
        for l in langs {
            weights.insert(l.clone(), FLOOR);
        }
        weights.insert(primary.to_owned(), FLOOR + HEAD_START);
        Self { weights }
    }

    /// One observed committed word: decay all, then bump each recogniser by 1.
    pub fn observe(&mut self, recognizers: &[String]) {
        for w in self.weights.values_mut() {
            *w *= DECAY;
        }
        for lang in recognizers {
            if let Some(w) = self.weights.get_mut(lang) {
                *w += 1.0;
            }
        }
    }

    /// Current weight for `lang`, never below [`FLOOR`]. Unknown → [`FLOOR`].
    #[must_use]
    pub fn weight_of(&self, lang: &str) -> f64 {
        self.weights.get(lang).copied().unwrap_or(FLOOR).max(FLOOR)
    }

    /// Re-seed to a new active set: keep still-active weights, drop removed, add
    /// new at [`FLOOR`], re-apply the primary head-start.
    pub fn set_languages(&mut self, primary: &str, langs: &[String]) {
        let mut next: HashMap<String, f64> = HashMap::new();
        for l in langs {
            let kept = self.weights.get(l).copied().unwrap_or(FLOOR);
            next.insert(l.clone(), kept);
        }
        let entry = next.entry(primary.to_owned()).or_insert(FLOOR);
        *entry = entry.max(FLOOR + HEAD_START);
        self.weights = next;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn m() -> Momentum {
        Momentum::new("en", &["en".into(), "es".into()])
    }

    #[test]
    fn primary_starts_ahead_of_the_rest() {
        let m = m();
        assert!(m.weight_of("en") > m.weight_of("es"));
    }

    #[test]
    fn observing_a_language_raises_only_that_language_relatively() {
        let mut m = m();
        let before_es = m.weight_of("es");
        let before_en = m.weight_of("en");
        m.observe(&["es".into()]);
        // es got bumped after decay; en only decayed.
        assert!(m.weight_of("es") > before_es);
        assert!(m.weight_of("en") < before_en);
    }

    #[test]
    fn weights_never_fall_below_the_floor() {
        let mut m = m();
        for _ in 0..500 {
            m.observe(&["es".into()]);
        }
        assert!(m.weight_of("en") >= FLOOR);
    }

    #[test]
    fn an_unrecognized_word_decays_all_and_bumps_none() {
        let mut m = m();
        let en0 = m.weight_of("en");
        m.observe(&[]);
        assert!(m.weight_of("en") < en0);
    }

    #[test]
    fn set_languages_retains_active_drops_removed_adds_new_at_floor() {
        // New primary is "de" (a NEW language) so that "es" is retained as a
        // NON-primary and keeps its exact observed weight — otherwise the
        // head-start max() would raise a retained primary and mask the retain.
        let mut m = m();
        m.observe(&["es".into()]);
        let es = m.weight_of("es");
        m.set_languages("de", &["es".into(), "de".into()]);
        assert_eq!(m.weight_of("es"), es); // retained non-primary
        assert_eq!(m.weight_of("de"), FLOOR + HEAD_START); // new primary at head-start
        assert_eq!(m.weight_of("en"), FLOOR); // dropped -> unknown -> floor
    }

    #[test]
    fn debug_is_implemented() {
        assert!(format!("{:?}", m()).contains("Momentum"));
    }

    use proptest::prelude::*;
    proptest! {
        #[test]
        fn repeatedly_observing_a_language_strictly_raises_it_and_overtakes_the_rest(bumps in 1u32..20) {
            let mut m = Momentum::new("en", &["en".into(), "es".into()]);
            let mut last = m.weight_of("es");
            for _ in 0..bumps {
                m.observe(&["es".into()]);
                let now = m.weight_of("es");
                prop_assert!(now > last); // strictly increasing: bump (+1) always beats decay (×0.9)
                last = now;
            }
            prop_assert!(m.weight_of("es") > m.weight_of("en"));
        }
    }
}
