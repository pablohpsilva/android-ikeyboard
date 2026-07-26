# BDD specification — panic isolation at the safe boundary.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/crash-guard/src/lib.rs (the `tests` module); as the cucumber harness is
# wired up (SEDD §12), these steps bind directly to it.

Feature: Panic isolation and safe-mode fallback
  As a person typing on FeatherKey
  I want an internal failure to degrade gracefully instead of crashing the keyboard
  So that I can keep typing without restarting my phone

  @BR-29 @BR-30 @mvp
  Scenario: A panicking operation returns a fallback instead of unwinding
    Given a guarded operation with a fallback value
    When the guarded operation panics
    Then the fallback value is returned
    And the panic does not propagate past the guard

  @BR-29 @mvp
  Scenario: A successful operation returns its value untouched
    Given a guarded operation with a fallback value
    When the guarded operation succeeds
    Then the operation's own value is returned

  @BR-31 @mvp
  Scenario: A caught panic is reported as an inspectable error
    Given a guarded operation observed via its result
    When the guarded operation panics with a message
    Then the result is an error carrying that panic message
