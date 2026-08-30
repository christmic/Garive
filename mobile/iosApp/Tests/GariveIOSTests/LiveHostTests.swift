import Testing
@testable import GariveIOS
#if canImport(GariveShared)
@preconcurrency import GariveShared
#endif

@Test
func commandIdentitiesAreUnique() {
#if canImport(GariveShared)
    let source = UUIDIdentitySource()
    #expect(source.nextId() != source.nextId())
#endif
}

@Test
func remoteConfigurationRequiresAnAccessGrant() {
#if canImport(GariveShared)
    let limits = HostClientLimits(
        maxCommandBytes: 4_096, maxEventBytes: 8_192,
        maxEvents: 256, followDeadlineMs: 120_000
    )
    #expect(throws: Error.self) {
        _ = try LiveHostClient(baseUrl: "https://example.com/", limits: limits)
    }
#endif
}

@Test @MainActor
func pairingLinksRequireExactFreshFields() {
    let defaults = UserDefaults(suiteName: "garive-test-\(UUID())")!
    let model = MobileViewModel(store: ConnectionStore(defaults: defaults))
    let expiry = Int(Date().timeIntervalSince1970) + 300
    var components = URLComponents()
    components.scheme = "garive"
    components.host = "pair"
    components.queryItems = [
        URLQueryItem(name: "origin", value: "https://agent.example.test/"),
        URLQueryItem(name: "code", value: "one-time-code"),
        URLQueryItem(name: "exp", value: String(expiry)),
        URLQueryItem(name: "name", value: "Test service"),
    ]
    model.acceptPairingURL(components.url!)
    #expect(model.pairingSuggestion?.serviceName == "Test service")

    components.queryItems?.append(URLQueryItem(name: "code", value: "duplicate"))
    model.acceptPairingURL(components.url!)
    #expect(model.pairingSuggestion == nil)
    #expect(model.errorCode == "invalid_pairing_link")
}
