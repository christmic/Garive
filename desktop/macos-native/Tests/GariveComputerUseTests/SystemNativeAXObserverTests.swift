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
        semanticRoot: .init(role: "AXWindow", label: "Bound")
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

    let snapshot = try observer.observe(window: binding, bounds: bounds)
    #expect(snapshot.nodes.first?.label == "Bound")
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

private func makeAXApplicationIdentity() throws -> NativeApplicationInstanceIdentity {
    try NativeApplicationInstanceIdentity(
        processIdentifier: 321,
        processStartSeconds: 123,
        processStartMicroseconds: 456,
        signingIdentifier: "dev.garive.fixture",
        codeDirectoryHash: Data([1, 2, 3])
    )
}
