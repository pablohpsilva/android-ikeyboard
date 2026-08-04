import XCTest
@testable import FeatherKeyKit

/// A stand-in `KeyboardEngine` so the boundary logic is tested without the real
/// core: it records what it was asked and returns scripted decisions. Proves the
/// decision-making lives in the port calls, not in the Swift decider.
private final class FakeEngine: KeyboardEngine {
    var properCaseResult: String?
    var correctionResult: String?
    var gestureResult: [String] = []
    private(set) var properCaseCalls: [(word: String, sentenceStart: Bool)] = []
    private(set) var correctionCalls: [String] = []
    private(set) var gestureCalls: [[GesturePoint]] = []

    func layoutKeys() -> [EngineKey] { [] }
    func decode(atLogicalX x: Float, y: Float) throws -> String { "" }
    func suggestions(preceding: String, prefix: String) -> [String] { [] }

    func properCase(word: String, isSentenceStart: Bool) -> String? {
        properCaseCalls.append((word, isSentenceStart)); return properCaseResult
    }
    func correction(for word: String) -> String? {
        correctionCalls.append(word); return correctionResult
    }
    func decodeGesture(points: [GesturePoint]) -> [String] {
        gestureCalls.append(points); return gestureResult
    }
}

final class WordBoundaryTests: XCTestCase {

    // MARK: - The commit decision (proper-case wins over correction; core owns both)

    func test_correction_from_core_replaces_the_typed_word() {
        let e = FakeEngine(); e.correctionResult = "the"
        let d = WordBoundary.decide(typed: "teh", precedingText: "", engine: e)
        XCTAssertEqual(d.resolved, "the")
        XCTAssertTrue(d.isCorrection)
        XCTAssertEqual(e.correctionCalls, ["teh"])
    }

    func test_proper_case_wins_over_edit_distance_correction() {
        let e = FakeEngine()
        e.properCaseResult = "London"   // core's proper-case decision
        e.correctionResult = "loudon"   // an edit-distance rival that must NOT win
        let d = WordBoundary.decide(typed: "london", precedingText: "in ", engine: e)
        XCTAssertEqual(d.resolved, "London")
        XCTAssertTrue(e.correctionCalls.isEmpty, "proper-case short-circuits correction")
    }

    func test_unchanged_word_is_not_a_correction() {
        let e = FakeEngine()   // both nil → nothing to apply
        let d = WordBoundary.decide(typed: "hello", precedingText: "", engine: e)
        XCTAssertEqual(d.resolved, "hello")
        XCTAssertFalse(d.isCorrection)
    }

    func test_mixed_case_word_is_never_edit_distance_corrected() {
        let e = FakeEngine(); e.correctionResult = "iphone"   // must be ignored
        let d = WordBoundary.decide(typed: "iPhone", precedingText: "", engine: e)
        XCTAssertEqual(d.resolved, "iPhone")
        XCTAssertTrue(e.correctionCalls.isEmpty, "mixed-case tokens are left as typed")
    }

    func test_empty_word_short_circuits_the_core() {
        let e = FakeEngine(); e.correctionResult = "x"
        let d = WordBoundary.decide(typed: "", precedingText: "hi ", engine: e)
        XCTAssertEqual(d.resolved, "")
        XCTAssertFalse(d.isCorrection)
        XCTAssertTrue(e.correctionCalls.isEmpty && e.properCaseCalls.isEmpty)
    }

    // MARK: - Sentence-start detection feeds proper-case / auto-caps

    func test_sentence_start_is_passed_to_proper_case() {
        for (text, expected) in [("", true), ("hi. ", true), ("word! ", true),
                                  ("a?\n", true), ("in ", false), ("the cat ", false)] {
            let e = FakeEngine()
            _ = WordBoundary.decide(typed: "paris", precedingText: text, engine: e)
            XCTAssertEqual(e.properCaseCalls.first?.sentenceStart, expected,
                           "precedingText \(text.debugDescription)")
        }
    }

    // MARK: - Revert: a backspace right after an autocorrect restores the typed word

    func test_revert_applies_when_the_field_ends_with_the_correction() {
        let r = WordBoundary.Revert(typed: "teh", corrected: "the")
        let edit = WordBoundary.revert(r, precedingText: "I saw the ")
        XCTAssertEqual(edit?.delete, 4)          // "the" + trailing space
        XCTAssertEqual(edit?.insert, "teh")
    }

    func test_revert_does_not_fire_when_the_correction_is_not_at_the_cursor() {
        let r = WordBoundary.Revert(typed: "teh", corrected: "the")
        XCTAssertNil(WordBoundary.revert(r, precedingText: "the cat sat "))
        XCTAssertNil(WordBoundary.revert(nil, precedingText: "the "))
    }
}
