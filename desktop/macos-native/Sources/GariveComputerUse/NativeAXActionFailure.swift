/// Stable native failures for one snapshot-bound semantic action.
public enum NativeAXActionFailure: Error, Equatable, Sendable {
    /// Action shape or bounded value is invalid.
    case invalidAction
    /// Accessibility permission disappeared after observation.
    case permissionRevoked
    /// Signed process or exact window changed before dispatch.
    case targetChanged
    /// Current semantic projection differs from the frozen snapshot.
    case snapshotStale
    /// Snapshot-local node no longer maps to the same AX object.
    case nodeStale
    /// The node did not advertise the requested portable action.
    case actionUnsupported
    /// Native policy withholds actions on a protected value.
    case sensitiveActionRequired
    /// Dispatch began but trustworthy terminal evidence is unavailable.
    case actionUncertain
}
