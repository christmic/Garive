/// Fail-closed outcomes from native application-instance verification.
public enum NativeApplicationIdentityFailure: String, Error, Equatable, Sendable {
    /// The configured code-signing requirement is unsafe or malformed.
    case invalidRequirement
    /// The requested process is absent or cannot be inspected.
    case targetUnavailable
    /// The running code does not satisfy the configured identity requirement.
    case signatureRejected
    /// Required signed or process-instance evidence is missing or malformed.
    case invalidIdentity
    /// The process instance changed while identity evidence was collected.
    case processChanged
}
