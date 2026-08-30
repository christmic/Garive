/// Stable failures produced before any native Accessibility mutation.
public enum NativeAXObservationFailure: Error, Equatable, Sendable {
    /// Caller supplied bounds outside the accepted protocol limits.
    case invalidBounds
    /// Accessibility permission is absent before native inspection begins.
    case permissionRequired
    /// The admitted process or exact window no longer matches its binding.
    case targetChanged
    /// Accessibility returned malformed or ambiguous semantic state.
    case invalidNativeData
    /// The semantic result exceeded an explicit caller bound.
    case resultBoundExceeded
}
