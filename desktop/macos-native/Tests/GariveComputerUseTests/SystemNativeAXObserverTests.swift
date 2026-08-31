import ApplicationServices
import Foundation
import Testing
@testable import GariveComputerUse

@Test("AX permission denial happens before application or window inspection")
func deniesAXObservationBeforeInspection() throws {
    let access = NativeAXAccessProbe(
        windowElements: [],
        semanticRoot: .init(role: "AXWindow")
    )
    let observer = SystemNativeAXObserver(
        access: access,
        permissionState: { .required },
        isCurrent: { _ in true }
    )

    #expect(throws: NativeAXObservationFailure.permissionRequired) {
        try observer.bindWindows(applicationIdentity: makeAXApplicationIdentity())
    }
    #expect(access.windowsCallCount == 0)
    #expect(access.semanticCallCount == 0)
}

@Test("AX observation retains and revalidates the exact enumerated window")
func revalidatesExactAXWindow() throws {
    let nativeWindow = AXUIElementCreateApplication(7_001)
    let access = NativeAXAccessProbe(
        windowElements: [nativeWindow],
        semanticRoot: .init(role: "AXWindow", label: "Bound"),
        semanticElements: [nativeWindow]
    )
    let observer = SystemNativeAXObserver(
        access: access,
        permissionState: { .granted },
        isCurrent: { _ in true }
    )
    let identity = try makeAXApplicationIdentity()
    let binding = try #require(
        observer.bindWindows(applicationIdentity: identity).first
    )
    let bounds = try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)

    let observation = try observer.observe(window: binding, bounds: bounds)
    #expect(observation.snapshot.nodes.first?.label == "Bound")
    #expect(observation.nodeCount == 1)
    #expect(access.semanticCallCount == 1)

    access.windowElements = [AXUIElementCreateApplication(7_002)]
    #expect(throws: NativeAXObservationFailure.targetChanged) {
        try observer.observe(window: binding, bounds: bounds)
    }
    #expect(access.semanticCallCount == 1)
}

@Test("AX window bindings cannot cross observer ownership")
func rejectsForeignAXWindowBinding() throws {
    let nativeWindow = AXUIElementCreateApplication(7_003)
    let firstAccess = NativeAXAccessProbe(
        windowElements: [nativeWindow],
        semanticRoot: .init(role: "AXWindow")
    )
    let secondAccess = NativeAXAccessProbe(
        windowElements: [nativeWindow],
        semanticRoot: .init(role: "AXWindow")
    )
    let first = SystemNativeAXObserver(
        access: firstAccess,
        permissionState: { .granted },
        isCurrent: { _ in true }
    )
    let second = SystemNativeAXObserver(
        access: secondAccess,
        permissionState: { .granted },
        isCurrent: { _ in true }
    )
    let binding = try #require(
        first.bindWindows(applicationIdentity: makeAXApplicationIdentity()).first
    )

    #expect(throws: NativeAXObservationFailure.targetChanged) {
        try second.observe(
            window: binding,
            bounds: try NativeAXObservationBounds(maxNodes: 1, maxTextBytes: 0)
        )
    }
    #expect(secondAccess.windowsCallCount == 0)
}

@Test("AX press revalidates the frozen snapshot and exact node before dispatch")
func dispatchesBoundAXPress() throws {
    let window = AXUIElementCreateApplication(7_004)
    let button = AXUIElementCreateApplication(7_005)
    let root = NativeAXSemanticSnapshotBuilder.Element(
        role: "AXWindow",
        children: [.init(role: "AXButton", label: "Continue", pressSupported: true)]
    )
    let access = NativeAXAccessProbe(
        windowElements: [window],
        semanticRoot: root,
        semanticElements: [window, button]
    )
    let observer = makeGrantedAXObserver(access: access)
    let windowBinding = try #require(
        observer.bindWindows(applicationIdentity: makeAXApplicationIdentity()).first
    )
    let observation = try observer.observe(
        window: windowBinding,
        bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)
    )

    let resulting = try observer.perform(
        action: .press(nodeIndex: 1),
        observation: observation
    )

    #expect(access.pressedElements.count == 1)
    #expect(access.isSameElement(access.pressedElements[0], button))
    #expect(resulting.snapshot.nodes[1].label == "Continue")
}

@Test("AX action rejects a changed semantic snapshot before dispatch")
func rejectsStaleAXSemanticAction() throws {
    let window = AXUIElementCreateApplication(7_006)
    let button = AXUIElementCreateApplication(7_007)
    let access = NativeAXAccessProbe(
        windowElements: [window],
        semanticRoot: .init(
            role: "AXWindow",
            children: [.init(role: "AXButton", label: "Before", pressSupported: true)]
        ),
        semanticElements: [window, button]
    )
    let observer = makeGrantedAXObserver(access: access)
    let windowBinding = try #require(
        observer.bindWindows(applicationIdentity: makeAXApplicationIdentity()).first
    )
    let observation = try observer.observe(
        window: windowBinding,
        bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)
    )
    access.semanticRoot = .init(
        role: "AXWindow",
        children: [.init(role: "AXButton", label: "After", pressSupported: true)]
    )

    #expect(throws: NativeAXActionFailure.snapshotStale) {
        try observer.perform(action: .press(nodeIndex: 1), observation: observation)
    }
    #expect(access.pressedElements.isEmpty)
}

@Test("AX set-value never dispatches to a secure text node")
func rejectsSecureAXSetValue() throws {
    let window = AXUIElementCreateApplication(7_008)
    let secureField = AXUIElementCreateApplication(7_009)
    let access = NativeAXAccessProbe(
        windowElements: [window],
        semanticRoot: .init(
            role: "AXWindow",
            children: [
                .init(
                    role: "AXTextField",
                    subrole: "AXSecureTextField",
                    value: "hidden",
                    valueSettable: true
                ),
            ]
        ),
        semanticElements: [window, secureField]
    )
    let observer = makeGrantedAXObserver(access: access)
    let windowBinding = try #require(
        observer.bindWindows(applicationIdentity: makeAXApplicationIdentity()).first
    )
    let observation = try observer.observe(
        window: windowBinding,
        bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)
    )

    #expect(throws: NativeAXActionFailure.sensitiveActionRequired) {
        try observer.perform(
            action: .setValue(nodeIndex: 1, value: "replacement"),
            observation: observation
        )
    }
    #expect(access.setValues.isEmpty)
}

@Test("AX action permission revocation fails before fresh native inspection")
func rejectsRevokedAXActionBeforeInspection() throws {
    let window = AXUIElementCreateApplication(7_010)
    let button = AXUIElementCreateApplication(7_011)
    let access = NativeAXAccessProbe(
        windowElements: [window],
        semanticRoot: .init(
            role: "AXWindow",
            children: [.init(role: "AXButton", pressSupported: true)]
        ),
        semanticElements: [window, button]
    )
    var permission = NativePermissionState.granted
    let observer = SystemNativeAXObserver(
        access: access,
        permissionState: { permission },
        isCurrent: { _ in true }
    )
    let windowBinding = try #require(
        observer.bindWindows(applicationIdentity: makeAXApplicationIdentity()).first
    )
    let observation = try observer.observe(
        window: windowBinding,
        bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)
    )
    let windowCallsBeforeAction = access.windowsCallCount
    let semanticCallsBeforeAction = access.semanticCallCount
    permission = .required

    #expect(throws: NativeAXActionFailure.permissionRevoked) {
        try observer.perform(action: .press(nodeIndex: 1), observation: observation)
    }
    #expect(access.windowsCallCount == windowCallsBeforeAction)
    #expect(access.semanticCallCount == semanticCallsBeforeAction)
    #expect(access.pressedElements.isEmpty)
}

@Test("one AX observation binding dispatches at most once")
func consumesAXObservationOnce() throws {
    let window = AXUIElementCreateApplication(7_012)
    let button = AXUIElementCreateApplication(7_013)
    let access = NativeAXAccessProbe(
        windowElements: [window],
        semanticRoot: .init(
            role: "AXWindow",
            children: [.init(role: "AXButton", pressSupported: true)]
        ),
        semanticElements: [window, button]
    )
    let observer = makeGrantedAXObserver(access: access)
    let windowBinding = try #require(
        observer.bindWindows(applicationIdentity: makeAXApplicationIdentity()).first
    )
    let observation = try observer.observe(
        window: windowBinding,
        bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)
    )
    _ = try observer.perform(action: .press(nodeIndex: 1), observation: observation)

    #expect(throws: NativeAXActionFailure.snapshotStale) {
        try observer.perform(action: .press(nodeIndex: 1), observation: observation)
    }
    #expect(access.pressedElements.count == 1)
}

@Test("AX set-value enforces the Runtime scalar bound before reinspection")
func rejectsOversizedAXSetValue() throws {
    let window = AXUIElementCreateApplication(7_014)
    let field = AXUIElementCreateApplication(7_015)
    let access = NativeAXAccessProbe(
        windowElements: [window],
        semanticRoot: .init(
            role: "AXWindow",
            children: [.init(role: "AXTextField", valueSettable: true)]
        ),
        semanticElements: [window, field]
    )
    let observer = makeGrantedAXObserver(access: access)
    let windowBinding = try #require(
        observer.bindWindows(applicationIdentity: makeAXApplicationIdentity()).first
    )
    let observation = try observer.observe(
        window: windowBinding,
        bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)
    )
    let semanticCallsBeforeAction = access.semanticCallCount

    #expect(throws: NativeAXActionFailure.invalidAction) {
        try observer.perform(
            action: .setValue(
                nodeIndex: 1,
                value: String(repeating: "a", count: 32_769)
            ),
            observation: observation
        )
    }
    #expect(access.semanticCallCount == semanticCallsBeforeAction)
    #expect(access.setValues.isEmpty)
}

@Test("native Unicode typing binds the focused window and exact text node")
func dispatchesBoundNativeUnicodeText() throws {
    let window = AXUIElementCreateApplication(7_016)
    let field = AXUIElementCreateApplication(7_017)
    let access = NativeAXAccessProbe(
        windowElements: [window],
        semanticRoot: .init(
            role: "AXWindow",
            children: [
                .init(
                    role: "AXTextField",
                    value: "",
                    focused: true,
                    valueSettable: true
                ),
            ]
        ),
        semanticElements: [window, field]
    )
    let keyboard = NativeKeyboardDispatchProbe()
    let observer = SystemNativeAXObserver(
        access: access,
        keyboard: keyboard,
        permissionState: { .granted },
        isCurrent: { _ in true }
    )
    let windowBinding = try #require(
        observer.bindWindows(applicationIdentity: makeAXApplicationIdentity()).first
    )
    let observation = try observer.observe(
        window: windowBinding,
        bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)
    )
    #expect(observation.snapshot.nodes[1].supportedActions == [.setValue, .typeText])

    _ = try observer.perform(
        action: .typeText(nodeIndex: 1, text: "原生🦀"),
        observation: observation
    )

    #expect(keyboard.preparedText.count == 1)
    #expect(keyboard.preparedText[0].0 == "原生🦀")
    #expect(keyboard.preparedText[0].1 == 321)
    #expect(keyboard.dispatchedText.count == 1)
}

@Test("portable key input fails before preparation when the exact window loses focus")
func rejectsNativeKeyAfterWindowFocusLoss() throws {
    let window = AXUIElementCreateApplication(7_018)
    let field = AXUIElementCreateApplication(7_019)
    let access = NativeAXAccessProbe(
        windowElements: [window],
        semanticRoot: .init(
            role: "AXWindow",
            children: [.init(role: "AXTextField", focused: true, valueSettable: true)]
        ),
        semanticElements: [window, field]
    )
    let keyboard = NativeKeyboardDispatchProbe()
    let observer = SystemNativeAXObserver(
        access: access,
        keyboard: keyboard,
        permissionState: { .granted },
        isCurrent: { _ in true }
    )
    let windowBinding = try #require(
        observer.bindWindows(applicationIdentity: makeAXApplicationIdentity()).first
    )
    let observation = try observer.observe(
        window: windowBinding,
        bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)
    )
    access.frontmostApplication = false

    #expect(throws: NativeAXActionFailure.focusChanged) {
        try observer.perform(action: .pressKey(.enter), observation: observation)
    }
    access.frontmostApplication = true
    access.focusedWindowElement = AXUIElementCreateApplication(7_020)

    #expect(throws: NativeAXActionFailure.focusChanged) {
        try observer.perform(action: .pressKey(.enter), observation: observation)
    }
    #expect(keyboard.preparedKeys.isEmpty)
    #expect(keyboard.dispatchedKeys.isEmpty)
}

@Test("portable key input targets the admitted process after exact focus revalidation")
func dispatchesBoundNativePortableKey() throws {
    let window = AXUIElementCreateApplication(7_021)
    let field = AXUIElementCreateApplication(7_022)
    let access = NativeAXAccessProbe(
        windowElements: [window],
        semanticRoot: .init(
            role: "AXWindow",
            children: [.init(role: "AXTextField", focused: true, valueSettable: true)]
        ),
        semanticElements: [window, field]
    )
    let keyboard = NativeKeyboardDispatchProbe()
    let observer = SystemNativeAXObserver(
        access: access,
        keyboard: keyboard,
        permissionState: { .granted },
        isCurrent: { _ in true }
    )
    let windowBinding = try #require(
        observer.bindWindows(applicationIdentity: makeAXApplicationIdentity()).first
    )
    let observation = try observer.observe(
        window: windowBinding,
        bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)
    )

    _ = try observer.perform(action: .pressKey(.pageDown), observation: observation)

    #expect(keyboard.preparedKeys.count == 1)
    #expect(keyboard.preparedKeys[0].0 == .pageDown)
    #expect(keyboard.preparedKeys[0].1 == 321)
    #expect(keyboard.dispatchedKeys.count == 1)
}

@Test("portable key rechecks focus after event preparation and before posting")
func rejectsNativeKeyFocusRaceBeforePosting() throws {
    let window = AXUIElementCreateApplication(7_023)
    let field = AXUIElementCreateApplication(7_024)
    let access = NativeAXAccessProbe(
        windowElements: [window],
        semanticRoot: .init(
            role: "AXWindow",
            children: [.init(role: "AXTextField", focused: true, valueSettable: true)]
        ),
        semanticElements: [window, field]
    )
    let keyboard = NativeKeyboardDispatchProbe()
    keyboard.keyPreparationHook = { access.frontmostApplication = false }
    let observer = SystemNativeAXObserver(
        access: access,
        keyboard: keyboard,
        permissionState: { .granted },
        isCurrent: { _ in true }
    )
    let windowBinding = try #require(
        observer.bindWindows(applicationIdentity: makeAXApplicationIdentity()).first
    )
    let observation = try observer.observe(
        window: windowBinding,
        bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)
    )

    #expect(throws: NativeAXActionFailure.focusChanged) {
        try observer.perform(action: .pressKey(.space), observation: observation)
    }
    #expect(keyboard.preparedKeys.count == 1)
    #expect(keyboard.dispatchedKeys.isEmpty)
}

private func makeGrantedAXObserver(
    access: NativeAXAccessProbe
) -> SystemNativeAXObserver {
    SystemNativeAXObserver(
        access: access,
        permissionState: { .granted },
        isCurrent: { _ in true }
    )
}

private func makeAXApplicationIdentity() throws -> NativeApplicationInstanceIdentity {
    try NativeApplicationInstanceIdentity(
        processIdentifier: 321,
        processStartSeconds: 123,
        processStartMicroseconds: 456,
        signingIdentifier: "dev.garive.fixture",
        codeDirectoryHash: Data([1, 2, 3])
    )
}
