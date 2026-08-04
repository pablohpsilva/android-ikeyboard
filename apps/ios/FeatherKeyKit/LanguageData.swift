import Foundation

/// One language's lexicon expressed in the shell's own vocabulary — free of UIKit
/// and of the generated FFI binding, so it is host-testable and can be built by the
/// platform shell. `CoreKeyboardEngine` maps it across the FFI to the core's
/// `LanguagePack`.
///
/// `words` MUST be in frequency order (most-common first): the core records each
/// word's input position as its bundled rank, so re-sorting here would corrupt the
/// ranking the whole prediction stack depends on. This mirrors the Android shell,
/// which passes the asset lines in file order.
public struct LanguageData: Equatable {
    public let tag: String
    public let words: [String]
    public let proper: [String]

    public init(tag: String, words: [String], proper: [String]) {
        self.tag = tag
        self.words = words
        self.proper = proper
    }
}
