import Foundation
import GariveProcessProtocol
import Testing

@Test("exact lifecycle retains and acknowledges terminal evidence")
func exactLifecycle() throws {
    let identity = stateIdentity()
    var reducer = try ProcessStateReducer(identity: identity)
    try reducer.start(identity: identity)
    try reducer.markRunning(identity: identity)
    try reducer.retainTerminal(stateReceipt(identity: identity))
    let status = try reducer.query(identity: identity)
    #expect(status.state == .processServiceStateTerminalRetained)
    #expect(status.hasTerminal)
    try reducer.acknowledge(identity: identity, receiptDigest: status.terminal.receiptDigest)
    #expect(try reducer.query(identity: identity).state == .processServiceStateAbsent)
}

@Test("replay, mismatch, premature terminal, and wrong acknowledgement fail closed")
func lifecycleFailures() throws {
    let identity = stateIdentity()
    var other = identity; other.dispatchAttemptID = "other-attempt"
    var reducer = try ProcessStateReducer(identity: identity)
    #expect(throws: ProcessStateFailure.identityMismatch) { try reducer.query(identity: other) }
    #expect(throws: ProcessStateFailure.stateConflict) {
        try reducer.retainTerminal(stateReceipt(identity: identity))
    }
    try reducer.start(identity: identity)
    #expect(throws: ProcessStateFailure.stateConflict) { try reducer.start(identity: identity) }
    try reducer.markRunning(identity: identity)
    var invalid = stateReceipt(identity: identity); invalid.processTreeTerminated = false
    #expect(throws: ProcessStateFailure.invalidTerminal) { try reducer.retainTerminal(invalid) }
    try reducer.retainTerminal(stateReceipt(identity: identity))
    #expect(throws: ProcessStateFailure.identityMismatch) {
        try reducer.acknowledge(identity: identity, receiptDigest: Data(repeating: 9, count: 32))
    }
    #expect(throws: ProcessStateFailure.identityMismatch) { try reducer.terminate(identity: other) }
}

@Test("terminated start authority cannot be replayed")
func terminatedStartCannotReplay() throws {
    let identity = stateIdentity()
    var reducer = try ProcessStateReducer(identity: identity)
    try reducer.start(identity: identity)
    try reducer.terminate(identity: identity)
    try reducer.terminate(identity: identity)
    #expect(throws: ProcessStateFailure.stateConflict) { try reducer.start(identity: identity) }
}

private func stateIdentity() -> GRVProcessIdentityV1 {
    var value = GRVProcessIdentityV1()
    value.protocolRevision = "guest-v1.0"; value.invocationID = "inv-1"
    value.dispatchAttemptID = "attempt-1"; value.executorRevision = "exec-1"
    value.preparedDigest = Data(repeating: 1, count: 32)
    value.vmConfigurationDigest = Data(repeating: 2, count: 32)
    value.workloadDigest = Data(repeating: 3, count: 32)
    return value
}

private func stateReceipt(identity: GRVProcessIdentityV1) -> GRVProcessTerminalReceiptV1 {
    var exit = GRVProcessExitV1(); exit.code = 0
    var value = GRVProcessTerminalReceiptV1()
    value.identity = identity; value.exit = exit; value.stdout = Data("ok\n".utf8)
    value.processTreeTerminated = true
    return value
}
