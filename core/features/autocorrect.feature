# BDD specification — no-clobber autocorrect.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/autocorrect/tests/no_clobber.rs; as the cucumber harness is wired up
# (SEDD §12), these steps bind directly to it.

Feature: No-clobber autocorrect
  As a person typing on FeatherKey
  I want real words left exactly as I typed them
  So that autocorrect never overwrites a word I clearly intended

  Background:
    Given English and Portuguese are both active
    And the English lexicon contains "cat", "cot", and "hat"
    And the Portuguese lexicon contains "mundo"

  @BR-12 @mvp
  Scenario: A real word is never clobbered
    When I finish typing the token "cat"
    Then the committed word is "cat"
    And no correction is applied

  @BR-18 @mvp
  Scenario: A word valid in another active language is not corrected
    When I finish typing the token "mundo"
    Then the committed word is "mundo"
    And no correction is applied

  @BR-12 @mvp
  Scenario: A misspelling is offered edit-distance-1 candidates
    When I finish typing the token "cxt"
    Then a correction is applied
    And the primary candidate is "cat"
    And "cot" is offered as an alternative

  @BR-12 @mvp
  Scenario: A non-word with no near neighbours is left untouched
    When I finish typing the token "qqqq"
    Then the committed word is "qqqq"
    And no correction is applied
