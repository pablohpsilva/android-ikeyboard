# BDD specification — the BR-26 sensitive-context suppression gate.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/sensitive-context/tests/br26_gate.rs; as the cucumber harness is wired
# up (SEDD §12), these steps bind directly to it.

Feature: Suppress learning in sensitive fields
  As a person typing a password or one-time code
  I want FeatherKey to never learn from that field
  So that my secrets are never persisted or predicted back

  @BR-26 @mvp
  Scenario: A password field suppresses learning
    Given the editor reports the field is sensitive
    When the composition root consults the suppression gate
    Then learning and prediction are suppressed

  @BR-26 @mvp
  Scenario: An ordinary field allows learning
    Given the editor reports the field is not sensitive
    When the composition root consults the suppression gate
    Then learning and prediction proceed
