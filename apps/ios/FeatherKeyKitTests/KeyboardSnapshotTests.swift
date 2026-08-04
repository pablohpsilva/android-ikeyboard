import XCTest
import UIKit
@testable import FeatherKeyKit

/// Renders the real keyboard view to a PNG so its native look can be reviewed
/// off-device (no keyboard-enable needed). Writes to $SNAPSHOT_DIR or /tmp.
final class KeyboardSnapshotTests: XCTestCase {
    func test_render_keyboard_snapshot_light_and_dark() throws {
        for style in [UIUserInterfaceStyle.light, .dark] {
            let vc = KeyboardViewController()
            let container = URL(fileURLWithPath: NSTemporaryDirectory())
                .appendingPathComponent(UUID().uuidString)
            try FileManager.default.createDirectory(at: container, withIntermediateDirectories: true)
            vc.containerDirOverride = container
            let size = CGSize(width: 393, height: 300)  // iPhone portrait keyboard-ish
            vc.overrideUserInterfaceStyle = style
            vc.view.frame = CGRect(origin: .zero, size: size)
            vc.view.overrideUserInterfaceStyle = style
            vc.viewWillAppear(false)
            vc.view.setNeedsLayout()
            vc.view.layoutIfNeeded()

            let renderer = UIGraphicsImageRenderer(size: size)
            let img = renderer.image { _ in
                vc.view.drawHierarchy(in: vc.view.bounds, afterScreenUpdates: true)
            }
            let dir = ProcessInfo.processInfo.environment["SNAPSHOT_DIR"] ?? NSTemporaryDirectory()
            let name = style == .light ? "keyboard-light.png" : "keyboard-dark.png"
            let url = URL(fileURLWithPath: dir).appendingPathComponent(name)
            try img.pngData()?.write(to: url)
            print("SNAPSHOT_WROTE \(url.path)")
        }
    }

    /// Renders the number and symbol pages so their native look can be reviewed.
    func test_render_symbol_pages() throws {
        let cases: [(SymbolPageView.Page, UIUserInterfaceStyle, String)] = [
            (.numbers, .light, "numbers-light.png"),
            (.symbols, .dark, "symbols-dark.png"),
        ]
        for (page, style, name) in cases {
            let size = CGSize(width: 393, height: 260)
            let v = SymbolPageView()
            v.overrideUserInterfaceStyle = style
            v.frame = CGRect(origin: .zero, size: size)
            v.backgroundColor = KeyboardTheme
                .resolved(for: UITraitCollection(userInterfaceStyle: style)).keyboardBackground
            v.configure(page: page,
                        theme: .resolved(for: UITraitCollection(userInterfaceStyle: style)))
            v.setNeedsLayout(); v.layoutIfNeeded()

            let renderer = UIGraphicsImageRenderer(size: size)
            let img = renderer.image { _ in v.drawHierarchy(in: v.bounds, afterScreenUpdates: true) }
            let dir = ProcessInfo.processInfo.environment["SNAPSHOT_DIR"] ?? NSTemporaryDirectory()
            let url = URL(fileURLWithPath: dir).appendingPathComponent(name)
            try img.pngData()?.write(to: url)
            print("SNAPSHOT_WROTE \(url.path)")
        }
    }
}
