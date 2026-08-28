import Testing
@testable import GariveIOS
@Test func fakeHostCompletes() throws {
    #expect(FakeHost.usesSharedFramework)
    #expect(try FakeHost().run("hello") == "hello from Garive")
}
