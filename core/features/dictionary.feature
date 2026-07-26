# BDD specification — per-language lexicon lookup.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/dictionary/tests/lookup.rs; as the cucumber harness is wired up
# (SEDD §12), these steps bind directly to it.
#
# The dictionary is the lexical substrate the predictive stack reads: prefix
# completions feed autocomplete (BR-10) and exact/fuzzy membership is the
# ground truth a no-clobber autocorrect policy stands on (BR-12). This crate
# provides the lookups only — ranking and correction policy live downstream.

Feature: Per-language lexicon lookup
  As the predictive text engine
  I want fast prefix and fuzzy lookups against a compact word set
  So that I can offer relevant completions and never lose the user's word

  Background:
    Given a dictionary of the words "apple", "apply", "apt", "cat", "cot"

  @BR-10 @mvp
  Scenario: Prefix lookup returns the completions for what is typed
    When I ask for completions of "app"
    Then the completions are "apple" and "apply" in order
    And "apt" is not among the completions

  @BR-10 @mvp
  Scenario: Completions are capped so the hot path stays bounded
    Given a dictionary of 21 words sharing the prefix "x"
    When I ask for completions of "x"
    Then at most 16 completions are returned

  @BR-12 @mvp
  Scenario: An exact word is reported present, so it is never treated as a typo
    When I look up the word "apt"
    Then the dictionary contains it

  @BR-12 @mvp
  Scenario: A one-edit typo surfaces the intended word as a fuzzy match
    When I ask for fuzzy matches of "cet"
    Then "cat" and "cot" are offered
    And the exact query is not offered as its own match
