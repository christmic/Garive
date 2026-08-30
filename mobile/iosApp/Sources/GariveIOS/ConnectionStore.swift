import Foundation
import Security

struct ConnectionCredentials: Equatable {
    let origin: String
    let accessGrant: String
}

final class ConnectionStore {
    private let defaults: UserDefaults
    private let originKey = "mobile.remote.origin"
    private let service = "com.garive.mobile.remote"
    private let account = "access-grant"

    init(defaults: UserDefaults = .standard) { self.defaults = defaults }

    func load() -> ConnectionCredentials? {
        guard let origin = defaults.string(forKey: originKey),
              let grant = readGrant(), !grant.isEmpty else { return nil }
        return ConnectionCredentials(origin: origin, accessGrant: grant)
    }

    func save(_ credentials: ConnectionCredentials) throws {
        let query = baseQuery.merging([
            kSecValueData as String: Data(credentials.accessGrant.utf8),
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]) { _, new in new }
        SecItemDelete(baseQuery as CFDictionary)
        let status = SecItemAdd(query as CFDictionary, nil)
        guard status == errSecSuccess else { throw ConnectionStoreError.keychain(status) }
        defaults.set(credentials.origin, forKey: originKey)
    }

    func clear() {
        SecItemDelete(baseQuery as CFDictionary)
        defaults.removeObject(forKey: originKey)
    }

    private var baseQuery: [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: account]
    }

    private func readGrant() -> String? {
        let query = baseQuery.merging([
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]) { _, new in new }
        var value: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &value) == errSecSuccess,
              let data = value as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }
}

enum ConnectionStoreError: Error { case keychain(OSStatus) }
