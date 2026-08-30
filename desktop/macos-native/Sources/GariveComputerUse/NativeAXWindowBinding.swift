import ApplicationServices

/// Broker-private binding to one exact Accessibility window object.
public final class NativeAXWindowBinding {
    /// Admitted signed process instance that owned the window when bound.
    public let applicationIdentity: NativeApplicationInstanceIdentity

    let element: AXUIElement
    let ownerIdentifier: ObjectIdentifier

    init(
        applicationIdentity: NativeApplicationInstanceIdentity,
        element: AXUIElement,
        ownerIdentifier: ObjectIdentifier
    ) {
        self.applicationIdentity = applicationIdentity
        self.element = element
        self.ownerIdentifier = ownerIdentifier
    }
}
