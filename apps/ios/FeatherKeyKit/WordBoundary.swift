import Foundation

/// The word-boundary commit decision, computed entirely from the shared core — it
/// holds no typing rules of its own. When the user taps space, the shell asks
/// whether the core wants to replace the just-typed word: a proper-noun / auto-caps
/// spelling first (BR-69), then a momentum-aware edit-distance correction (BR-12).
/// Pure and UIKit-free so it is host-testable; the controller performs the field
/// I/O around it. Mirrors the ordering the Android shell uses at its boundary.
public enum WordBoundary {

    /// What to commit for the just-typed `typed` word. `isCorrection` is true when
    /// the core changed it, which is what arms the one-slot revert.
    public struct Decision: Equatable {
        public let typed: String
        public let resolved: String
        public var isCorrection: Bool { resolved != typed }
    }

    /// An armed autocorrect the user can undo with the very next backspace.
    public struct Revert: Equatable {
        public let typed: String       // what the user actually typed
        public let corrected: String   // what the core replaced it with
        public init(typed: String, corrected: String) {
            self.typed = typed; self.corrected = corrected
        }
    }

    /// The commit for `typed` given the `precedingText` already in the field.
    /// Proper-case wins over correction (short-circuits it, matching Android); a
    /// mixed-case token (`iPhone`, `NASA`) is never edit-distance corrected.
    public static func decide(typed: String, precedingText: String,
                              engine: KeyboardEngine) -> Decision {
        guard !typed.isEmpty else { return Decision(typed: typed, resolved: typed) }
        let sentenceStart = startsSentence(precedingText)
        let out: String
        if let proper = engine.properCase(word: typed, isSentenceStart: sentenceStart) {
            out = proper
        } else if typed == typed.lowercased(), let fix = engine.correction(for: typed) {
            out = fix
        } else {
            out = typed
        }
        return Decision(typed: typed, resolved: out)
    }

    /// The edit that undoes an autocorrect when a backspace immediately follows it:
    /// only when the field still ends with the corrected word and its space. Returns
    /// how many characters to delete and the original word to restore, or nil when
    /// the backspace is not a revert (nothing armed, or the cursor has moved on).
    public static func revert(_ armed: Revert?, precedingText: String)
        -> (delete: Int, insert: String)? {
        guard let r = armed, precedingText.hasSuffix(r.corrected + " ") else { return nil }
        return (delete: r.corrected.count + 1, insert: r.typed)
    }

    /// True when `precedingText` (the field text just before the typed word) is
    /// empty or ends a sentence — so the word begins a new one. Mirrors the Android
    /// auto-caps rule: end-of-sentence punctuation (optionally trailing spaces) or a
    /// newline.
    static func startsSentence(_ precedingText: String) -> Bool {
        var s = Substring(precedingText)
        while s.last == " " { s = s.dropLast() }
        guard let last = s.last else { return true }
        return last == "." || last == "!" || last == "?" || last == "\n"
    }
}
