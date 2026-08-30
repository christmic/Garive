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
