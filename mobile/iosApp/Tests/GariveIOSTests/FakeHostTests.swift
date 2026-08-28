import Testing
@testable import GariveIOS
@Test func fakeHostCompletes() throws { #expect(try FakeHost().run("hello") == "hello from Garive") }
