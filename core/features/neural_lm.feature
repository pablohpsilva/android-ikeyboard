@BR-11
Feature: On-device neural next-word language model (foundation)
  A tiny per-user embedding LM learns which word follows a short context,
  generalising across similar contexts, cold-starting harmlessly, and
  surviving persistence. (Sub-project 1: the model in isolation, not yet
  wired to the live suggestion strip.)

  @BR-11
  Scenario: Learns a two-word context the bigram cannot
    Given the model has repeatedly seen "going to work" and "walking to school"
    When I have typed "going to"
    Then it ranks "work" above "school"
    And after "walking to" it ranks "school" above "work"

  @BR-11
  Scenario: Generalises across similar contexts via embeddings
    Given the model has learned "the cat", "an cat" and "the dog"
    When I type "an"
    Then "dog" is surfaced as a candidate after "an"

  @BR-10
  Scenario: A cold model asserts nothing
    Given a fresh model that has learned nothing
    When I ask for the next words after any context
    Then its confidence is zero
    And its ranking is the deterministic uniform tie-order

  @BR-11
  Scenario: Learning survives persistence
    Given a trained model persisted and reloaded through a secure store
    Then its rankings and confidence are unchanged
    And an absent or corrupt stored blob reloads as a cold-start model
