/// Prompt-free native Accessibility window enumeration and observation.
public final class SystemNativeAXObserver {
    private let access: any NativeAXAccessing
    private let permissionState: () -> NativePermissionState
    private let isCurrent: (NativeApplicationInstanceIdentity) throws -> Bool

    /// Creates an observer bound to one explicit application signing policy.
    public init(applicationVerifier: NativeApplicationIdentityVerifier) {
        access = SystemNativeAXAccess()
        permissionState = {
            SystemNativePermissionInspector().inspect().accessibility
        }
        isCurrent = applicationVerifier.isCurrent
    }

    init(
        access: any NativeAXAccessing,
        permissionState: @escaping () -> NativePermissionState,
        isCurrent: @escaping (NativeApplicationInstanceIdentity) throws -> Bool
    ) {
        self.access = access
        self.permissionState = permissionState
        self.isCurrent = isCurrent
    }

    /// Enumerates exact windows only for the still-current admitted process.
    public func bindWindows(
        applicationIdentity: NativeApplicationInstanceIdentity
    ) throws -> [NativeAXWindowBinding] {
        try requireAccess(to: applicationIdentity)
        let elements = try access.windows(
            processIdentifier: applicationIdentity.processIdentifier
        )
        try requireCurrent(applicationIdentity)
        let ownerIdentifier = ObjectIdentifier(access)
        return elements.map {
            NativeAXWindowBinding(
                applicationIdentity: applicationIdentity,
                element: $0,
                ownerIdentifier: ownerIdentifier
            )
        }
    }

    /// Reads one bounded semantic tree only while its exact window remains bound.
    public func observe(
        window: NativeAXWindowBinding,
        bounds: NativeAXObservationBounds
    ) throws -> NativeAXSemanticSnapshot {
        guard window.ownerIdentifier == ObjectIdentifier(access) else {
            throw NativeAXObservationFailure.targetChanged
        }
        try requireAccess(to: window.applicationIdentity)
        try requireWindow(window)
        let root = try access.semanticElement(root: window.element, bounds: bounds)
        let snapshot = try NativeAXSemanticSnapshotBuilder.build(root: root, bounds: bounds)
        try requireCurrent(window.applicationIdentity)
        try requireWindow(window)
        return snapshot
    }

    private func requireAccess(
        to identity: NativeApplicationInstanceIdentity
    ) throws {
        guard permissionState() == .granted else {
            throw NativeAXObservationFailure.permissionRequired
        }
        try requireCurrent(identity)
    }

    private func requireCurrent(
        _ identity: NativeApplicationInstanceIdentity
    ) throws {
        guard try isCurrent(identity) else {
            throw NativeAXObservationFailure.targetChanged
        }
    }

    private func requireWindow(_ binding: NativeAXWindowBinding) throws {
        let current = try access.windows(
            processIdentifier: binding.applicationIdentity.processIdentifier
        )
        guard current.contains(where: {
            access.isSameElement($0, binding.element)
        }) else {
            throw NativeAXObservationFailure.targetChanged
        }
    }
}
