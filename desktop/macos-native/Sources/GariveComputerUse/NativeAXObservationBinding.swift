import ApplicationServices

/// Broker-private binding from one semantic snapshot to its exact AX objects.
public final class NativeAXObservationBinding {
    /// Provider-neutral bounded snapshot exposed to Runtime.
    public let snapshot: NativeAXSemanticSnapshot
    /// Number of snapshot-local native node bindings.
    public var nodeCount: Int { elements.count }

    let window: NativeAXWindowBinding
    let elements: [AXUIElement]
    let ownerIdentifier: ObjectIdentifier

    init(
        snapshot: NativeAXSemanticSnapshot,
        window: NativeAXWindowBinding,
        elements: [AXUIElement],
        ownerIdentifier: ObjectIdentifier
    ) {
        self.snapshot = snapshot
        self.window = window
        self.elements = elements
        self.ownerIdentifier = ownerIdentifier
    }
}
