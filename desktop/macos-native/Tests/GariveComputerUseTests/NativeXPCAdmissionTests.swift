import Darwin
import Foundation
import Security
import Testing
@testable import GariveComputerUse

private let exactRequirement = #"identifier "com.garive.desktop" and anchor apple generic"#

@Test("XPC admission binds exact user and audit session")
func admitsOnlyTheExactPeerScope() throws {
    let policy = try NativeXPCPeerAdmissionPolicy(
        codeSigningRequirement: exactRequirement,
        effectiveUserIdentifier: 501,
        auditSessionIdentifier: 77
    )
    let admitted = try NativeXPCPeerFacts(
        processIdentifier: 42,
        effectiveUserIdentifier: 501,
        auditSessionIdentifier: 77
    )
    try policy.validate(admitted)

    let wrongUser = try NativeXPCPeerFacts(
        processIdentifier: 42,
        effectiveUserIdentifier: 502,
        auditSessionIdentifier: 77
    )
    #expect(throws: NativeXPCAdmissionFailure.userMismatch) {
        try policy.validate(wrongUser)
    }
    let wrongSession = try NativeXPCPeerFacts(
        processIdentifier: 42,
        effectiveUserIdentifier: 501,
        auditSessionIdentifier: 78
    )
    #expect(throws: NativeXPCAdmissionFailure.auditSessionMismatch) {
        try policy.validate(wrongSession)
    }
}

@Test("XPC admission rejects invalid identities and broad or malformed requirements")
func rejectsInvalidAdmissionMaterial() {
    #expect(throws: NativeXPCAdmissionFailure.invalidPeerIdentity) {
        try NativeXPCPeerFacts(
            processIdentifier: 0,
            effectiveUserIdentifier: 501,
            auditSessionIdentifier: 77
        )
    }
    #expect(throws: NativeXPCAdmissionFailure.invalidPeerIdentity) {
        try NativeXPCPeerFacts(
            processIdentifier: 42,
            effectiveUserIdentifier: 501,
            auditSessionIdentifier: -1
        )
    }
    #expect(throws: NativeXPCAdmissionFailure.invalidCodeSigningRequirement) {
        try NativeXPCPeerAdmissionPolicy(
            codeSigningRequirement: "always",
            effectiveUserIdentifier: 501,
            auditSessionIdentifier: 77
        )
    }
    #expect(throws: NativeXPCAdmissionFailure.invalidCodeSigningRequirement) {
        try NativeXPCPeerAdmissionPolicy(
            codeSigningRequirement: "identifier ???",
            effectiveUserIdentifier: 501,
            auditSessionIdentifier: 77
        )
    }
    #expect(throws: NativeXPCAdmissionFailure.invalidPeerIdentity) {
        try NativeXPCPeerAdmissionPolicy(
            codeSigningRequirement: exactRequirement,
            effectiveUserIdentifier: 501,
            auditSessionIdentifier: -1
        )
    }
}

@Test("the native listener enforces the current process designated requirement")
func nativeListenerAdmitsAnExactlySignedPeer() async throws {
    let policy = try NativeXPCPeerAdmissionPolicy(
        codeSigningRequirement: try currentTestDesignatedRequirement(),
        effectiveUserIdentifier: geteuid(),
        auditSessionIdentifier: try currentAuditSessionIdentifier()
    )
    let listener = NSXPCListener.anonymous()
    let delegate = NativeXPCAdmissionProbeDelegate(policy: policy)
    listener.delegate = delegate
    policy.configure(listener)
    listener.activate()
    defer { listener.invalidate() }

    let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
    connection.remoteObjectInterface = NSXPCInterface(with: NativeXPCAdmissionProbe.self)
    connection.activate()
    defer { connection.invalidate() }
    let response = await withCheckedContinuation { continuation in
        let proxy = connection.remoteObjectProxy as? NativeXPCAdmissionProbe
        proxy?.ping { continuation.resume(returning: $0) }
    }
    #expect(response == "admitted")
}

func currentTestDesignatedRequirement() throws -> String {
    var code: SecCode?
    guard SecCodeCopySelf(SecCSFlags(), &code) == errSecSuccess, let code else {
        throw NativeXPCAdmissionFailure.invalidCodeSigningRequirement
    }
    var staticCode: SecStaticCode?
    guard SecCodeCopyStaticCode(code, SecCSFlags(), &staticCode) == errSecSuccess,
          let staticCode
    else {
        throw NativeXPCAdmissionFailure.invalidCodeSigningRequirement
    }
    var requirement: SecRequirement?
    guard SecCodeCopyDesignatedRequirement(staticCode, SecCSFlags(), &requirement) == errSecSuccess,
          let requirement
    else {
        throw NativeXPCAdmissionFailure.invalidCodeSigningRequirement
    }
    var text: CFString?
    guard SecRequirementCopyString(requirement, SecCSFlags(), &text) == errSecSuccess,
          let text
    else {
        throw NativeXPCAdmissionFailure.invalidCodeSigningRequirement
    }
    return text as String
}

private func currentAuditSessionIdentifier() throws -> Int32 {
    var info = auditinfo_addr()
    guard getaudit_addr(&info, Int32(MemoryLayout<auditinfo_addr>.size)) == 0 else {
        throw NativeXPCAdmissionFailure.invalidPeerIdentity
    }
    return info.ai_asid
}
