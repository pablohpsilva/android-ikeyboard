# BDD specification — the personalized autocorrect gate.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in the featherkey-core integration
# test `core/crates/featherkey-core/tests/autocorrect_gate.rs`; as the cucumber
# harness is wired up (SEDD §12), these steps bind directly to it. The gate
# (`docs/superpowers/specs/2026-07-31-autocorrect-gate-design.md`) is a tiny
# per-user MLP residual on the `NoClobberCorrector`'s apply threshold: it learns,
# from real revert/keep/reach outcomes, how aggressively to apply the corrections
# the no-clobber policy already permits (BR-15's learned-aggressiveness advance,
# without a user-facing slider) — but the no-clobber veto (BR-12) runs first and
# is absolute, so the gate is never even consulted for a word the user clearly
# intended, no matter how the gate has been trained.

Feature: The autocorrect gate learns when to trust a correction
  As a person typing on FeatherKey
  I want autocorrect to stop repeating a fix I keep reverting
  So that it becomes less annoying over time — without ever overwriting a word I clearly intended

  Background:
    Given the English lexicon contains "cat", "dog", "hat", and "bat" in frequency order

  @BR-12
  Scenario: A strong correction still applies at cold start
    When I finish typing the token "xat"
    Then a correction is applied
    And the primary candidate is "cat"

  @BR-12
  Scenario: Repeatedly reverting one correction suppresses it
    Given I have reverted the "xat" correction to "cat" eight times
    When I finish typing the token "xat"
    Then no correction is applied
    And "cat" is surfaced as the withheld candidate

  @BR-12
  Scenario: A known word is never clobbered no matter how eager the gate has become
    Given the gate has been reinforced toward "apply" two hundred times
    When I finish typing the token "cat"
    Then the committed word is "cat"
    And no correction is applied

  @BR-12
  Scenario: A sensitive field records nothing for the gate
    Given I have reverted the "xat" correction to "cat" eight times while the field is sensitive
    When I finish typing the token "xat"
    Then a correction is applied
    And the primary candidate is "cat"
