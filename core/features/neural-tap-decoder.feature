Feature: The tap decoder learns the user's systematic aim and generalizes it

  # BR-7: the keyboard learns the user's typing style and becomes measurably
  # more accurate for that user — here, across keys, not just per key.

  @BR-7
  Scenario: A cold-start warp does not change decoding
    Given a fresh tap-warp model
    When the user taps at a spread of positions across the keyboard
    Then every decoded key and candidate order is identical to decoding with no warp

  @BR-7
  Scenario: A systematic hand-bias generalizes to an un-tapped key
    Given a user who consistently taps several keys off-centre in the same direction
    When the tap-warp model has learned from those taps
    And the user taps a different key they have never tapped, with the same bias
    Then the decoder resolves the intended un-tapped key
    And an unbiased decoder would have mis-resolved it to a neighbour

  @BR-7
  Scenario: The warp does not double-correct a well-learned key
    Given a key whose per-key mean offset has already converged
    When the user taps it on its learned centre
    Then the warp contributes approximately zero additional shift

  @BR-7
  Scenario: Learning is suppressed in a sensitive field
    Given a password field
    When the user taps keys off-centre
    Then the tap-warp model is not updated
