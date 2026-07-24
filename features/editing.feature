# BDD specification — grapheme-aware cursor movement and word selection.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/editing/tests/cursor_editing.rs; as the cucumber harness is wired up
# (SEDD §12), these steps bind directly to it.

Feature: Cursor movement and text selection
  As a person editing text on FeatherKey
  I want the caret to move by whole characters and words
  So that a single key press never lands inside an emoji or an accented letter

  @BR-49 @mvp
  Scenario: The right arrow steps over a whole emoji cluster
    Given the text "hi 👋 café" with the caret before the wave emoji
    When I press the right arrow once
    Then the caret is positioned just after the whole "👋" cluster

  @BR-49 @mvp
  Scenario: The left arrow is the inverse of the right arrow across a cluster
    Given the text "hi 👋 café" with the caret just after the wave emoji
    When I press the left arrow once
    Then the caret returns to the position before the wave emoji

  @BR-49 @mvp
  Scenario: A word jump moves to the end of the next word
    Given the text "the quick brown" with the caret at the start
    When I jump one word to the right
    Then the caret is positioned at the end of "the"

  @BR-49 @mvp
  Scenario: Double-tapping selects the whole word under the caret
    Given the text "edit café now" with the caret inside "café"
    When I select the word at the caret
    Then the selection covers exactly "café"

  @BR-49 @mvp
  Scenario: A caret index that splits a character is rejected, not a panic
    Given the text "café" and a byte index that falls inside the "é"
    When I ask to move the caret right
    Then the operation returns a "not on a char boundary" error
