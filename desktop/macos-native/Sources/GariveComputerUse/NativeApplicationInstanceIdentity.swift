import Foundation

private let maximumSigningIdentifierBytes = 1_024
private let maximumCodeDirectoryHashBytes = 128
private let microsecondsPerSecond: UInt64 = 1_000_000

/// Broker-private identity for one exact running signed application instance.
public struct NativeApplicationInstanceIdentity: Equatable, Sendable {
    /// Kernel process identifier, never sufficient as authority by itself.
    public let processIdentifier: Int32
    /// Process start time seconds reported by `proc_pidinfo`.
    public let processStartSeconds: UInt64
    /// Process start time microsecond remainder reported by `proc_pidinfo`.
    public let processStartMicroseconds: UInt64
    /// Validated code-signing identifier.
    public let signingIdentifier: String
    /// Validated CodeDirectory hash bytes for the exact running code.
    public let codeDirectoryHash: Data

    /// Constructs one strictly bounded process/code identity.
    public init(
        processIdentifier: Int32,
        processStartSeconds: UInt64,
        processStartMicroseconds: UInt64,
        signingIdentifier: String,
        codeDirectoryHash: Data
    ) throws {
        guard processIdentifier > 0,
              processStartSeconds > 0,
              processStartMicroseconds < microsecondsPerSecond,
              !signingIdentifier.isEmpty,
              signingIdentifier.utf8.count <= maximumSigningIdentifierBytes,
              !codeDirectoryHash.isEmpty,
              codeDirectoryHash.count <= maximumCodeDirectoryHashBytes
        else {
            throw NativeApplicationIdentityFailure.invalidIdentity
        }
        self.processIdentifier = processIdentifier
        self.processStartSeconds = processStartSeconds
        self.processStartMicroseconds = processStartMicroseconds
        self.signingIdentifier = signingIdentifier
        self.codeDirectoryHash = codeDirectoryHash
    }
}
