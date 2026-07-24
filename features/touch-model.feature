# BDD specification — adaptive tap-geometry learning.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/touch-model/tests/learning_improves_targeting.rs. `touch-model` is the
# sole writer of the tap-geometry data domain (ADR-14); the decoder only reads
# the offsets it learns (ADR-15).

Feature: Adaptive tap-geometry learning
  As a person typing on FeatherKey
  I want the keyboard to learn where I actually tap each key
  So that my targeting improves the more I type

  Background:
    Given a fresh, unbiased touch model

  @BR-7 @mvp
  Scenario: The model learns a consistent tap bias
    Given the model reports no offset for the "e" key
    When I tap 4 pixels right and 6 pixels low of "e" 50 times
    Then the learned offset for "e" approaches 4 pixels right and 6 pixels low

  @BR-46 @mvp
  Scenario: A non-finite sample never corrupts the learned model
    Given one valid tap offset has been learned for the "t" key
    When a non-finite tap offset is observed for "t"
    Then the observation is refused as an error
    And the previously learned offset for "t" is unchanged
