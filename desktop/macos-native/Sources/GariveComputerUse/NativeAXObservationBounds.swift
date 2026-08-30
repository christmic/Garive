private let maximumAXObservationNodes = 10_000
private let maximumAXObservationTextBytes = 1_048_576

/// Explicit resource limits for one prompt-free Accessibility observation.
public struct NativeAXObservationBounds: Equatable, Sendable {
    /// Maximum number of flat semantic nodes returned.
    public let maxNodes: Int
    /// Maximum combined UTF-8 bytes across returned semantic text.
    public let maxTextBytes: Int

    /// Creates bounds within the accepted Computer Use protocol ceiling.
    public init(maxNodes: Int, maxTextBytes: Int) throws {
        guard (1 ... maximumAXObservationNodes).contains(maxNodes),
              (0 ... maximumAXObservationTextBytes).contains(maxTextBytes)
        else {
            throw NativeAXObservationFailure.invalidBounds
        }
        self.maxNodes = maxNodes
        self.maxTextBytes = maxTextBytes
    }
}
