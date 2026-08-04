# BDD specification — the iOS keyboard extension reuses the shared Rust core.
# Tagged to the Business Requirement it verifies (ARCH §8). The observable
# behaviour is char-in / decoded-char-out through the FFI boundary.

Feature: FeatherKey types on iOS through the shared core
  As a person typing on the FeatherKey iOS keyboard
  I want a key tap to insert the character the shared core decodes
  So that iOS reuses the exact typing engine Android uses

  @BR-70
  Scenario: Typing a letter commits the core-decoded character
    Given the FeatherKey iOS keyboard is shown in a text field
    When the user taps the centre of the "h" key
    Then the character "h" is inserted into the field
    And no typing logic ran outside the shared core

  @BR-10 @BR-70
  Scenario: The suggestion strip offers real completions from the bundled lexicon
    Given the FeatherKey iOS keyboard bundles the shared English lexicon
    When the user has typed the prefix "th"
    Then the shared core offers a completion beginning with "th"
    And the completions come from the same word list the Android app ships

  @BR-47 @BR-70
  Scenario: The number and symbol pages insert their literal characters
    Given the FeatherKey iOS keyboard is showing its number page
    When the user taps the "5" key
    Then the character "5" is inserted into the field
    And switching to the symbol page then tapping "#" inserts "#"
    And the number and symbol characters match the ones the Android keyboard shows

  @BR-12 @BR-69 @BR-70
  Scenario: Committing a word at a space applies the core's correction and proper-case
    Given the FeatherKey iOS keyboard is shown in a text field
    When the user has typed the word "teh" and taps space
    Then the shared core replaces it with its chosen correction
    And a proper-case decision from the core wins over an edit-distance correction
    And an immediate backspace restores the exact word the user typed
    And every one of these decisions came from the shared core, not from Swift
