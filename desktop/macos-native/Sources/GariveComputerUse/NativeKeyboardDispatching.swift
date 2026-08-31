import Carbon.HIToolbox
import CoreGraphics

protocol NativeKeyboardDispatching: AnyObject {
    func prepareTypeText(_ text: String, processIdentifier: Int32) throws -> () -> Void
    func preparePressKey(
        _ key: NativeKeyboardKey,
        processIdentifier: Int32
    ) throws -> () -> Void
}

final class SystemNativeKeyboardDispatcher: NativeKeyboardDispatching {
    func prepareTypeText(
        _ text: String,
        processIdentifier: Int32
    ) throws -> () -> Void {
        let source = CGEventSource(stateID: .privateState)
        guard let keyDown = CGEvent(
            keyboardEventSource: source,
            virtualKey: 0,
            keyDown: true
        ), let keyUp = CGEvent(
            keyboardEventSource: source,
            virtualKey: 0,
            keyDown: false
        ) else {
            throw NativeAXActionFailure.actionUnsupported
        }
        let units = Array(text.utf16)
        units.withUnsafeBufferPointer { buffer in
            keyDown.keyboardSetUnicodeString(
                stringLength: buffer.count,
                unicodeString: buffer.baseAddress
            )
        }
        return {
            keyDown.postToPid(processIdentifier)
            keyUp.postToPid(processIdentifier)
        }
    }

    func preparePressKey(
        _ key: NativeKeyboardKey,
        processIdentifier: Int32
    ) throws -> () -> Void {
        let source = CGEventSource(stateID: .privateState)
        let code = keyCode(key)
        guard let keyDown = CGEvent(
            keyboardEventSource: source,
            virtualKey: code,
            keyDown: true
        ), let keyUp = CGEvent(
            keyboardEventSource: source,
            virtualKey: code,
            keyDown: false
        ) else {
            throw NativeAXActionFailure.actionUnsupported
        }
        return {
            keyDown.postToPid(processIdentifier)
            keyUp.postToPid(processIdentifier)
        }
    }

    private func keyCode(_ key: NativeKeyboardKey) -> CGKeyCode {
        let code: Int
        switch key {
        case .enter: code = kVK_Return
        case .tab: code = kVK_Tab
        case .escape: code = kVK_Escape
        case .backspace: code = kVK_Delete
        case .delete: code = kVK_ForwardDelete
        case .arrowUp: code = kVK_UpArrow
        case .arrowDown: code = kVK_DownArrow
        case .arrowLeft: code = kVK_LeftArrow
        case .arrowRight: code = kVK_RightArrow
        case .home: code = kVK_Home
        case .end: code = kVK_End
        case .pageUp: code = kVK_PageUp
        case .pageDown: code = kVK_PageDown
        case .space: code = kVK_Space
        }
        return CGKeyCode(code)
    }
}
