# BDD specification — the suggestion strip learns which word I mean.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in the featherkey-core integration
# tests (core/crates/featherkey-core/tests/{w6b_ranking_reflects_learning,
# neural_learning,neural_persistence}.rs), backed by the re-ranker's own inline
# #[cfg(test)] unit tests in core/crates/neural-ranker/src/{lib,persist}.rs; as
# the cucumber harness is wired up (SEDD §12), these steps bind directly to it. The
# tiny neural re-ranker starts identical to the legacy linear ranking (cold-start
# prior) and learns online from strip-picks; its weights are encrypted in the
# RankerModel namespace and purged by the "clear learned data" wipe (ADR-3).

Feature: The suggestion strip learns which word I mean
  As a person typing on FeatherKey
  I want the strip to promote the completion I keep choosing
  So that the words I actually use rise to the top — and are forgotten when I clear my data

  @BR-11 @mvp
  Scenario: Repeatedly choosing a lower-ranked completion promotes it, and clearing data forgets it
    Given the strip offers "test", "team" and "tea" for the prefix "te"
    When I choose "tea" from the strip several times in an ordinary field
    Then "tea" is ranked ahead of "test" for "te"
    When I clear my learned data
    Then "test" is ranked ahead of "tea" for "te" again
