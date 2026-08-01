# BDD specification — proper-noun capitalization (BR-69).
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/propercase/tests/propercase_spec.rs.

Feature: Proper-noun capitalization
  As a person typing on FeatherKey
  I want names, countries and capitals capitalized for me mid-sentence
  So that my prose reads correctly without extra shift taps

  @BR-69 @mvp
  Scenario: A known proper noun typed lowercase is capitalized mid-sentence
    Given the proper-noun lexicon contains "Paris"
    And "paris" is not a common lowercase word
    When the word "paris" is committed mid-sentence
    Then it is recased to "Paris"

  @BR-69 @mvp
  Scenario: A word that is also a common lowercase word is left alone
    Given the proper-noun lexicon contains "Rose"
    And "rose" is a common lowercase word
    When the word "rose" is committed mid-sentence
    Then it is left as "rose"

  @BR-69 @mvp
  Scenario: The canonical form restores accents as well as case
    Given the proper-noun lexicon contains "João"
    And "joao" is not a common lowercase word
    When the word "joao" is committed mid-sentence
    Then it is recased to "João"

  @BR-69 @mvp
  Scenario: A word at a sentence start is left to auto-capitalization
    Given the proper-noun lexicon contains "Paris"
    When the word "paris" is committed at a sentence start
    Then it is left unchanged

  @BR-69 @mvp
  Scenario: Deliberate all-caps is never rewritten
    Given the proper-noun lexicon contains "Paris"
    When the word "PARIS" is committed mid-sentence
    Then it is left unchanged

  @BR-69 @mvp
  Scenario: An unknown word is left unchanged
    Given the proper-noun lexicon contains "Paris"
    When the word "florp" is committed mid-sentence
    Then it is left unchanged

  @BR-69 @mvp
  Scenario: A habitually capitalized mid-sentence name is learned
    Given the field permits learning
    When "Zoe" is committed mid-sentence
    And "zoe" is later committed mid-sentence
    Then "zoe" is recased to "Zoe"

  @BR-69 @mvp
  Scenario: Names in a sensitive field are never learned
    Given the field is a password field
    When "Zoe" is committed mid-sentence in that field
    Then "zoe" is not learned as a proper noun
