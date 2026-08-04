import XCTest
@testable import FeatherKeyKit

final class KeyboardGeometryTests: XCTestCase {
    func test_logicalBounds_is_the_union_extent_of_keys() {
        let keys = [
            EngineKey(label: "a", x: 0, y: 0, width: 100, height: 360),
            EngineKey(label: "b", x: 900, y: 0, width: 100, height: 360),
        ]
        let b = KeyboardGeometry.logicalBounds(keys)
        XCTAssertEqual(b.width, 1000, accuracy: 0.001)   // 900 + 100
        XCTAssertEqual(b.height, 360, accuracy: 0.001)
    }

    func test_toLogical_uses_independent_x_y_affine_scale() {
        let logical = LogicalSize(width: 1000, height: 360)
        // A touch at the centre of a 320x216 view → centre of logical space.
        let p = KeyboardGeometry.toLogical(viewX: 160, viewY: 108,
                                           viewWidth: 320, viewHeight: 216,
                                           logical: logical)
        XCTAssertEqual(p.x, 500, accuracy: 0.001)   // 160 * 1000/320
        XCTAssertEqual(p.y, 180, accuracy: 0.001)   // 108 * 360/216
    }

    func test_toLogical_maps_a_known_key_centre_back_to_that_key() {
        let logical = LogicalSize(width: 1000, height: 360)
        // Key "b" centre is logical (950,180); it renders at view x =
        // 950 * 320/1000 = 304, y = 180 * 216/360 = 108. Round-trips back.
        let p = KeyboardGeometry.toLogical(viewX: 304, viewY: 108,
                                           viewWidth: 320, viewHeight: 216,
                                           logical: logical)
        XCTAssertEqual(p.x, 950, accuracy: 0.001)
        XCTAssertEqual(p.y, 180, accuracy: 0.001)
    }
}
