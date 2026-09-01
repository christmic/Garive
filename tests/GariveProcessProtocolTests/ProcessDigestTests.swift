import Foundation
import GariveProcessProtocol
import Testing

@Test("Swift workload digest matches the Rust vector")
func workloadDigestVector() throws {
    #expect(try processWorkloadDigest(identity: identity(), workload: workload()).hex
        == "570a130149d364aacd0929f6e6605a46005fc28e28b486a16fcdcbafe000c13d")
}

@Test("every canonical workload input changes or invalidates its digest")
func everyWorkloadInputIsBound() throws {
    let baseIdentity = identity(), baseWorkload = workload()
    let base = try processWorkloadDigest(identity: baseIdentity, workload: baseWorkload)
    var identityVariants: [GRVProcessIdentityV1] = []
    func identityVariant(_ change: (inout GRVProcessIdentityV1) -> Void) {
        var value = baseIdentity; change(&value); identityVariants.append(value)
    }
    identityVariant { $0.protocolRevision = "guest-v1.1" }
    identityVariant { $0.invocationID = "inv-2" }
    identityVariant { $0.dispatchAttemptID = "attempt-2" }
    identityVariant { $0.executorRevision = "exec-2" }
    identityVariant { $0.preparedDigest[0] = 9 }
    identityVariant { $0.vmConfigurationDigest[0] = 9 }
    for value in identityVariants {
        #expect(try processWorkloadDigest(identity: value, workload: baseWorkload) != base)
    }
    var workloadVariants: [GRVProcessWorkloadV1] = []
    func workloadVariant(_ change: (inout GRVProcessWorkloadV1) -> Void) {
        var value = baseWorkload; change(&value); workloadVariants.append(value)
    }
    workloadVariant { $0.lane = "test" }
    workloadVariant { $0.executable = "/usr/bin/env" }
    workloadVariant { $0.argv.swapAt(0, 1) }
    workloadVariant { $0.workingDirectory = "other" }
    workloadVariant { $0.environment[0].value = "C" }
    workloadVariant { $0.maxOutputBytes -= 1 }
    workloadVariant { $0.timeoutMilliseconds -= 1 }
    workloadVariant { $0.maxProcesses -= 1 }
    workloadVariant { $0.maxOpenFiles -= 1 }
    workloadVariant { $0.workspaceMode = .processWorkspaceModeReadWrite }
    for value in workloadVariants {
        #expect(try processWorkloadDigest(identity: baseIdentity, workload: value) != base)
    }
}

@Test("workload digest rejects identity, ordering, bounds, and paths")
func workloadDigestRejectsInvalidInputs() throws {
    var badIdentity = identity()
    badIdentity.preparedDigest.removeLast()
    #expect(throws: ProcessDigestFailure.invalidIdentity) {
        try processWorkloadDigest(identity: badIdentity, workload: workload())
    }
    var bad = workload()
    bad.environment = [environment("Z", "1"), environment("A", "2")]
    #expect(throws: ProcessDigestFailure.invalidWorkload) {
        try processWorkloadDigest(identity: identity(), workload: bad)
    }
    bad = workload(); bad.workingDirectory = "../escape"
    #expect(throws: ProcessDigestFailure.invalidWorkload) {
        try processWorkloadDigest(identity: identity(), workload: bad)
    }
    bad = workload(); bad.argv = [String(repeating: "x", count: 16_385)]
    #expect(throws: ProcessDigestFailure.invalidWorkload) {
        try processWorkloadDigest(identity: identity(), workload: bad)
    }
}

@Test("Swift receipt digest matches Rust and requires terminal proof")
func receiptDigestVector() throws {
    var receiptIdentity = identity()
    receiptIdentity.workloadDigest = try processWorkloadDigest(identity: receiptIdentity, workload: workload())
    var exit = GRVProcessExitV1(); exit.code = 0
    var receipt = GRVProcessTerminalReceiptV1()
    receipt.identity = receiptIdentity; receipt.exit = exit
    receipt.stdout = Data("ok\n".utf8); receipt.processTreeTerminated = true
    #expect(try processReceiptDigest(receipt).hex
        == "23860e0a2c08e0f3c05157b3c85c4b8f53c3b54a9ddef81ff0bd130337abcb84")
    receipt.processTreeTerminated = false
    #expect(throws: ProcessDigestFailure.invalidReceipt) { try processReceiptDigest(receipt) }
    receipt.processTreeTerminated = true; receipt.exit.timedOut = false
    #expect(throws: ProcessDigestFailure.invalidReceipt) { try processReceiptDigest(receipt) }
}

private func identity() -> GRVProcessIdentityV1 {
    var value = GRVProcessIdentityV1()
    value.protocolRevision = "guest-v1.0"; value.invocationID = "inv-1"
    value.dispatchAttemptID = "attempt-1"; value.executorRevision = "exec-1"
    value.preparedDigest = Data(repeating: 1, count: 32)
    value.vmConfigurationDigest = Data(repeating: 2, count: 32)
    return value
}

private func workload() -> GRVProcessWorkloadV1 {
    var value = GRVProcessWorkloadV1()
    value.lane = "build"; value.executable = "/usr/bin/swift"
    value.argv = ["swift", "test"]; value.workingDirectory = "project"
    value.environment = [environment("LANG", "C.UTF-8")]
    value.maxOutputBytes = 1_048_576; value.timeoutMilliseconds = 300_000
    value.maxProcesses = 64; value.maxOpenFiles = 256
    value.workspaceMode = .processWorkspaceModeReadOnly
    return value
}

private func environment(_ key: String, _ value: String) -> GRVProcessEnvironmentEntryV1 {
    var entry = GRVProcessEnvironmentEntryV1(); entry.key = key; entry.value = value
    return entry
}

private extension Data {
    var hex: String { map { String(format: "%02x", $0) }.joined() }
}
