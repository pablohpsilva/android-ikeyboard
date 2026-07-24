# BDD specification — smart typing (auto-capitalization, double-space-period,
# smart punctuation).
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/smart-typing/tests/smart_typing_spec.rs; as the cucumber harness is
# wired up (SEDD §12), these steps bind directly to it.

Feature: Smart typing assistance
  As a person typing on FeatherKey
  I want obvious mechanical edits made for me
  So that my prose is correctly capitalized and punctuated without extra taps

  @BR-48 @mvp
  Scenario: The first letter of a field is capitalized
    Given the text before the caret is ""
    Then the next letter is auto-capitalized

  @BR-48 @mvp
  Scenario: The first letter of a new sentence is capitalized
    Given the text before the caret is "Hello world. "
    Then the next letter is auto-capitalized

  @BR-48 @mvp
  Scenario: A letter mid-word is not capitalized
    Given the text before the caret is "Hello wor"
    Then the next letter is not auto-capitalized

  @BR-48 @mvp
  Scenario: A double space becomes a period and a space
    Given the text before the caret is "hello "
    When I type a space
    Then the trailing space is replaced with ". "

  @BR-48 @mvp
  Scenario: A single space between words is left alone
    Given the text before the caret is "hello"
    When I type a space
    Then nothing is replaced

  @BR-48 @mvp
  Scenario: A quote at the start of a span opens
    Given the text before the caret is ""
    When I type a straight double quote
    Then the committed character is the opening curly quote "“"

  @BR-48 @mvp
  Scenario: A quote after a letter closes
    Given the text before the caret is "bye"
    When I type a straight double quote
    Then the committed character is the closing curly quote "”"
