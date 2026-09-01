import Darwin
import Foundation
import GariveNativeXPC
import GariveProcessProtocol
import Security
import Testing
@testable import GariveProcessService

@Test("the admitted XPC endpoint returns one canonical unavailable response")
func canonicalXPCFrameRoundTrip() async throws {
    let policy = try NativeXPCPeerAdmissionPolicy(
        codeSigningRequirement: try currentDesignatedRequirement(),
        effectiveUserIdentifier: geteuid(),
        auditSessionIdentifier: try currentAuditSession()
    )
    let listener = NSXPCListener.anonymous()
    let delegate = ProcessServiceListenerDelegate(
        admissionPolicy: policy,
        endpoint: try ProcessServiceEndpoint.validated()
    )
    listener.delegate = delegate
    policy.configure(listener)
    listener.activate()
    defer { listener.invalidate() }

    let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
    connection.remoteObjectInterface = NSXPCInterface(with: ProcessServiceXPC.self)
    connection.activate()
    defer { connection.invalidate() }

    let requestFrame = try validQueryFrame()
    let responseFrame = await withCheckedContinuation { continuation in
        let proxy = connection.remoteObjectProxy as? ProcessServiceXPC
        proxy?.exchange(frame: requestFrame) { continuation.resume(returning: $0) }
    }
    let response = try decodeHostResponseFrame(responseFrame)
    #expect(response.error.failure == .processProtocolFailureServiceUnavailable)
}

@Test("the closed endpoint classifies malformed and oversized frames")
func closedFrameFailures() async throws {
    let endpoint = try ProcessServiceEndpoint.validated()
    let malformed = await exchange(Data([0, 0, 0, 1, 0xFF]), with: endpoint)
    #expect(
        try decodeHostResponseFrame(malformed).error.failure
            == .processProtocolFailureMalformed
    )
    let oversized = await exchange(Data([0, 0x11, 0, 1]), with: endpoint)
    #expect(
        try decodeHostResponseFrame(oversized).error.failure
            == .processProtocolFailureBoundsExceeded
    )
}

@Test("the C1 target contains no execution or ambient configuration source")
func closedSourcePolicy() throws {
    let package = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent()
    let sources = ["GariveProcessService", "GariveProcessIsolationService"].map {
        package.appendingPathComponent("Sources/\($0)")
    }
    let files = sources.flatMap {
        (FileManager.default.enumerator(at: $0, includingPropertiesForKeys: nil)?
            .allObjects as? [URL]) ?? []
    }
    let text = try files.filter { $0.pathExtension == "swift" }
        .map { try String(contentsOf: $0, encoding: .utf8) }.joined()
    for forbidden in [
        "ProcessInfo.processInfo.environment", "getenv(", "Process()", "VZVirtualMachine(",
        "FileHandle", "FileManager", "/bin/sh", "os_log", "Logger(", "print(",
    ] {
        #expect(!text.contains(forbidden), "forbidden source: \(forbidden)")
    }
}

@Test("service bootstrap accepts only the exact signed package metadata")
func exactBootstrapMetadata() throws {
    let requirement = #"identifier "com.garive.desktop" and anchor apple generic"#
    let value = try ProcessServiceBootstrapConfiguration(
        bundleIdentifier: ProcessServiceBootstrapConfiguration.bundleIdentifier,
        backendCodeSigningRequirement: requirement,
        effectiveUserIdentifier: 501,
        auditSessionIdentifier: 77
    )
    #expect(value.admissionPolicy.codeSigningRequirement == requirement)
    #expect(throws: ProcessServiceBootstrapFailure.invalidBundleIdentifier) {
        try ProcessServiceBootstrapConfiguration(
            bundleIdentifier: "com.garive.wrong",
            backendCodeSigningRequirement: requirement,
            effectiveUserIdentifier: 501,
            auditSessionIdentifier: 77
        )
    }
    #expect(throws: ProcessServiceBootstrapFailure.missingCodeSigningRequirement) {
        try ProcessServiceBootstrapConfiguration(
            bundleIdentifier: ProcessServiceBootstrapConfiguration.bundleIdentifier,
            backendCodeSigningRequirement: nil,
            effectiveUserIdentifier: 501,
            auditSessionIdentifier: 77
        )
    }
}

private func validQueryFrame() throws -> Data {
    var identityRequest = GRVProcessIdentityRequestV1()
    identityRequest.identity = GRVProcessIdentityV1()
    var request = GRVProcessHostRequestV1()
    request.query = identityRequest
    return try encodeProcessFrame(request)
}

private func exchange(_ frame: Data, with endpoint: ProcessServiceEndpoint) async -> Data {
    await withCheckedContinuation { continuation in
        endpoint.exchange(frame: frame) { continuation.resume(returning: $0) }
    }
}

private func currentDesignatedRequirement() throws -> String {
    var code: SecCode?
    guard SecCodeCopySelf(SecCSFlags(), &code) == errSecSuccess, let code else {
        throw NativeXPCAdmissionFailure.invalidCodeSigningRequirement
    }
    var staticCode: SecStaticCode?
    guard SecCodeCopyStaticCode(code, SecCSFlags(), &staticCode) == errSecSuccess,
          let staticCode
    else { throw NativeXPCAdmissionFailure.invalidCodeSigningRequirement }
    var requirement: SecRequirement?
    guard SecCodeCopyDesignatedRequirement(staticCode, SecCSFlags(), &requirement) == errSecSuccess,
          let requirement
    else { throw NativeXPCAdmissionFailure.invalidCodeSigningRequirement }
    var text: CFString?
    guard SecRequirementCopyString(requirement, SecCSFlags(), &text) == errSecSuccess,
          let text
    else { throw NativeXPCAdmissionFailure.invalidCodeSigningRequirement }
    return text as String
}

private func currentAuditSession() throws -> Int32 {
    var info = auditinfo_addr()
    guard getaudit_addr(&info, Int32(MemoryLayout<auditinfo_addr>.size)) == 0 else {
        throw NativeXPCAdmissionFailure.invalidPeerIdentity
    }
    return info.ai_asid
}
