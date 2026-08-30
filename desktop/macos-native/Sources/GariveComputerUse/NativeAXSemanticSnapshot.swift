/// Bounded semantic result for one exact native window observation.
public struct NativeAXSemanticSnapshot: Equatable, Sendable {
    /// Flat parent-before-child tree.
    public let nodes: [NativeAXSemanticNode]
    /// Unique focused node index when Accessibility identifies one.
    public let focusedNodeIndex: Int?
    /// Combined UTF-8 bytes charged against the observation bound.
    public let textBytes: Int
    /// Number of secure values removed at the native boundary.
    public let redactedValueCount: Int
}
