@BR-11
Feature: Neural next-word LM wired into the live suggestion strip
  The on-device embedding LM contributes to the strip as a re-ranker feature and
  a word-boundary candidate source, learning online under the same consent /
  sensitivity gate as the bigram, and never regressing the cold-start order.

  @BR-11
  Scenario: A warm LM reorders the strip by two-word context
    Given the LM has learned "going to work" and "walking to school"
    When I have committed "going" then "to" and ask for suggestions at the boundary
    Then "work" ranks above "school"
    And after committing "walking" then "to", "school" ranks above "work"

  @BR-10
  Scenario: Cold start does not change today's strip
    Given a fresh core whose LM has learned nothing
    When I rank any suggestion set
    Then the order is exactly the pre-LM order

  @BR-11
  Scenario: The LM surfaces a next-word the bigram never recorded
    Given the LM has learned "the cat", "an cat" and "the dog"
    When I am at a boundary after "an"
    Then "dog" appears among the suggestions

  @BR-26
  Scenario: No learning in a sensitive field
    Given a sensitive field
    When I commit words
    Then the LM learns nothing
