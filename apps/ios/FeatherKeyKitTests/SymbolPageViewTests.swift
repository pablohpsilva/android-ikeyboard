import XCTest
import UIKit
@testable import FeatherKeyKit
// SymbolPageView / KeyboardTheme are compiled into the test target directly
// (the FeatherKeyKeyboard sources are members of this test bundle).

/// @BR-47 @BR-70 — the number/symbol pages insert their literal characters (no core
/// decode; these are UI-owned pages, exactly as the Android shell models them).
final class SymbolPageViewTests: XCTestCase {
    private let theme = KeyboardTheme.resolved(for: UITraitCollection(userInterfaceStyle: .light))

    private func laidOut() -> SymbolPageView {
        let v = SymbolPageView()
        v.frame = CGRect(x: 0, y: 0, width: 393, height: 260)
        v.configure(page: .numbers, theme: theme)
        v.layoutIfNeeded()
        return v
    }

    private func button(_ title: String, in v: UIView) -> UIButton? {
        for sub in v.subviews {
            if let b = sub as? UIButton, b.title(for: .normal) == title { return b }
            if let found = button(title, in: sub) { return found }
        }
        return nil
    }

    func test_number_page_key_inserts_its_literal() {
        let v = laidOut()
        var inserted: [String] = []
        v.onInsert = { inserted.append($0) }
        XCTAssertNotNil(button("5", in: v), "number page must show a '5' key")
        button("5", in: v)?.sendActions(for: .touchUpInside)
        XCTAssertEqual(inserted, ["5"])
    }

    func test_toggle_switches_to_symbols_and_inserts_symbol_literal() {
        let v = laidOut()
        var inserted: [String] = []
        v.onInsert = { inserted.append($0) }
        // On the number page, the "#+=" key switches to the symbol page.
        XCTAssertNotNil(button("#+=", in: v), "number page must offer the #+= toggle")
        button("#+=", in: v)?.sendActions(for: .touchUpInside)
        v.layoutIfNeeded()
        XCTAssertNotNil(button("#", in: v), "symbol page must show a '#' key")
        button("#", in: v)?.sendActions(for: .touchUpInside)
        XCTAssertEqual(inserted, ["#"])
        // And the symbol page offers the "123" toggle back to numbers.
        XCTAssertNotNil(button("123", in: v))
    }

    func test_ABC_key_requests_return_to_alpha() {
        let v = laidOut()
        var wentBack = false
        v.onBackToAlpha = { wentBack = true }
        button("ABC", in: v)?.sendActions(for: .touchUpInside)
        XCTAssertTrue(wentBack)
    }

    func test_rows_match_the_android_character_set() {
        let v = laidOut()
        // Parity: same literals the Android keyboard shows (KeyboardView.kt).
        for c in ["1", "0", "-", "/", ":", ";", "(", ")", "$", "&", "@", "\""] {
            XCTAssertNotNil(button(c, in: v), "number page missing '\(c)'")
        }
        button("#+=", in: v)?.sendActions(for: .touchUpInside)
        v.layoutIfNeeded()
        for c in ["[", "]", "{", "}", "#", "%", "^", "*", "+", "=",
                  "_", "\\", "|", "~", "<", ">", "€", "£", "¥", "•"] {
            XCTAssertNotNil(button(c, in: v), "symbol page missing '\(c)'")
        }
    }
}
