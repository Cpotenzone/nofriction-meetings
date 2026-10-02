import Foundation
import Security

/// API keys live only here: generic passwords, service com.nofriction.meetings.ai,
/// account = provider id, this device only, never synced to iCloud.
enum KeychainStore {
    static let service = "com.nofriction.meetings.ai"

    enum Failure: LocalizedError {
        case status(OSStatus)
        var errorDescription: String? {
            if case .status(let s) = self {
                return "Couldn't save the key to the Keychain (\(s))."
            }
            return nil
        }
    }

    private static func base(_ account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrSynchronizable as String: kCFBooleanFalse!,
        ]
    }

    static func set(_ secret: String, for account: String) throws {
        SecItemDelete(base(account) as CFDictionary)
        var q = base(account)
        q[kSecValueData as String] = Data(secret.utf8)
        q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(q as CFDictionary, nil)
        guard status == errSecSuccess else { throw Failure.status(status) }
    }

    static func get(_ account: String) -> String? {
        var q = base(account)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let data = out as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    static func has(_ account: String) -> Bool { get(account) != nil }

    static func delete(_ account: String) {
        SecItemDelete(base(account) as CFDictionary)
    }
}
