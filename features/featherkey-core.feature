# BDD specification — the composition façade (featherkey-core).
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/featherkey-core/tests/composition.rs and
# crates/featherkey-core/tests/e2_sensitive_ordering.rs; as the cucumber harness
# is wired up (SEDD §12), these steps bind directly to them.
#
# featherkey-core closes no new product BR of its own — it composes the crates
# that own each BR behind the contracts ports and enforces the E-2 gate ordering
# (BR-26) at the one place the whole system is wired together.

Feature: The composed keyboard core
  As the Android shell
  I want one narrow, safe API over the whole Rust core
  So that I can decode, suggest, correct, and learn without knowing the internals

  Background:
    Given a core configured with the English lexicon "cat, cot, dog"

  @BR-26 @mvp
  Scenario: A sensitive field structurally cannot be learned
    When the user types "hunter2" into a password field
    Then "hunter2" is not known to the core
    And the learned frequency of "hunter2" is zero

  @BR-26 @mvp
  Scenario: The same word in an ordinary field is learned
    When the user types "hunter2" into an ordinary field
    Then "hunter2" becomes known to the core

  @BR-5 @mvp
  Scenario: A touch is decoded to the intended key
    When the shell reports a touch at the centre of the "q" key
    Then the decoded key is "q"

  @BR-10 @mvp
  Scenario: A prefix is completed from the active lexicon
    When the shell asks for completions of "c"
    Then the suggestions include "cat" and "cot"

  @BR-12 @mvp
  Scenario: A word the user intended is never clobbered
    When the shell asks to correct "cat"
    Then "cat" is returned unchanged with no correction applied

  @BR-16 @mvp
  Scenario: Active languages can be switched
    When the shell activates English and Spanish
    Then the active languages are English then Spanish

  @BR-7 @BR-8 @mvp
  Scenario: Learned vocabulary survives a save and reload
    Given the user has added "zebra" to their dictionary
    When the core is persisted and a fresh core reloads from the secure store
    Then "zebra" is known to the reloaded core
