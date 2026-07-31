# BDD specification — key layouts and geometry.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/layout-engine/src/standard.rs and the crate's lib.rs test module; as the
# cucumber harness is wired up (SEDD §12), these steps bind directly to it.

Feature: Non-alphabetic pages and RTL-ready layouts
  As a person typing on FeatherKey
  I want number and symbol pages, and layouts that know their reading direction
  So that I can type digits and punctuation and be ready for RTL locales

  @BR-47 @mvp
  Scenario: The numeric page presents the digit row
    When I switch to the numeric page
    Then the page presents the keys "1 2 3 4 5 6 7 8 9 0"
    And the page is tagged as a numeric layout

  @BR-47 @mvp
  Scenario: The symbols page presents punctuation and symbols
    When I switch to the symbols page
    Then the page presents symbol keys including "." and "?" and "@"
    And the page is tagged as a symbols layout

  @BR-53 @mvp
  Scenario: A layout can be tagged right-to-left without reordering its keys
    Given the single-row tracer layout "q w e r t"
    When I tag the layout as right-to-left
    Then the layout reports a right-to-left reading direction
    And the layout's keys are left in their original order
    # Bidirectional reordering is deferred until the launch language set is
    # fixed (ADR-16); this scenario only pins the direction marker.

  @BR-68 @mvp
  Scenario: A chosen Latin layout overrides the language default
    Given the active language is English
    When the user chooses the QWERTZ layout
    Then the alpha page presents the QWERTZ arrangement (top row starts "q w e r t z")

  @BR-68 @mvp
  Scenario: The Latin layout choice does not affect a non-Latin script
    Given the active language is Russian
    When the user chooses the AZERTY layout
    Then the alpha page still presents the Cyrillic ЙЦУКЕН block

  @BR-68
  Scenario: Auto reproduces the per-language default
    Given the active language is French
    When the user leaves the layout on Auto
    Then the alpha page presents AZERTY (French's national default)
