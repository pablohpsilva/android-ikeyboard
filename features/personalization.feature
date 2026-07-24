# BDD specification — on-device personal vocabulary learning.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/personalization/tests/roundtrip.rs; as the cucumber harness is wired
# up (SEDD §12), these steps bind directly to it.

Feature: On-device personal vocabulary learning
  As a person typing on FeatherKey
  I want the keyboard to learn the words and names I use
  So that my own vocabulary stops being flagged as wrong — without my data ever leaving the device

  Background:
    Given a fresh personalization model

  @BR-7 @mvp
  Scenario: A repeated word is learned and its frequency grows
    When I observe the word "featherkey" 3 times
    Then the frequency of "featherkey" is 3
    And the word "featherkey" is known

  @BR-7 @mvp
  Scenario: A whitelisted name is known without ever being typed
    When I whitelist the word "acme"
    Then the word "acme" is known
    And the frequency of "acme" is 0

  @BR-13 @mvp
  Scenario: Learned vocabulary stays on the device across restarts
    Given I have observed the word "hyperloop" 2 times
    When the model is persisted and reloaded through the on-device secure store
    Then the frequency of "hyperloop" is 2
    And no network or off-device channel was used
