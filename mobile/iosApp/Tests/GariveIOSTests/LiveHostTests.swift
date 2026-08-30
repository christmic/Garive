import Testing
import Foundation
@testable import GariveIOS
#if canImport(GariveShared)
@preconcurrency import GariveShared
#endif

@Test
func connectionStoreClearsGrantAndRotatesDeviceIdentity() throws {
    let suffix = UUID().uuidString
    let suite = "garive-tests-\(suffix)"
    let defaults = try #require(UserDefaults(suiteName: suite))
    let store = ConnectionStore(
        defaults: defaults,
        originKey: "origin-\(suffix)",
        service: "com.garive.tests.\(suffix)",
        account: "grant-\(suffix)",
        deviceKeyTag: "com.garive.tests.device.\(suffix)"
    )
    defer {
        store.clear()
        defaults.removePersistentDomain(forName: suite)
    }
    let secret = "grant-that-must-never-appear-in-defaults"
    let firstDeviceKey = try store.devicePublicKey()

    try store.save(ConnectionCredentials(
        origin: "https://agent.example.test",
        accessGrant: secret
    ))

    #expect(store.load() == ConnectionCredentials(
        origin: "https://agent.example.test",
        accessGrant: secret
    ))
    #expect(!defaults.dictionaryRepresentation().values.contains {
        String(describing: $0).contains(secret)
    })

    store.clear()

    #expect(store.load() == nil)
    #expect(try store.devicePublicKey() != firstDeviceKey)
}

@Test
func pendingMutationStorageRoundTripsAndClears() throws {
    let suite = "garive-pending-tests-\(UUID())"
    let defaults = try #require(UserDefaults(suiteName: suite))
    let persistence = UserDefaultsMobileWorkPersistence(
        defaults: defaults,
        recordKey: "record",
        payloadKey: "payload",
        preferencesKey: "preferences"
    )
    defer { defaults.removePersistentDomain(forName: suite) }

    persistence.writePendingPayload(value: "exact input")
    persistence.writePendingRecord(value: "{\"schema_version\":1}")
    persistence.writePreferencesRecord(value: "{\"schema_version\":1}")

    #expect(persistence.readPendingPayload() == "exact input")
    #expect(persistence.readPendingRecord() == "{\"schema_version\":1}")
    #expect(persistence.readPreferencesRecord() == "{\"schema_version\":1}")
    persistence.writePendingRecord(value: nil)
    persistence.writePendingPayload(value: nil)
    persistence.writePreferencesRecord(value: nil)
    #expect(persistence.readPendingRecord() == nil)
    #expect(persistence.readPendingPayload() == nil)
    #expect(persistence.readPreferencesRecord() == nil)
}

@Test
func commandIdentitiesHaveExactLowercaseSortableShape() {
#if canImport(GariveShared)
    var now: UInt64 = 1_700_000_000_000
    var random: UInt8 = 1
    let source = SortableCommandIdentitySource(
        nowMillis: { defer { now += 1 }; return now },
        randomBytes: { defer { random += 1 }; return Array(repeating: random, count: 10) }
    )
    let first = source.nextId()
    let second = source.nextId()
    #expect(first.count == 26)
    #expect(first.allSatisfy { "0123456789abcdefghjkmnpqrstvwxyz".contains($0) })
    #expect(first < second)
    #expect(first != second)
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

@Test
func wakeHintsAreContentFreeAndExact() {
    let token = String(repeating: "r", count: 43)
    let valid: [AnyHashable: Any] = ["garive": [
        "schema_version": 1, "route_token": token,
        "category": "attention", "collapse_key": "attention",
    ]]
    #expect(WakeEnvelope.routeToken(from: valid) == token)

    let leaking: [AnyHashable: Any] = ["garive": [
        "schema_version": 1, "route_token": token,
        "category": "attention", "collapse_key": "attention",
        "session_id": "must-not-appear",
    ]]
    #expect(WakeEnvelope.routeToken(from: leaking) == nil)
}

@Test @MainActor
func newTaskPresentationPreservesExplicitAgentChoice() {
    let defaults = UserDefaults(suiteName: "garive-agent-choice-\(UUID())")!
    let model = MobileViewModel(store: ConnectionStore(defaults: defaults))

    model.showNewTask(definitionID: "definition-review")
    #expect(model.presentingNewTask)
    #expect(model.preferredDefinitionID == "definition-review")

    model.dismissNewTask()
    #expect(!model.presentingNewTask)
    #expect(model.preferredDefinitionID == nil)
}

@Test
func mobileGoalStartersMatchDesktopWorkOutcomes() {
    #expect(mobileGoalStarters == [
        MobileGoalStarter(label: "Synthesize", prompt: "Turn notes into a clear decision memo"),
        MobileGoalStarter(label: "Analyze", prompt: "Find the key patterns and recommend next steps"),
        MobileGoalStarter(label: "Create", prompt: "Draft a polished project brief from my outline"),
    ])
}

@Test
func stableMobileNoticesUseActionableCopy() {
    #expect(mobileNoticeMessage("runtime_unavailable") == "Runtime unavailable. Verified history is still shown.")
    #expect(mobileNoticeMessage("validation_input_too_large") == "Outcome is over 16 KiB. Shorten it before sending.")
}
