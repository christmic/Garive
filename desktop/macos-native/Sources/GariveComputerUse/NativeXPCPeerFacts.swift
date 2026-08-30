private let invalidPeerAuditSessionIdentifier: Int32 = -1

/// Public security facts supplied by one accepted `NSXPCConnection`.
public struct NativeXPCPeerFacts: Equatable, Sendable {
    /// Kernel process identifier for diagnostic correlation only.
    public let processIdentifier: Int32
    /// Effective user identity authenticated by XPC.
    public let effectiveUserIdentifier: UInt32
    /// Login audit-session identity authenticated by XPC.
    public let auditSessionIdentifier: Int32

    /// Constructs one bounded peer identity.
    public init(
        processIdentifier: Int32,
        effectiveUserIdentifier: UInt32,
        auditSessionIdentifier: Int32
    ) throws {
        guard processIdentifier > 0,
              auditSessionIdentifier != invalidPeerAuditSessionIdentifier
        else {
            throw NativeXPCAdmissionFailure.invalidPeerIdentity
        }
        self.processIdentifier = processIdentifier
        self.effectiveUserIdentifier = effectiveUserIdentifier
        self.auditSessionIdentifier = auditSessionIdentifier
    }
}
