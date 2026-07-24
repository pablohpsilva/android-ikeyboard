# BDD specification — model-biased keystroke decoding (the accuracy engine).
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/input-decoder/src/lib.rs (unit tests) and
# crates/input-decoder/tests/tracer_bullet.rs (end-to-end). These cover the
# ADR-15 / D-4 break: the decoder now reads the per-user touch-model to bias
# key geometry before resolving a touch.

Feature: Model-biased keystroke decoding
  As a person whose taps drift consistently off-centre
  I want the keyboard to learn where I actually tap each key
  So that my off-centre taps still resolve to the key I intended

  Background:
    Given the single-row tracer layout "q w e r t"

  @BR-7 @BR-6 @mvp
  Scenario: A learned per-user offset reclaims a tap that would otherwise miss
    Given I have an untrained (unbiased) touch model
    When I tap 60 pixels right of the "e" key centre
    Then the committed character is "r"
    Given the model has learned that I tap "e" 60 pixels right of centre
    When I tap 60 pixels right of the "e" key centre
    Then the committed character is "e"
    And the decoder's confidence in "e" is 1.0

  @BR-6 @BR-46 @mvp
  Scenario: An unbiased model decodes exactly like plain nearest-key
    Given I have an untrained (unbiased) touch model
    When I tap between the "w" and "e" keys, nearer "e"
    Then the top candidate is "e"
    And "w" is ranked above the remaining keys
    And each candidate's confidence is lower than the one before it

  @BR-7 @mvp
  Scenario: A learned offset for one key does not perturb another
    Given the model has learned that I tap "w" 30 pixels left and 40 pixels low
    When I tap the centre of the "e" key
    Then the committed character is "e"
    And the decoder's confidence in "e" is 1.0
