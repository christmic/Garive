/// One provider-neutral node in a parent-before-child Accessibility snapshot.
public struct NativeAXSemanticNode: Equatable, Sendable {
    /// Portable semantic mutations supported by this node.
    public enum SupportedAction: String, Equatable, Sendable {
        /// Invoke the node's native press action.
        case press
        /// Replace the node's native value.
        case setValue = "set_value"
    }

    /// Sensitivity attached to an exposed or redacted value.
    public enum ValueSensitivity: String, Equatable, Sendable {
        /// Ordinary application value subject to Runtime policy.
        case ordinary
        /// Credential-like value that the native boundary must not expose.
        case protected
    }

    /// Screen-space geometry reported by Accessibility.
    public struct Frame: Equatable, Sendable {
        /// Horizontal screen coordinate.
        public let x: Double
        /// Vertical screen coordinate.
        public let y: Double
        /// Non-negative width.
        public let width: Double
        /// Non-negative height.
        public let height: Double

        /// Creates immutable screen-space geometry.
        public init(x: Double, y: Double, width: Double, height: Double) {
            self.x = x
            self.y = y
            self.width = width
            self.height = height
        }
    }

    /// Snapshot-local index assigned parent before child.
    public let nodeIndex: Int
    /// Parent index, absent only for the root window.
    public let parentIndex: Int?
    /// Native semantic role.
    public let role: String
    /// Native semantic subrole when present.
    public let subrole: String?
    /// Human-readable label when present.
    public let label: String?
    /// Bounded scalar value summary, never a secure-field value.
    public let valueSummary: String?
    /// Sensitivity of the original value when one was present or protected.
    public let valueSensitivity: ValueSensitivity?
    /// Whether the native boundary removed the original value.
    public let valueRedacted: Bool
    /// Native enabled state when available.
    public let enabled: Bool?
    /// Native focused state when available.
    public let focused: Bool?
    /// Native selected state when available.
    public let selected: Bool?
    /// Closed portable action set supported by this node.
    public let supportedActions: [SupportedAction]
    /// Screen-space geometry when complete and valid.
    public let frame: Frame?
}
