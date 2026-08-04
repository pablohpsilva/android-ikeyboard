import XCTest
@testable import FeatherKeyKit

/// A `SwipeTracker` must classify a quick tap as *not* a swipe, so glide typing
/// never steals a plain keypress (BR-41). These prove the classification and the
/// point accumulation without any UIKit or touch machinery.
final class SwipeTrackerTests: XCTestCase {

    // MARK: - Swipe vs. tap classification

    func test_short_drag_under_key_pitch_is_a_tap() {
        var t = SwipeTracker()
        t.begin(at: GesturePoint(x: 100, y: 100))
        t.move(to: GesturePoint(x: 105, y: 102))   // travel well under a key width
        XCTAssertFalse(t.isSwipe(keyPitch: 40), "a short jiggle is a tap, not a swipe")
    }

    func test_long_multi_column_drag_is_a_swipe() {
        var t = SwipeTracker()
        t.begin(at: GesturePoint(x: 100, y: 100))
        t.move(to: GesturePoint(x: 160, y: 100))   // one column over
        t.move(to: GesturePoint(x: 220, y: 100))   // spans > 2 columns, arc > pitch
        XCTAssertTrue(t.isSwipe(keyPitch: 40))
    }

    func test_long_but_single_column_wiggle_is_not_a_swipe() {
        var t = SwipeTracker()
        t.begin(at: GesturePoint(x: 100, y: 100))
        t.move(to: GesturePoint(x: 110, y: 160))   // long vertical arc, x-range < pitch
        t.move(to: GesturePoint(x: 100, y: 220))
        XCTAssertFalse(t.isSwipe(keyPitch: 40),
                       "arc length alone is not enough — must cross columns")
    }

    func test_never_moved_tracker_is_not_a_swipe() {
        var t = SwipeTracker()
        t.begin(at: GesturePoint(x: 50, y: 50))
        XCTAssertFalse(t.isSwipe(keyPitch: 40))
        let empty = SwipeTracker()
        XCTAssertFalse(empty.isSwipe(keyPitch: 40), "an empty tracker is not a swipe")
    }

    // MARK: - Path accumulation

    func test_path_is_begin_then_each_move_in_order() {
        var t = SwipeTracker()
        t.begin(at: GesturePoint(x: 1, y: 1))
        t.move(to: GesturePoint(x: 2, y: 2))
        t.move(to: GesturePoint(x: 3, y: 3))
        XCTAssertEqual(t.path, [GesturePoint(x: 1, y: 1),
                                GesturePoint(x: 2, y: 2),
                                GesturePoint(x: 3, y: 3)])
    }
}
