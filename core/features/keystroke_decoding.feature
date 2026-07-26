# BDD specification — keystroke decoding accuracy.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/input-decoder/tests/tracer_bullet.rs; as the cucumber harness is wired
# up (SEDD §12), these steps bind directly to it.

Feature: Keystroke decoding accuracy
  As a person typing on FeatherKey
  I want my touches resolved to the key I intended
  So that I do not have to correct missed keystrokes

  Background:
    Given the single-row tracer layout "q w e r t"

  @BR-5 @mvp
  Scenario: A dead-center tap resolves to that key
    When I tap the center of the "r" key
    Then the committed character is "r"
    And the decoder's confidence in "r" is 1.0

  @BR-5 @mvp
  Scenario: A sloppy tap resolves to the nearest key
    When I tap 20 pixels toward "r" from the center of "t"
    Then the committed character is "t"

  @BR-6 @mvp
  Scenario: Candidates are ranked by proximity
    When I tap between the "w" and "e" keys, nearer "e"
    Then the top candidate is "e"
    And "w" is ranked above the remaining keys
    And each candidate's confidence is lower than the one before it
