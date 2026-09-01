import Darwin
import Foundation
import GariveNativeXPC

/// Closed failures raised before the service listener can activate.
public enum ProcessServiceBootstrapFailure: Error, Equatable, Sendable {
    case invalidBundleIdentifier
    case missingCodeSigningRequirement
    case unavailableAuditSession
}

/// Signed package metadata and authenticated service scope.
public struct ProcessServiceBootstrapConfiguration: Sendable {
    public static let bundleIdentifier = "com.garive.desktop.process-isolation-service"
    public static let backendCodeSigningRequirementKey =
        "GariveBackendCodeSigningRequirement"

    public let admissionPolicy: NativeXPCPeerAdmissionPolicy

    /// Validates explicit values without consulting ambient configuration.
    public init(
        bundleIdentifier: String?,
        backendCodeSigningRequirement: String?,
        effectiveUserIdentifier: UInt32,
        auditSessionIdentifier: Int32
    ) throws {
        guard bundleIdentifier == Self.bundleIdentifier else {
            throw ProcessServiceBootstrapFailure.invalidBundleIdentifier
        }
        guard let backendCodeSigningRequirement else {
            throw ProcessServiceBootstrapFailure.missingCodeSigningRequirement
        }
        admissionPolicy = try NativeXPCPeerAdmissionPolicy(
            codeSigningRequirement: backendCodeSigningRequirement,
            effectiveUserIdentifier: effectiveUserIdentifier,
            auditSessionIdentifier: auditSessionIdentifier
        )
    }

    /// Reads only code-signed bundle metadata and kernel-authenticated identities.
    public static func current(bundle: Bundle) throws -> Self {
        var audit = auditinfo_addr()
        guard getaudit_addr(&audit, Int32(MemoryLayout<auditinfo_addr>.size)) == 0 else {
            throw ProcessServiceBootstrapFailure.unavailableAuditSession
        }
        return try Self(
            bundleIdentifier: bundle.bundleIdentifier,
            backendCodeSigningRequirement: bundle.object(
                forInfoDictionaryKey: backendCodeSigningRequirementKey
            ) as? String,
            effectiveUserIdentifier: geteuid(),
            auditSessionIdentifier: audit.ai_asid
        )
    }
}
