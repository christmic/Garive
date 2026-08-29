import Testing
@testable import GariveIOS

@Test
func sharedLiveHostFrameworkIsLinked() {
    #expect(MobileHostRunner.usesSharedFramework)
}

@Test
func nonLoopbackConfigurationFailsBeforeTransport() async {
    await confirmation { confirmed in
        MobileHostRunner().run(
            hostURL: "https://example.com/", definitionID: "definition", message: "private"
        ) { result in
            if case .failure = result { confirmed() }
        }
    }
}
