import XCTest
@testable import FeatherKeyKit

/// @BR-10 @BR-70 — the iOS shell must feed the SAME English word list the Android
/// app ships into the shared core, so the suggestion strip offers real completions.
final class BundledLexiconTests: XCTestCase {
    private var testBundle: Bundle { Bundle(for: Self.self) }

    func test_loads_english_lexicon_in_frequency_order() {
        let langs = BundledLexicon.load(tags: ["en"], from: testBundle)
        let en = langs.first { $0.tag == "en" }
        XCTAssertNotNil(en, "an 'en' language must be produced")
        // The real list is ~11.8k words; a tiny count means the resource is missing.
        XCTAssertGreaterThan(en?.words.count ?? 0, 10_000)
        // File order is frequency rank — must be preserved, never sorted.
        XCTAssertEqual(en?.words.first, "the")
        XCTAssertGreaterThan(en?.proper.count ?? 0, 100)
    }

    func test_bundled_lexicon_yields_prefix_completions_through_the_core() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }

        let langs = BundledLexicon.load(tags: ["en"], from: testBundle)
        let engine = try CoreKeyboardEngine(containerDir: dir, languages: langs)

        let completions = engine.suggestions(preceding: "", prefix: "th")
        XCTAssertFalse(completions.isEmpty, "a real lexicon must produce completions for 'th'")
        XCTAssertTrue(completions.allSatisfy { $0.lowercased().hasPrefix("th") },
                      "every completion for prefix 'th' must begin with 'th'; got \(completions)")
    }
}
