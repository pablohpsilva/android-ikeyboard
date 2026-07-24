# BDD specification — encrypted personal-data persistence.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/secure-store/tests/roundtrip.rs; as the cucumber harness is wired up
# (SEDD §12), these steps bind directly to it.

Feature: Encrypted persistence of personal data
  As a person whose typing personalises FeatherKey
  I want my learned data stored encrypted at rest
  So that a stolen device or backup never leaks it in the clear

  Background:
    Given a secure store opened with a fixed 32-byte key

  @BR-8 @mvp
  Scenario: A stored value round-trips through encryption
    When I put the value "hello" under namespace "user_dict" key "greeting"
    Then getting namespace "user_dict" key "greeting" returns "hello"

  @BR-23 @mvp
  Scenario: An absent key reads back as nothing
    When I get namespace "user_dict" key "never_written"
    Then the result is empty

  @BR-62 @mvp
  Scenario: A wrong key cannot decrypt the ciphertext
    Given a value was stored under the fixed key
    When I reopen the store with a different 32-byte key
    And I get the previously stored key
    Then a crypto error is returned

  @BR-8 @mvp
  Scenario: Namespaces are isolated from one another
    When I put the value "a" under namespace "user_dict" key "k"
    And I put the value "b" under namespace "clipboard" key "k"
    Then getting namespace "user_dict" key "k" returns "a"
    And getting namespace "clipboard" key "k" returns "b"
