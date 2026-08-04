import Foundation

/// One renderable key of the active layout, in the core's logical coordinate
/// space. UIKit-free so it is host-testable. Mirrors the core's `FfiKey`.
public struct EngineKey: Equatable {
    public let label: String
    public let x, y, width, height: Float
    public init(label: String, x: Float, y: Float, width: Float, height: Float) {
        self.label = label; self.x = x; self.y = y; self.width = width; self.height = height
    }
}

/// The sole seam the keyboard extension depends on. Its implementation wraps the
/// shared Rust core; the extension never touches the generated binding directly
/// (DIP — swappable, e.g. a fake in tests). No typing logic lives here or above.
public protocol KeyboardEngine {
    func layoutKeys() -> [EngineKey]
    /// The character the core decodes at a logical-space point ("" if none).
    func decode(atLogicalX x: Float, y: Float) throws -> String
    /// Ranked next-word/completion candidates from the shared core for the current
    /// `prefix` in `preceding` context (both taken from the field's text).
    func suggestions(preceding: String, prefix: String) -> [String]
    /// The core's canonical proper-noun / auto-capitalized spelling for `word` at a
    /// word boundary, or nil to keep it as typed. `isSentenceStart` hands the word's
    /// position to the core's auto-capitalization (BR-69).
    func properCase(word: String, isSentenceStart: Bool) -> String?
    /// The core's momentum-aware edit-distance correction for a fully-typed `word`,
    /// or nil to keep it as typed. iOS supplies no device dictionary, so the core
    /// decides from the active languages alone (BR-12).
    func correction(for word: String) -> String?
    /// Ranked words the shared core decodes from a swipe/glide path, best first —
    /// empty if it is not a gesture. `points` are in the core's logical frame (the
    /// same frame `layoutKeys` reports), so the shell projects raw touches first
    /// (BR-41).
    func decodeGesture(points: [GesturePoint]) -> [String]
}
