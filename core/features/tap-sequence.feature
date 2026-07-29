# BDD specification — word-level noisy-channel tap decode.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form lives in crates/tap-sequence/tests/beam.rs (the search) and
# crates/featherkey-core/src/rank.rs (the blend into the suggestion strip).
#
# The behaviour this file pins is the one no per-tap decision can deliver: a tap
# already committed as the wrong letter is reconsidered once later taps arrive.

Feature: Reading a whole word from ambiguous taps
  As a person typing quickly with imprecise fingers
  I want the word I aimed for offered even when a tap landed on the wrong key
  So that a single slip early in a word does not cost me the word

  Background:
    Given the lexicon contains "the", "then", "rhythm", and "rhino"

  @BR-5 @BR-6 @mvp
  Scenario: A slip on the first tap is repaired by the taps that follow
    Given my first tap landed on "r" with "t" a close rival
    And my next two taps were clearly "h" and "e"
    When the suggestions are ranked
    Then "the" is offered ahead of "rhythm"

  @BR-6 @mvp
  Scenario: A cleanly typed word is left alone
    Given every tap of "the" landed squarely on its key
    When the suggestions are ranked
    Then "the" leads the suggestions

  @BR-5 @mvp
  Scenario: Taps that spell nothing real propose nothing
    Given my taps spell a sequence no word in the lexicon continues
    When the suggestions are ranked
    Then no spatial hypothesis is offered

  @BR-6 @mvp
  Scenario: A word typed by other means is not second-guessed
    Given the word in progress arrived by swipe or a long-press accent
    When the suggestions are ranked
    Then the suggestions are exactly what the typed prefix alone would give
