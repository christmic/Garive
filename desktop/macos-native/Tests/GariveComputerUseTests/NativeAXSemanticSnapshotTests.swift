import Testing
@testable import GariveComputerUse

@Test("AX observation bounds reject zero and protocol-exceeding values")
func rejectsInvalidAXObservationBounds() {
    #expect(throws: NativeAXObservationFailure.invalidBounds) {
        try NativeAXObservationBounds(maxNodes: 0, maxTextBytes: 1)
    }
    #expect(throws: NativeAXObservationFailure.invalidBounds) {
        try NativeAXObservationBounds(maxNodes: 10_001, maxTextBytes: 1)
    }
    #expect(throws: NativeAXObservationFailure.invalidBounds) {
        try NativeAXObservationBounds(maxNodes: 1, maxTextBytes: 1_048_577)
    }
}

@Test("semantic projection redacts secure values and exposes portable actions")
func redactsSecureAXValues() throws {
    let root = NativeAXSemanticSnapshotBuilder.Element(
        role: "AXWindow",
        label: "Login",
        pressSupported: true,
        children: [
            .init(
                role: "AXTextField",
                subrole: "AXSecureTextField",
                label: "Password",
                value: "do-not-leak",
                enabled: true,
                focused: true,
                selected: false,
                pressSupported: false,
                valueSettable: true,
                frame: .init(x: 8, y: 12, width: 200, height: 22)
            ),
        ]
    )
    let bounds = try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 1_024)

    let snapshot = try NativeAXSemanticSnapshotBuilder.build(root: root, bounds: bounds)

    #expect(snapshot.nodes.count == 2)
    #expect(snapshot.nodes[1].valueSummary == nil)
    #expect(snapshot.nodes[1].valueRedacted)
    #expect(snapshot.nodes[0].supportedActions == [.press])
    #expect(snapshot.nodes[1].supportedActions.isEmpty)
    #expect(snapshot.focusedNodeIndex == 1)
}

@Test("semantic projection fails closed at node and text bounds")
func enforcesAXSemanticBounds() throws {
    let root = NativeAXSemanticSnapshotBuilder.Element(
        role: "AXWindow",
        label: "12345",
        children: [.init(role: "AXButton", label: "next")]
    )

    #expect(throws: NativeAXObservationFailure.resultBoundExceeded) {
        try NativeAXSemanticSnapshotBuilder.build(
            root: root,
            bounds: try NativeAXObservationBounds(maxNodes: 1, maxTextBytes: 100)
        )
    }
    #expect(throws: NativeAXObservationFailure.resultBoundExceeded) {
        try NativeAXSemanticSnapshotBuilder.build(
            root: root,
            bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 4)
        )
    }
}

@Test("semantic projection rejects cyclic native graphs")
func rejectsCyclicAXSemanticGraph() throws {
    let root = NativeAXSemanticSnapshotBuilder.Element(role: "AXWindow")
    root.children.append(root)

    #expect(throws: NativeAXObservationFailure.invalidNativeData) {
        try NativeAXSemanticSnapshotBuilder.build(
            root: root,
            bounds: try NativeAXObservationBounds(maxNodes: 10, maxTextBytes: 100)
        )
    }
}
