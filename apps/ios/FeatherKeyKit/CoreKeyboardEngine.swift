import Foundation
import Security

/// Adapts the UniFFI-generated `KeyboardCore` to the `KeyboardEngine` port.
/// This is the ONLY type that talks to the generated binding.
public final class CoreKeyboardEngine: KeyboardEngine {
    private let core: KeyboardCore

    /// Opens the shared core over an encrypted store in `containerDir`, keyed by a
    /// 32-byte device key kept in the SAME container. Deliberately NOT the Keychain:
    /// a keyboard extension without Full Access cannot reach the Keychain, so a
    /// Keychain-backed key throws and the keyboard renders no letters. The store is
    /// required by the core's sole constructor; this slice issues no learn/persist
    /// calls against it, so a container-file key is sufficient here (a Keychain- or
    /// App-Group-backed key returns with the persistence/learning slice).
    ///
    /// `languages` are supplied by the shell (see `BundledLexicon`) so this adapter
    /// stays free of any bundle/resource knowledge. `words` are passed to the core in
    /// the caller's order — the core treats input position as frequency rank.
    public init(containerDir: URL, languages: [LanguageData]) throws {
        let key = try Self.deviceKey(in: containerDir)
        let dbPath = containerDir.appendingPathComponent("featherkey.redb").path
        let packs = languages.map {
            LanguagePack(tag: $0.tag, words: $0.words, proper: $0.proper)
        }
        self.core = try KeyboardCore.open(dbPath: dbPath, deviceKey: key, languages: packs)
        self.core.useAlphaLayout()
    }

    public func layoutKeys() -> [EngineKey] {
        core.layoutKeys().map {
            EngineKey(label: $0.label, x: $0.x, y: $0.y, width: $0.width, height: $0.height)
        }
    }

    public func decode(atLogicalX x: Float, y: Float) throws -> String {
        try core.decode(x: x, y: y).best ?? ""
    }

    public func suggestions(preceding: String, prefix: String) -> [String] {
        core.suggest(preceding: preceding, prefix: prefix).map { $0.word }
    }

    public func properCase(word: String, isSentenceStart: Bool) -> String? {
        core.properCase(word: word, isSentenceStart: isSentenceStart)
    }

    /// No device dictionary on iOS (unlike Android's shell), so both known-word and
    /// candidate lists are empty — the core corrects from the active languages. The
    /// core owns the decision; the shell only unwraps "applied and actually changed".
    public func correction(for word: String) -> String? {
        guard let c = try? core.chooseCorrection(text: word, deviceKnown: [], deviceCands: []) else {
            return nil
        }
        return c.applied && c.primary != word ? c.primary : nil
    }

    /// Marshal the shell's logical-frame path to the core's `FfiPoint` and return the
    /// ranked words. The core owns the vocabulary, key centres, and learned data —
    /// the shell passes only the finger path (BR-41).
    public func decodeGesture(points: [GesturePoint]) -> [String] {
        core.decodeGesture(points: points.map { FfiPoint(x: $0.x, y: $0.y) }).map { $0.word }
    }

    /// A 32-byte key persisted in the extension's own container — reachable without
    /// Full Access, unlike the Keychain. Generated once, reloaded thereafter.
    private static func deviceKey(in dir: URL) throws -> Data {
        let url = dir.appendingPathComponent("device.key")
        if let d = try? Data(contentsOf: url), d.count == 32 { return d }
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
            throw NSError(domain: "FeatherKey", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "RNG failed"])
        }
        let d = Data(bytes)
        try d.write(to: url, options: .atomic)
        return d
    }
}
