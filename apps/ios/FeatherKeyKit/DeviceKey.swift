import Foundation
import Security

public enum DeviceKeyError: Error { case unexpectedStatus(OSStatus), rng }

/// A 32-byte device key persisted in the iOS Keychain — the iOS analog of the
/// Android Keystore-backed key (BR-62). The shared core's `open()` requires it to
/// key the encrypted store.
public enum DeviceKey {
    private static let service = "com.featherkey.ios.deviceKey"

    /// The persisted 32-byte key, generating + storing it on first call.
    public static func loadOrCreate(account: String) throws -> Data {
        if let existing = try load(account: account) { return existing }
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess
        else { throw DeviceKeyError.rng }
        let key = Data(bytes)
        let add: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecValueData as String: key,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw DeviceKeyError.unexpectedStatus(status) }
        return key
    }

    static func load(account: String) throws -> Data? {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var out: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &out)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else { throw DeviceKeyError.unexpectedStatus(status) }
        return out as? Data
    }

    public static func delete(account: String) throws {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let status = SecItemDelete(q as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound
        else { throw DeviceKeyError.unexpectedStatus(status) }
    }
}
