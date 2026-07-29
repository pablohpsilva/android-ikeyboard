//! Executable form of features/tap-sequence.feature (BR-5, BR-6).
//!
//! The lexicon is a plain `BTreeSet` fake: this crate must depend on no
//! dictionary implementation, and the fake also lets a test *count* oracle calls
//! to pin the beam's bounded work.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::collections::BTreeSet;

use featherkey_tap_sequence::{hypotheses, Lexicon, TapDistribution, TapSequence, BEAM, BRANCH};

/// A lexicon over a fixed word set, counting the liveness probes it answers.
struct FakeLexicon {
    words: BTreeSet<String>,
    probes: Cell<usize>,
}

impl FakeLexicon {
    fn new(words: &[&str]) -> Self {
        Self {
            words: words.iter().map(|w| (*w).to_owned()).collect(),
            probes: Cell::new(0),
        }
    }
}

impl Lexicon for FakeLexicon {
    fn is_live_prefix(&self, prefix: &str) -> bool {
        self.probes.set(self.probes.get() + 1);
        self.words.iter().any(|w| w.starts_with(prefix))
    }

    fn completions(&self, prefix: &str, limit: usize) -> Vec<String> {
        self.words
            .iter()
            .filter(|w| w.starts_with(prefix))
            .take(limit)
            .cloned()
            .collect()
    }
}

/// A tap: `(key, probability)` pairs, best first.
fn tap(pairs: &[(char, f32)]) -> TapDistribution {
    TapDistribution::from_ranked(pairs.iter().copied())
}

fn seq(taps: &[TapDistribution]) -> TapSequence {
    let mut s = TapSequence::new();
    for t in taps {
        s.push(t.clone());
    }
    s
}

#[test]
fn revises_an_earlier_tap_to_reach_a_real_word() {
    // The first tap landed on "r" but "t" was a close rival; the next two taps
    // are unambiguous. Only a whole-word search can go back and choose "t".
    let lex = FakeLexicon::new(&["the", "rhythm", "rhino", "then"]);
    let taps = seq(&[
        tap(&[('r', 0.55), ('t', 0.42), ('e', 0.03)]),
        tap(&[('h', 0.95), ('n', 0.05)]),
        tap(&[('e', 0.95), ('w', 0.05)]),
    ]);
    let got = hypotheses(&taps, &lex, 5);
    let words: Vec<&str> = got.iter().map(|h| h.word.as_str()).collect();
    assert!(words.contains(&"the"), "got {words:?}");
    let the = words.iter().position(|w| *w == "the").unwrap();
    let rhythm = words.iter().position(|w| *w == "rhythm");
    assert!(
        rhythm.is_none_or(|r| the < r),
        "the must outrank rhythm: {words:?}"
    );
}

#[test]
fn a_clean_word_is_unchanged() {
    let lex = FakeLexicon::new(&["cat", "cot", "bat"]);
    let taps = seq(&[
        tap(&[('c', 0.98), ('x', 0.02)]),
        tap(&[('a', 0.98), ('s', 0.02)]),
        tap(&[('t', 0.98), ('y', 0.02)]),
    ]);
    let got = hypotheses(&taps, &lex, 5);
    assert_eq!(got.first().map(|h| h.word.as_str()), Some("cat"));
}

#[test]
fn never_invents_a_word_the_taps_do_not_explain() {
    // "bat" shares no first-tap key, so no hypothesis may reach it.
    let lex = FakeLexicon::new(&["cat", "bat"]);
    let taps = seq(&[tap(&[('c', 1.0)]), tap(&[('a', 1.0)]), tap(&[('t', 1.0)])]);
    let got = hypotheses(&taps, &lex, 5);
    assert!(got.iter().all(|h| h.word != "bat"), "{got:?}");
}

#[test]
fn prunes_dead_prefixes_within_the_analytic_bound() {
    let lex = FakeLexicon::new(&["the", "then", "there", "rhythm"]);
    let taps = seq(&[
        tap(&[('r', 0.5), ('t', 0.4), ('f', 0.1)]),
        tap(&[('h', 0.8), ('n', 0.1), ('b', 0.1)]),
        tap(&[('e', 0.8), ('w', 0.1), ('q', 0.1)]),
    ]);
    let _ = hypotheses(&taps, &lex, 5);
    let bound = BEAM * BRANCH * taps.len() + BEAM;
    assert!(
        lex.probes.get() <= bound,
        "{} probes exceeds bound {bound}",
        lex.probes.get()
    );
}

#[test]
fn an_empty_sequence_yields_nothing() {
    let lex = FakeLexicon::new(&["the"]);
    assert!(hypotheses(&TapSequence::new(), &lex, 5).is_empty());
}

#[test]
fn an_empty_lexicon_yields_nothing() {
    let lex = FakeLexicon::new(&[]);
    let taps = seq(&[tap(&[('a', 1.0)])]);
    assert!(hypotheses(&taps, &lex, 5).is_empty());
}

#[test]
fn push_pop_clear_len() {
    let mut s = TapSequence::new();
    assert_eq!(s.len(), 0);
    s.push(tap(&[('a', 1.0)]));
    s.push(tap(&[('b', 1.0)]));
    assert_eq!(s.len(), 2);
    s.pop();
    assert_eq!(s.len(), 1);
    s.clear();
    assert_eq!(s.len(), 0);
    s.pop(); // popping empty is a no-op, never a panic
    assert_eq!(s.len(), 0);
}

#[test]
fn capacity_is_bounded_and_never_reallocates() {
    let mut s = TapSequence::new();
    let cap = s.capacity();
    for _ in 0..cap * 2 {
        s.push(tap(&[('a', 1.0)]));
    }
    assert_eq!(s.len(), cap, "length is capped at capacity");
    assert_eq!(s.capacity(), cap, "capacity never grew");
}

#[test]
fn a_distribution_keeps_only_the_branch_most_likely_keys() {
    let d = TapDistribution::from_ranked(
        [('a', 0.4), ('b', 0.3), ('c', 0.2), ('d', 0.1)]
            .iter()
            .copied(),
    );
    assert_eq!(d.len(), BRANCH.min(4));
    assert_eq!(d.best(), Some('a'));
}
