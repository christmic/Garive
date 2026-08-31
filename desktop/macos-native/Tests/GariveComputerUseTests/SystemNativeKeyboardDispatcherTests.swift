import Darwin
import Testing
@testable import GariveComputerUse

@Test("system keyboard prepares every closed portable key", arguments: NativeKeyboardKey.allCases)
func preparesEveryNativePortableKey(_ key: NativeKeyboardKey) throws {
    _ = try SystemNativeKeyboardDispatcher().preparePressKey(
        key,
        processIdentifier: getpid()
    )
}

@Test("system keyboard prepares bounded Unicode without posting during preflight")
func preparesNativeUnicodeText() throws {
    _ = try SystemNativeKeyboardDispatcher().prepareTypeText(
        "原生🦀",
        processIdentifier: getpid()
    )
}
