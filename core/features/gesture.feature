# BDD specification — swipe/glide decoding lives in the shared Rust core.
# The SHARK²-style decoder was moved out of the Android Kotlin shell into
# `featherkey-gesture` and exposed over the FFI as `decode_gesture`, so a swipe
# is decoded by the same engine on every platform. The observable behaviour is
# path-in / ranked-words-out through the core (BR-41), consumed by iOS this wave.

Feature: FeatherKey decodes swipe gestures in the shared core
  As a person glide-typing on the FeatherKey keyboard
  I want a swipe across a word's letters to insert that word
  So that swipe typing uses the exact engine on every platform, not a Swift copy

  @BR-41 @BR-70
  Scenario: A swipe over a word's letters decodes to that word
    Given the FeatherKey iOS keyboard is shown in a text field
    When the user glides the finger across the letters of "hello"
    Then the shared core decodes the path to the word "hello"
    And the word is committed with a trailing space
    And the decision came from the shared core, not from Swift

  @BR-41
  Scenario: A quick tap is never treated as a swipe
    Given the FeatherKey iOS keyboard is shown in a text field
    When the user taps a single key without gliding
    Then the gesture is classified as a tap, not a swipe
    And the per-key tap decode inserts the character as usual

  @BR-41
  Scenario: The other glide candidates are offered as alternatives
    Given the user has glided a word onto the field
    When the core returns more than one candidate for the path
    Then the remaining candidates appear in the suggestion strip
    And picking one replaces the committed word in place
