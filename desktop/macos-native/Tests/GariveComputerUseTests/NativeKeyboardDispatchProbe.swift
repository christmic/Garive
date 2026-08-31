@testable import GariveComputerUse

final class NativeKeyboardDispatchProbe: NativeKeyboardDispatching {
    private(set) var preparedText: [(String, Int32)] = []
    private(set) var dispatchedText: [(String, Int32)] = []
    private(set) var preparedKeys: [(NativeKeyboardKey, Int32)] = []
    private(set) var dispatchedKeys: [(NativeKeyboardKey, Int32)] = []
    var keyPreparationHook: (() -> Void)?

    func prepareTypeText(
        _ text: String,
        processIdentifier: Int32
    ) throws -> () -> Void {
        preparedText.append((text, processIdentifier))
        return { [self] in dispatchedText.append((text, processIdentifier)) }
    }

    func preparePressKey(
        _ key: NativeKeyboardKey,
        processIdentifier: Int32
    ) throws -> () -> Void {
        preparedKeys.append((key, processIdentifier))
        keyPreparationHook?()
        return { [self] in dispatchedKeys.append((key, processIdentifier)) }
    }
}
