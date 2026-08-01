//! Recent words: a small ephemeral buffer of the last two committed words,
//! so the LM gets 2-word context without any FFI change.

/// A small buffer holding the last two committed words: older and newer.
/// Used to provide two-word context for the language model without requiring
/// an FFI interface change.
#[derive(Debug, Clone)]
pub struct RecentWords {
    /// The oldest of the two words (entered first). Only read outside tests
    /// once `push`/`two_word_context` are wired into `FeatherKeyCore` (Task
    /// 5/7); until then this field is write-only in production builds.
    #[cfg_attr(not(test), allow(dead_code))]
    older: Option<String>,
    /// The newest of the two words (entered most recently). Same
    /// not-yet-wired note as `older`.
    #[cfg_attr(not(test), allow(dead_code))]
    newer: Option<String>,
}

impl RecentWords {
    /// Create a new empty recent-words buffer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            older: None,
            newer: None,
        }
    }

    /// Advance the 2-word window: the previous "newer" becomes "older", `word`
    /// becomes "newer".
    ///
    /// Not yet called from `FeatherKeyCore` (Task 4 owns the buffer but does
    /// not wire it into the commit path — that is Task 5/7); the
    /// `not(test)` allow keeps this crate's `-D warnings` clippy gate green
    /// in the interim, matching `correct.rs`'s `LastCorrection` fields.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn push(&mut self, word: &str) {
        self.older = self.newer.take();
        self.newer = Some(word.to_string());
    }

    /// Get two-word context for the LM given the preceding word.
    ///
    /// Returns:
    /// - `[]` if `preceding` is empty (a boundary — no context).
    /// - `[older, preceding]` if the buffer's newest word matches `preceding`
    ///   and an older word exists (coherent 2-word context).
    /// - `[preceding]` otherwise (safe k=1 degradation — never a WRONG 2-word context).
    ///
    /// Not yet called from `FeatherKeyCore` (Task 5/7); see `push`'s note.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn two_word_context(&self, preceding: &str) -> Vec<String> {
        // Empty preceding is a boundary — no context
        if preceding.is_empty() {
            return vec![];
        }

        // Check if the newer word matches the preceding context and older exists
        if let Some(ref newer) = self.newer {
            if newer == preceding {
                if let Some(ref older) = self.older {
                    return vec![older.clone(), preceding.to_string()];
                }
            }
        }

        // Safe degradation: just the preceding word (k=1)
        vec![preceding.to_string()]
    }

    /// Clear both word slots.
    ///
    /// Not yet called from `FeatherKeyCore` or exercised by this module's own
    /// tests (Task 5/7 wires the caller); unconditional allow, unlike `push`/
    /// `two_word_context` which the tests below already exercise.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.older = None;
        self.newer = None;
    }
}

impl Default for RecentWords {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_word_context_returns_older_and_preceding_when_coherent() {
        let mut r = RecentWords::new();
        r.push("going");
        r.push("to");
        assert_eq!(
            r.two_word_context("to"),
            vec!["going".to_string(), "to".to_string()]
        );
    }

    #[test]
    fn a_mismatched_preceding_degrades_to_one_word() {
        // Cursor jump: shell's `preceding` disagrees with the buffer's newest word.
        let mut r = RecentWords::new();
        r.push("going");
        r.push("to");
        assert_eq!(
            r.two_word_context("elsewhere"),
            vec!["elsewhere".to_string()]
        );
    }

    #[test]
    fn empty_preceding_is_a_boundary() {
        let mut r = RecentWords::new();
        r.push("hi");
        assert!(r.two_word_context("").is_empty());
    }

    #[test]
    fn push_advances_the_window() {
        let mut r = RecentWords::new();
        r.push("a");
        r.push("b");
        r.push("c");
        assert_eq!(
            r.two_word_context("c"),
            vec!["b".to_string(), "c".to_string()]
        );
    }
}
