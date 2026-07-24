# BDD specification — autocomplete & next-word prediction relevance.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in crates/prediction/src/lib.rs
# (the `tests` module); as the cucumber harness is wired up (SEDD §12), these
# steps bind directly to it. Prediction is the statistical, prefix-completion
# scheme of ADR-3 (neural behind the same Predictor port in v1.x), ranking
# completions read from each active language's lexicon via `dictionary` (ADR-13).

Feature: Relevant autocomplete completions for the in-progress word
  As a person typing on FeatherKey
  I want the word I am typing completed with genuinely relevant suggestions
  So that I finish words in fewer keystrokes without picking a language first

  Background:
    Given English and Spanish lexicons are both active

  @BR-10 @mvp
  Scenario: A prefix is completed with lexicon words, best first
    When I have typed the prefix "app"
    Then the top suggestion is "app"
    And the longer completions "apple" and "apply" follow it
    And each suggestion's score is no higher than the one before it

  @BR-10 @mvp @multilingual
  Scenario: Completions are drawn from every active language and de-duplicated
    When I have typed the prefix "hel"
    Then a word shared by both languages appears exactly once
    And words unique to either language are offered

  @BR-10 @mvp
  Scenario: A word boundary offers no completions yet
    When I have typed no prefix
    Then no suggestions are offered

  @BR-10 @mvp
  Scenario: An unrecognised prefix offers no completions
    When I have typed the prefix "xyz"
    Then no suggestions are offered
