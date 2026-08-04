import Foundation

/// Loads FeatherKey's bundled per-language lexicons — the SAME plain-text word lists
/// the Android app ships (`assets/lexicons/<tag>.txt` and `assets/proper/<tag>.txt`),
/// referenced from a single on-disk copy so the two shells never drift (DRY).
///
/// Format (identical to Android's loader): one word per line, UTF-8. Lines are
/// trimmed and blanks dropped; **file order is preserved** because it encodes
/// frequency rank. A missing or unreadable file yields an empty list, matching the
/// Android loader's tolerance — the keyboard still renders (its layout comes from the
/// core, not from the word list).
public enum BundledLexicon {
    /// Builds a `LanguageData` per tag from the resources in `bundle`.
    public static func load(tags: [String], from bundle: Bundle) -> [LanguageData] {
        tags.map { tag in
            LanguageData(tag: tag,
                         words: lines(tag, in: "lexicons", bundle),
                         proper: lines(tag, in: "proper", bundle))
        }
    }

    private static func lines(_ tag: String, in subdir: String, _ bundle: Bundle) -> [String] {
        guard let url = bundle.url(forResource: tag, withExtension: "txt", subdirectory: subdir),
              let text = try? String(contentsOf: url, encoding: .utf8) else { return [] }
        return text.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }
}
