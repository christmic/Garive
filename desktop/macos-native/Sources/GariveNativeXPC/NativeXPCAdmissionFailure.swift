/// Stable fail-closed outcomes from native XPC peer admission.
public enum NativeXPCAdmissionFailure: String, Error, Equatable, Sendable {
    /// The configured requirement is empty, broad, oversized, or invalid syntax.
    case invalidCodeSigningRequirement
    /// The peer facts cannot identify one live process instance.
    case invalidPeerIdentity
    /// The caller is outside the configured effective-user boundary.
    case userMismatch
    /// The caller is outside the configured login audit session.
    case auditSessionMismatch
}
