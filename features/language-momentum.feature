# BDD specification — language momentum and multilingual correction.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in the Rust unit tests that cite
# the same BRs:
#   - crates/language-momentum/src/lib.rs      (the recency-weighted model)
#   - crates/candidate-ranker/src/lib.rs       (momentum-weighted ranking)
#   - crates/featherkey-core/src/correct.rs    (choose_correction)
#   - crates/featherkey-core/src/lib.rs        (rank_candidates, observe_language)
# This file keeps the behaviour traceable to the requirements; the Rust suite is
# the executable proof.

Feature: Language momentum across concurrent languages
  As a bilingual person typing on FeatherKey
  I want the keyboard to follow the language I am actually writing in
  So that suggestions and autocorrect fit my current language without my tagging each word

  Background:
    Given English and Spanish are both active
    And English is the primary language

  @BR-19b @mvp
  Scenario: Sustained typing in one language biases suggestions to that language
    Given I have typed several Spanish words in a row
    When I ask for completions that both languages could offer
    Then the Spanish completion is ranked first

  @BR-12 @BR-18 @mvp
  Scenario: A deliberate word from another active language is not autocorrected
    Given I have been typing English
    When I finish typing a word that is valid in Spanish
    Then the committed word is left exactly as typed
    And no correction is applied

  @BR-18 @mvp
  Scenario: A typo is corrected in the language I am currently writing
    Given I have typed several Spanish words in a row
    When I finish typing a misspelling whose only real fixes are Spanish words
    Then the applied correction is the Spanish word

  @BR-12 @BR-19b @mvp
  Scenario: A closest-spelling fix is not flipped by mild momentum
    Given the momentum for Spanish only slightly exceeds English
    When I finish typing a misspelling one edit from both an English and a Spanish word
    Then the primary-language fix is kept
    But sustained Spanish typing eventually flips it to the Spanish fix

  @BR-12 @BR-18 @mvp
  Scenario: A word only the device dictionary knows is never autocorrected
    Given the device dictionary recognises a word that no bundled lexicon contains
    When I finish typing that word outside a sensitive field
    Then the committed word is left exactly as typed
    And no correction is applied
