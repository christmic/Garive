import Darwin
import Foundation
import Security

private let maximumApplicationRequirementBytes = 4_096

/// Resolves and revalidates one dynamic signed process without name discovery.
public struct NativeApplicationIdentityVerifier: Sendable {
    /// Exact admitted Security requirement.
    public let codeSigningRequirement: String

    /// Validates the explicit requirement before any process inspection.
    public init(codeSigningRequirement: String) throws {
        let trimmed = codeSigningRequirement.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty,
              trimmed != "always",
              !codeSigningRequirement.contains("\0"),
              codeSigningRequirement.utf8.count <= maximumApplicationRequirementBytes,
              Self.makeRequirement(codeSigningRequirement) != nil
        else {
            throw NativeApplicationIdentityFailure.invalidRequirement
        }
        self.codeSigningRequirement = codeSigningRequirement
    }

    /// Resolves a PID only when process-start and signed-code evidence remain stable.
    public func resolve(processIdentifier: Int32) throws -> NativeApplicationInstanceIdentity {
        let before = try Self.processStart(processIdentifier)
        guard let requirement = Self.makeRequirement(codeSigningRequirement) else {
            throw NativeApplicationIdentityFailure.invalidRequirement
        }
        var code: SecCode?
        let attributes = [
            kSecGuestAttributePid as String: NSNumber(value: processIdentifier)
        ] as CFDictionary
        guard SecCodeCopyGuestWithAttributes(nil, attributes, SecCSFlags(), &code) == errSecSuccess,
              let code
        else {
            throw NativeApplicationIdentityFailure.targetUnavailable
        }
        guard SecCodeCheckValidity(code, SecCSFlags(), requirement) == errSecSuccess else {
            throw NativeApplicationIdentityFailure.signatureRejected
        }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(code, SecCSFlags(), &staticCode) == errSecSuccess,
              let staticCode
        else {
            throw NativeApplicationIdentityFailure.invalidIdentity
        }
        var information: CFDictionary?
        guard SecCodeCopySigningInformation(
            staticCode,
            SecCSFlags(),
            &information
        ) == errSecSuccess,
            let dictionary = information as NSDictionary?,
            let signingIdentifier = dictionary[kSecCodeInfoIdentifier] as? String,
            let codeDirectoryHash = dictionary[kSecCodeInfoUnique] as? Data
        else {
            throw NativeApplicationIdentityFailure.invalidIdentity
        }
        guard SecCodeCheckValidity(code, SecCSFlags(), requirement) == errSecSuccess else {
            throw NativeApplicationIdentityFailure.signatureRejected
        }
        let after = try Self.processStart(processIdentifier)
        guard before == after else {
            throw NativeApplicationIdentityFailure.processChanged
        }
        return try NativeApplicationInstanceIdentity(
            processIdentifier: processIdentifier,
            processStartSeconds: before.seconds,
            processStartMicroseconds: before.microseconds,
            signingIdentifier: signingIdentifier,
            codeDirectoryHash: codeDirectoryHash
        )
    }

    /// Re-resolves native evidence and compares it with the frozen instance identity.
    public func isCurrent(_ identity: NativeApplicationInstanceIdentity) throws -> Bool {
        do {
            return try resolve(processIdentifier: identity.processIdentifier) == identity
        } catch NativeApplicationIdentityFailure.targetUnavailable,
                NativeApplicationIdentityFailure.processChanged {
            return false
        }
    }

    private static func makeRequirement(_ value: String) -> SecRequirement? {
        var requirement: SecRequirement?
        guard SecRequirementCreateWithString(
            value as CFString,
            SecCSFlags(),
            &requirement
        ) == errSecSuccess else {
            return nil
        }
        return requirement
    }

    private static func processStart(_ processIdentifier: Int32) throws -> ProcessStart {
        guard processIdentifier > 0 else {
            throw NativeApplicationIdentityFailure.targetUnavailable
        }
        var info = proc_bsdinfo()
        let expectedSize = MemoryLayout<proc_bsdinfo>.size
        let copied = withUnsafeMutablePointer(to: &info) {
            proc_pidinfo(
                processIdentifier,
                PROC_PIDTBSDINFO,
                0,
                $0,
                Int32(expectedSize)
            )
        }
        guard copied == Int32(expectedSize),
              info.pbi_pid == UInt32(processIdentifier),
              info.pbi_start_tvsec > 0,
              info.pbi_start_tvusec < 1_000_000
        else {
            throw NativeApplicationIdentityFailure.targetUnavailable
        }
        return ProcessStart(
            seconds: info.pbi_start_tvsec,
            microseconds: info.pbi_start_tvusec
        )
    }

    private struct ProcessStart: Equatable {
        let seconds: UInt64
        let microseconds: UInt64
    }
}
