import XCTest
@testable import FeatherKeyKit

/// A `LayoutProjection` maps a raw screen-space swipe point into the core's logical
/// frame. It must be a *continuous* affine fit — an off-grid point projects to the
/// interpolated logical coordinate, never snapped to the nearest key.
final class LayoutProjectionTests: XCTestCase {

    /// logical = screen*0.5 + 10 on x, screen*0.25 - 5 on y.
    private func gridPairs() -> [(screen: GesturePoint, logical: GesturePoint)] {
        var pairs: [(screen: GesturePoint, logical: GesturePoint)] = []
        for sx in stride(from: Float(0), through: 300, by: 100) {
            for sy in stride(from: Float(0), through: 200, by: 100) {
                let s = GesturePoint(x: sx, y: sy)
                let l = GesturePoint(x: sx * 0.5 + 10, y: sy * 0.25 - 5)
                pairs.append((s, l))
            }
        }
        return pairs
    }

    func test_reproduces_a_known_grid_point_exactly() {
        let p = LayoutProjection(pairs: gridPairs())
        let out = p.toLogical(GesturePoint(x: 200, y: 100))
        XCTAssertEqual(out.x, 200 * 0.5 + 10, accuracy: 1e-3)
        XCTAssertEqual(out.y, 100 * 0.25 - 5, accuracy: 1e-3)
    }

    func test_off_grid_point_interpolates_and_is_not_snapped() {
        let p = LayoutProjection(pairs: gridPairs())
        // A point strictly between two rows/columns — must map to the interpolated
        // affine value, proving no key-snapping.
        let out = p.toLogical(GesturePoint(x: 137, y: 63))
        XCTAssertEqual(out.x, 137 * 0.5 + 10, accuracy: 1e-3)
        XCTAssertEqual(out.y, 63 * 0.25 - 5, accuracy: 1e-3)
    }

    func test_degenerate_axis_does_not_crash_and_is_finite() {
        // All pairs share one x → zero variance on x; must fall back, not divide by 0.
        let pairs: [(screen: GesturePoint, logical: GesturePoint)] = [
            (GesturePoint(x: 50, y: 0),   GesturePoint(x: 99, y: 0)),
            (GesturePoint(x: 50, y: 100), GesturePoint(x: 99, y: 25)),
            (GesturePoint(x: 50, y: 200), GesturePoint(x: 99, y: 50)),
        ]
        let p = LayoutProjection(pairs: pairs)
        let out = p.toLogical(GesturePoint(x: 50, y: 80))
        XCTAssertTrue(out.x.isFinite && out.y.isFinite)
        XCTAssertEqual(out.y, 80 * 0.25, accuracy: 1e-3)   // y axis still fits
    }
}
