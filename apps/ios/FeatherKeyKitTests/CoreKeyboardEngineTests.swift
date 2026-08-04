import XCTest
@testable import FeatherKeyKit

final class CoreKeyboardEngineTests: XCTestCase {
    func test_decode_at_a_key_centre_returns_that_keys_letter() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }

        let langs = [LanguageData(tag: "en", words: ["hello", "the", "world"], proper: [])]
        let engine = try CoreKeyboardEngine(containerDir: dir, languages: langs)
        let keys = engine.layoutKeys()
        XCTAssertFalse(keys.isEmpty, "core must expose a layout")

        // Pick a known letter key and decode at its centre; expect that letter.
        let h = try XCTUnwrap(keys.first { $0.label == "h" })
        let got = try engine.decode(atLogicalX: h.x + h.width / 2, y: h.y + h.height / 2)
        XCTAssertEqual(got, "h")
    }

    /// The whole swipe FFI path: a glide over h-e-l-l-o through the real layout
    /// centres decodes to "hello" via the shared core (BR-41).
    func test_a_swipe_over_the_letters_decodes_to_that_word_via_the_core() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }

        let langs = [LanguageData(tag: "en", words: ["hello", "help", "hero", "world"], proper: [])]
        let engine = try CoreKeyboardEngine(containerDir: dir, languages: langs)
        let keys = engine.layoutKeys()
        let centre: (Character) -> GesturePoint? = { ch in
            keys.first { $0.label == String(ch) }
                .map { GesturePoint(x: $0.x + $0.width / 2, y: $0.y + $0.height / 2) }
        }
        let path = "hello".compactMap(centre)
        XCTAssertGreaterThanOrEqual(path.count, 3)
        XCTAssertEqual(engine.decodeGesture(points: path).first, "hello")
    }
}
