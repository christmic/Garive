import ApplicationServices
import Foundation

/// Broker-private binding from one semantic snapshot to its exact AX objects.
public final class NativeAXObservationBinding {
    /// Provider-neutral bounded snapshot exposed to Runtime.
    public let snapshot: NativeAXSemanticSnapshot
    /// Number of snapshot-local native node bindings.
    public var nodeCount: Int { elements.count }

    let window: NativeAXWindowBinding
    let elements: [AXUIElement]
    let ownerIdentifier: ObjectIdentifier
    let bounds: NativeAXObservationBounds
    private let consumptionLock = NSLock()
    private var consumed = false

    init(
        snapshot: NativeAXSemanticSnapshot,
        window: NativeAXWindowBinding,
        elements: [AXUIElement],
        ownerIdentifier: ObjectIdentifier,
        bounds: NativeAXObservationBounds
    ) {
        self.snapshot = snapshot
        self.window = window
        self.elements = elements
        self.ownerIdentifier = ownerIdentifier
        self.bounds = bounds
    }

    func consume() -> Bool {
        consumptionLock.lock()
        defer { consumptionLock.unlock() }
        guard !consumed else { return false }
        consumed = true
        return true
    }
}
