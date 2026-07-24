# BDD specification — concurrent languages & per-word language detection.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/locale-manager/src/lib.rs (the `tests` module) and
# crates/locale-manager/tests/detection.rs; as the cucumber harness is wired up
# (SEDD §12), these steps bind directly to it. Language identification is the
# statistical, hysteresis-based scheme of ADR-10, reading each active language's
# lexicon via `dictionary` (ADR-13).

Feature: Concurrent multilingual typing with automatic per-word detection
  As a person who types in more than one language
  I want the keyboard to keep several languages active and pick the right one per word
  So that I never have to tag words or hunt for a language switch

  Background:
    Given English and Portuguese are both active, in that order

  @BR-16 @mvp @multilingual
  Scenario: Two languages are concurrently active without a manual switch
    Then the active languages are "en" then "pt"

  @BR-19b @mvp @multilingual
  Scenario: A word that belongs to exactly one active language detects that language
    When I type a word only English recognises
    Then the detected language is "en"

  @BR-18 @mvp @multilingual
  Scenario: A word shared by both languages resolves deterministically by hysteresis
    When I type a word both languages recognise
    Then the detected language is the most-recent active language "en"

  @BR-19b @mvp @multilingual
  Scenario: A word no active language recognises detects nothing
    When I type gibberish in no active language
    Then no language is detected

  @BR-17 @mvp @multilingual
  Scenario: A manual switch takes effect instantly and flips the hysteresis winner
    When I switch the active order to Portuguese then English
    And I type a word both languages recognise
    Then the detected language is the most-recent active language "pt"
