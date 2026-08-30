import Darwin
import Foundation
import Testing
@testable import GariveComputerUse

@Test("application identity binds dynamic code and one process start")
func bindsTheCurrentApplicationInstance() throws {
    let verifier = try NativeApplicationIdentityVerifier(
        codeSigningRequirement: try currentTestDesignatedRequirement()
    )
    let identity = try verifier.resolve(processIdentifier: getpid())

    #expect(identity.processIdentifier == getpid())
    #expect(!identity.signingIdentifier.isEmpty)
    #expect(!identity.codeDirectoryHash.isEmpty)
    #expect(try verifier.isCurrent(identity))

    let stale = try NativeApplicationInstanceIdentity(
        processIdentifier: identity.processIdentifier,
        processStartSeconds: identity.processStartSeconds + 1,
        processStartMicroseconds: identity.processStartMicroseconds,
        signingIdentifier: identity.signingIdentifier,
        codeDirectoryHash: identity.codeDirectoryHash
    )
    #expect(try !verifier.isCurrent(stale))
}

@Test("application identity rejects missing targets and a wrong signer")
func rejectsMissingOrWrongApplicationIdentity() throws {
    let verifier = try NativeApplicationIdentityVerifier(
        codeSigningRequirement: try currentTestDesignatedRequirement()
    )
    #expect(throws: NativeApplicationIdentityFailure.targetUnavailable) {
        try verifier.resolve(processIdentifier: Int32.max)
    }

    let wrongSigner = try NativeApplicationIdentityVerifier(
        codeSigningRequirement: #"identifier "not.garive.test""#
    )
    #expect(throws: NativeApplicationIdentityFailure.signatureRejected) {
        try wrongSigner.resolve(processIdentifier: getpid())
    }
}

@Test("application identity material is strictly bounded")
func rejectsInvalidApplicationIdentityMaterial() {
    #expect(throws: NativeApplicationIdentityFailure.invalidRequirement) {
        try NativeApplicationIdentityVerifier(codeSigningRequirement: "always")
    }
    #expect(throws: NativeApplicationIdentityFailure.invalidIdentity) {
        try NativeApplicationInstanceIdentity(
            processIdentifier: 0,
            processStartSeconds: 1,
            processStartMicroseconds: 0,
            signingIdentifier: "com.garive.test",
            codeDirectoryHash: Data([1])
        )
    }
    #expect(throws: NativeApplicationIdentityFailure.invalidIdentity) {
        try NativeApplicationInstanceIdentity(
            processIdentifier: 42,
            processStartSeconds: 1,
            processStartMicroseconds: 1_000_000,
            signingIdentifier: "com.garive.test",
            codeDirectoryHash: Data([1])
        )
    }
}
