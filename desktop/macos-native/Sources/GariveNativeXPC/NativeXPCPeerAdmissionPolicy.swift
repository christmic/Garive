import Foundation
import Security

private let maximumCodeSigningRequirementBytes = 4_096
private let invalidConfiguredAuditSessionIdentifier: Int32 = -1

/// Explicit listener-level code-signing and login-session admission policy.
public struct NativeXPCPeerAdmissionPolicy: Sendable {
    /// Exact code-signing requirement installed into the XPC listener.
    public let codeSigningRequirement: String
    /// Required effective user identity.
    public let effectiveUserIdentifier: UInt32
    /// Required login audit session.
    public let auditSessionIdentifier: Int32

    /// Validates all configured admission material without starting a listener.
    public init(
        codeSigningRequirement: String,
        effectiveUserIdentifier: UInt32,
        auditSessionIdentifier: Int32
    ) throws {
        let trimmed = codeSigningRequirement.trimmingCharacters(in: .whitespacesAndNewlines)
        guard auditSessionIdentifier != invalidConfiguredAuditSessionIdentifier else {
            throw NativeXPCAdmissionFailure.invalidPeerIdentity
        }
        guard !trimmed.isEmpty,
              trimmed != "always",
              !codeSigningRequirement.contains("\0"),
              codeSigningRequirement.utf8.count <= maximumCodeSigningRequirementBytes,
              Self.isValidRequirement(codeSigningRequirement)
        else {
            throw NativeXPCAdmissionFailure.invalidCodeSigningRequirement
        }
        self.codeSigningRequirement = codeSigningRequirement
        self.effectiveUserIdentifier = effectiveUserIdentifier
        self.auditSessionIdentifier = auditSessionIdentifier
    }

    /// Installs the signature requirement before the listener is activated.
    public func configure(_ listener: NSXPCListener) {
        listener.setConnectionCodeSigningRequirement(codeSigningRequirement)
    }

    /// Revalidates public peer scope after XPC has admitted its signature.
    public func validate(_ facts: NativeXPCPeerFacts) throws {
        guard facts.effectiveUserIdentifier == effectiveUserIdentifier else {
            throw NativeXPCAdmissionFailure.userMismatch
        }
        guard facts.auditSessionIdentifier == auditSessionIdentifier else {
            throw NativeXPCAdmissionFailure.auditSessionMismatch
        }
    }

    /// Extracts and validates authenticated facts from a new XPC connection.
    public func validate(_ connection: NSXPCConnection) throws -> NativeXPCPeerFacts {
        let facts = try NativeXPCPeerFacts(
            processIdentifier: connection.processIdentifier,
            effectiveUserIdentifier: connection.effectiveUserIdentifier,
            auditSessionIdentifier: connection.auditSessionIdentifier
        )
        try validate(facts)
        return facts
    }

    private static func isValidRequirement(_ value: String) -> Bool {
        var requirement: SecRequirement?
        return SecRequirementCreateWithString(
            value as CFString,
            SecCSFlags(),
            &requirement
        ) == errSecSuccess && requirement != nil
    }
}
