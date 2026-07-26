# BDD specification — opt-in, content-free diagnostics ring buffer.
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/diagnostics/src/lib.rs (module `tests`); as the cucumber harness is
# wired up (SEDD §12), these steps bind directly to it.

Feature: Content-free diagnostics ring buffer
  As a person who has opted in to diagnostics
  I want the keyboard to retain only bounded, content-free event codes
  So that I get useful telemetry without any of my typed text being recorded

  @BR-60 @mvp
  Scenario: The buffer wraps at capacity, dropping the oldest event
    Given a diagnostics buffer with capacity 3
    When I record the codes "Startup, LayoutSwitched, DecodeError, StoreWriteFailed"
    Then the snapshot holds 3 events
    And the oldest retained event's code is "LayoutSwitched"
    And the newest retained event's code is "StoreWriteFailed"

  @BR-60 @mvp
  Scenario: Recorded events carry a clock timestamp but never user text
    Given a diagnostics buffer with capacity 4 and a clock starting at 100
    When I record the code "Startup"
    Then the event's code is "Startup"
    And the event's timestamp is 100
    And the event has no text field
