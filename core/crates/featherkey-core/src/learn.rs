//! `LearnFromInput` + `ManageUserDictionary` use-cases (ARCH §9.1) and
//! persistence, wired to the `SecureStore` port.
//!
//! # E-2 — the sensitive-context gate (BR-26)
//! `learn_word` and `observe_tap` are the *only* paths that fold observed input
//! into learned state, and each consults [`SensitivityPolicy`] **first**. A
//! sensitive field short-circuits before any `observe`, so a password/OTP
//! keystroke can never reach [`Personalization`] or [`TouchModel`]. This is the
//! ordering the `tests/e2_sensitive_ordering.rs` property test pins down.
//!
//! Explicit dictionary edits (`add_to_dictionary`) are deliberate user actions,
//! not passive learning, so they are intentionally *not* gated — the user asked
//! for that word to be remembered.

use featherkey_context::Context;
use featherkey_contracts::{SecureStore, SensitiveContextSource};
use featherkey_corrections::Corrections;
use featherkey_kernel::KeyId;
use featherkey_neural_ranker::{NeuralRanker, RankFeatures};
use featherkey_personalization::Personalization;
use featherkey_touch_model::TouchModel;

use crate::error::FeatherKeyError;
use crate::rank::PRIOR_COEFFS;
use crate::FeatherKeyCore;

/// Learning rate for the online pairwise re-ranker update (Task 12). Fixed for
/// determinism: the same pick sequence always produces the same weight change,
/// and it is small enough that a single stray pick nudges rather than reshapes
/// the ranking. Kept beside the trainer that consumes it.
const RANKER_LR: f32 = 0.05;

impl FeatherKeyCore {
    /// Fold a committed `word` (typed after `preceding`) into learned state —
    /// **unless** `field` is sensitive, in which case both the word and the
    /// `preceding -> word` transition are dropped unlearned (E-2, BR-26).
    ///
    /// The single gate covers *both* learners so a password/OTP keystroke can
    /// never reach [`Personalization`] **or** the next-word [`Context`] model.
    /// `preceding` may be empty (sentence start); the context model itself skips
    /// transitions with a too-short previous token.
    pub fn learn_word(&mut self, preceding: &str, word: &str, field: &dyn SensitiveContextSource) {
        if self.sensitivity.should_suppress(field) {
            return;
        }
        self.personalization.observe(word);
        self.context.record(preceding, word);
        // If this commit corresponds to the still-cached shown set (i.e. the
        // committed word was one of the suggestions), train the ranker toward it
        // against that snapshot's prefix. A strip pick that already fired trains
        // here would find no snapshot (it was consumed) and no-op — so a pick +
        // commit trains exactly once.
        if let Some(snap_prefix) = self.last_ranked.as_ref().map(|s| s.prefix.clone()) {
            self.reinforce_from_pick(&snap_prefix, word);
        }
    }

    /// Record that, for the typed `prefix`, the user picked `picked` from the
    /// suggestion strip (a correction signal that can promote a lower-ranked but
    /// repeatedly-chosen completion) — **unless** `field` is sensitive (E-2,
    /// BR-26), in which case the signal is dropped.
    pub fn observe_strip_pick(
        &mut self,
        prefix: &str,
        picked: &str,
        field: &dyn SensitiveContextSource,
    ) {
        if self.sensitivity.should_suppress(field) {
            return;
        }
        self.corrections.note_pick(prefix, picked);
        // Past the sensitivity gate, so training is structurally suppressed in a
        // sensitive field (this line is never reached there). Reinforce the net
        // toward the picked word against the shown-set snapshot it was chosen from.
        self.reinforce_from_pick(prefix, picked);
    }

    /// Reinforce the neural re-ranker toward `chosen` using the cached shown-set
    /// snapshot for `prefix`, then **consume** that snapshot (Task 12).
    ///
    /// A no-op unless a snapshot is cached, its prefix equals `prefix`
    /// (lowercased, matching how `rank_suggestions` keys it), and `chosen` was one
    /// of the shown words. On a match it runs one pairwise LTR update over exactly
    /// the features the user saw — `O(top-k)`, no vocabulary clone — and clears
    /// `last_ranked`, so a pick that also commits trains once, not twice.
    ///
    /// Callers must already be past the sensitivity gate (both call sites are), so
    /// training never runs in a sensitive field.
    fn reinforce_from_pick(&mut self, prefix: &str, chosen: &str) {
        let Some(snap) = self.last_ranked.as_ref() else {
            return;
        };
        if snap.prefix != prefix.to_lowercase() {
            return;
        }
        let Some(chosen_idx) = snap.shown.iter().position(|(w, _)| w == chosen) else {
            return;
        };
        // Clone the tiny (top-k) feature set out of the borrow so the ranker can
        // be mutably borrowed for the update.
        let shown: Vec<RankFeatures> = snap.shown.iter().map(|(_, f)| f.clone()).collect();
        self.neural_ranker.reinforce(&shown, chosen_idx, RANKER_LR);
        self.last_ranked = None;
    }

    /// Record one low-weight `unwanted` signal for `word` (reverted/deleted right
    /// after being offered) — **unless** `field` is sensitive (E-2, BR-26).
    pub fn observe_delete_retype(&mut self, word: &str, field: &dyn SensitiveContextSource) {
        if self.sensitivity.should_suppress(field) {
            return;
        }
        self.corrections.note_unwanted(word);
    }

    /// Bulk-load pre-computed `(prev, next, count)` bigram transitions into the
    /// next-word model (migrating the legacy Kotlin `context.tsv`). Set-semantics,
    /// so re-running a migration is idempotent. Not gated: migration is a
    /// deliberate one-time import of the user's own prior data.
    pub fn import_context<I: IntoIterator<Item = (String, String, u32)>>(
        &mut self,
        transitions: I,
    ) {
        self.context.import(transitions);
    }

    /// Bulk-load pre-computed `(word, count)` learned frequencies into the
    /// personalization model (migrating the legacy Kotlin `usage.tsv`).
    /// Set-semantics, so re-running a migration is idempotent. Not gated: same
    /// rationale as [`import_context`] — a deliberate one-time import of the
    /// user's own prior data.
    pub fn import_frequencies<I: IntoIterator<Item = (String, u32)>>(&mut self, frequencies: I) {
        self.personalization.import(frequencies);
    }

    /// Every learned word paired with its observed frequency, for the shell's
    /// swipe/gesture decoder (which ranks gesture paths by learned usage).
    #[must_use]
    pub fn learned_frequencies(&self) -> Vec<(String, u32)> {
        self.personalization
            .frequencies()
            .iter()
            .map(|(w, c)| (w.clone(), *c))
            .collect()
    }

    /// Every observed key's learned tap-offset bias, as `(key, dx, dy)`, for the
    /// shell to re-centre gesture key positions before swipe decoding.
    #[must_use]
    pub fn tap_offsets(&self) -> Vec<(String, f32, f32)> {
        self.touch_model
            .offsets()
            .into_iter()
            .map(|(ch, dx, dy)| (ch.to_string(), dx, dy))
            .collect()
    }

    /// Fold one tap's offset `(dx, dy)` for `key` into the per-user tap model —
    /// **unless** `field` is sensitive, in which case the tap is dropped
    /// unlearned (E-2, BR-26).
    ///
    /// # Errors
    /// [`FeatherKeyError::TouchModel`] if the offset is non-finite (the model is
    /// left unchanged).
    pub fn observe_tap(
        &mut self,
        key: char,
        dx: f32,
        dy: f32,
        field: &dyn SensitiveContextSource,
    ) -> Result<(), FeatherKeyError> {
        if self.sensitivity.should_suppress(field) {
            return Ok(());
        }
        self.touch_model.observe(KeyId(key), dx, dy)?;
        Ok(())
    }

    /// Add `word` to the user dictionary as always-correct (ARCH §9.1
    /// `ManageUserDictionary`). A deliberate user action, so it is not gated by
    /// field sensitivity.
    pub fn add_to_dictionary(&mut self, word: &str) {
        self.personalization.whitelist(word);
    }

    /// Whether `word` is known — learned by frequency or whitelisted.
    #[must_use]
    pub fn knows_word(&self, word: &str) -> bool {
        self.personalization.is_known(word)
    }

    /// How many times `word` has been observed (0 if never or only whitelisted).
    #[must_use]
    pub fn word_frequency(&self, word: &str) -> u32 {
        self.personalization.frequency(word)
    }

    /// The likeliest next words after `prev`, most-frequent first (inspection
    /// seam over the learned next-word model; mirrors [`Self::word_frequency`]).
    #[must_use]
    pub fn context_next_words(&self, prev: &str, limit: usize) -> Vec<String> {
        self.context.next_words(prev, limit)
    }

    /// How often `picked` was chosen from the strip for the typed `prefix`
    /// (inspection seam over the correction-signal model).
    #[must_use]
    pub fn correction_pref_count(&self, prefix: &str, picked: &str) -> u32 {
        self.corrections.pref_count(prefix, picked)
    }

    /// How often `word` was flagged unwanted (inspection seam).
    #[must_use]
    pub fn correction_unwanted_count(&self, word: &str) -> u32 {
        self.corrections.unwanted_count(word)
    }

    /// Encrypt and persist all learned state — the vocabulary *and* the per-user
    /// tap-geometry model — through the `SecureStore` port (the `secure-store`
    /// adapter at the composition root), so both survive across sessions.
    ///
    /// # Errors
    /// [`FeatherKeyError::Store`] if the backend or crypto layer fails.
    pub fn persist(&self, store: &impl SecureStore) -> Result<(), FeatherKeyError> {
        self.personalization.persist(store)?;
        self.touch_model.persist(store)?;
        self.context.persist(store)?;
        self.corrections.persist(store)?;
        self.neural_ranker.persist(store)?;
        Ok(())
    }

    /// Reload all learned state — vocabulary and tap model — from the
    /// `SecureStore`, replacing the in-memory models. An absent blob restores an
    /// empty/unbiased model (first run).
    ///
    /// # Errors
    /// [`FeatherKeyError::Store`] if the backend or crypto layer fails.
    pub fn restore(&mut self, store: &impl SecureStore) -> Result<(), FeatherKeyError> {
        self.personalization = Personalization::load(store)?;
        self.touch_model = TouchModel::load(store)?;
        self.context = Context::load(store)?;
        self.corrections = Corrections::load(store)?;
        self.neural_ranker = NeuralRanker::load(store, &PRIOR_COEFFS)?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use featherkey_contracts::Candidate;
    use featherkey_neural_ranker::RankFeatures;

    struct Ordinary;
    impl SensitiveContextSource for Ordinary {
        fn is_sensitive(&self) -> bool {
            false
        }
    }

    /// A core whose "te…" completions are `tea` < `team` < `teach`.
    fn core() -> FeatherKeyCore {
        FeatherKeyCore::new(vec![(
            "en".to_owned(),
            vec!["tea".to_owned(), "team".to_owned(), "teach".to_owned()],
        )])
        .expect("valid single-language core")
    }

    fn no_device() -> Vec<Candidate> {
        Vec::new()
    }

    /// A representative feature vector for weight-comparison probes (arbitrary
    /// values that exercise every slot, so any weight change moves the score).
    fn probe() -> RankFeatures {
        RankFeatures {
            positional: -0.7,
            ln_momentum: 0.2,
            is_lexicon: 1.0,
            is_device: 0.0,
            correction_promote: 0.1,
            correction_demote: 0.0,
            spatial: 0.3,
        }
    }

    fn probe_score(core: &FeatherKeyCore) -> f64 {
        core.neural_ranker().score(&probe())
    }

    #[test]
    fn a_pick_that_also_commits_trains_only_once() {
        // Control: exactly one strip pick after one ranked query → one reinforce.
        let mut once = core();
        let _ = once.rank_suggestions("", "te", no_device());
        once.observe_strip_pick("te", "team", &Ordinary);

        // Test: the same pick, then the commit of the same word. The pick
        // consumed the only snapshot, so `learn_word` finds none and trains
        // nothing — the net weights must match the single-reinforce control.
        let mut pick_then_commit = core();
        let _ = pick_then_commit.rank_suggestions("", "te", no_device());
        pick_then_commit.observe_strip_pick("te", "team", &Ordinary);
        pick_then_commit.learn_word("", "team", &Ordinary);

        let untrained = probe_score(&core());
        assert_ne!(
            probe_score(&once),
            untrained,
            "the single pick must actually train the ranker (else the test is vacuous)"
        );
        assert_eq!(
            probe_score(&pick_then_commit),
            probe_score(&once),
            "pick + commit must train exactly once (snapshot consumed by the pick)"
        );

        // And two picks (each refreshing the snapshot) train twice — proving the
        // equality above is the consumed snapshot, not an inert second update.
        let mut twice = core();
        let _ = twice.rank_suggestions("", "te", no_device());
        twice.observe_strip_pick("te", "team", &Ordinary);
        let _ = twice.rank_suggestions("", "te", no_device());
        twice.observe_strip_pick("te", "team", &Ordinary);
        assert_ne!(
            probe_score(&twice),
            probe_score(&once),
            "two full pick rounds must train twice, unlike one pick + commit"
        );
    }

    #[test]
    fn reinforce_from_pick_consumes_the_matching_snapshot() {
        let mut fk = core();
        let _ = fk.rank_suggestions("", "te", no_device());
        assert!(fk.last_ranked().is_some());
        let before = probe_score(&fk);

        fk.reinforce_from_pick("te", "team");

        assert!(
            fk.last_ranked().is_none(),
            "a successful reinforce consumes (clears) the snapshot"
        );
        assert_ne!(
            before,
            probe_score(&fk),
            "the matching pick trained the net"
        );
    }

    #[test]
    fn reinforce_from_pick_ignores_a_prefix_mismatch() {
        let mut fk = core();
        let _ = fk.rank_suggestions("", "te", no_device());
        let before = probe_score(&fk);

        // Snapshot prefix is "te"; a pick reported under a different prefix does
        // not train and leaves the snapshot intact for its real prefix.
        fk.reinforce_from_pick("xy", "team");

        assert!(fk.last_ranked().is_some(), "a mismatch leaves the snapshot");
        assert_eq!(before, probe_score(&fk), "a prefix mismatch trains nothing");
    }

    #[test]
    fn reinforce_from_pick_ignores_a_word_not_in_the_shown_set() {
        let mut fk = core();
        let _ = fk.rank_suggestions("", "te", no_device());
        let before = probe_score(&fk);

        // "zzz" was never shown for this prefix: no chosen index, no training.
        fk.reinforce_from_pick("te", "zzz");

        assert!(
            fk.last_ranked().is_some(),
            "an unshown word leaves the snapshot"
        );
        assert_eq!(before, probe_score(&fk), "an unshown word trains nothing");
    }

    #[test]
    fn reinforce_from_pick_is_a_noop_without_a_snapshot() {
        let mut fk = core();
        // No rank_suggestions call, so there is no cached snapshot at all.
        assert!(fk.last_ranked().is_none());
        let before = probe_score(&fk);

        fk.reinforce_from_pick("te", "team");

        assert_eq!(before, probe_score(&fk), "no snapshot means no training");
    }
}
