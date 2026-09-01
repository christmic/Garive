import Foundation
import SwiftProtobuf

/// Maximum protobuf payload admitted by one process-protocol frame.
public let processFrameMaximumPayloadBytes = 1_114_112

/// Closed framing failures that never expose payload or host paths.
public enum ProcessFrameFailure: Error, Equatable, Sendable {
    case malformed
    case boundsExceeded
}

/// Encodes one canonical four-byte big-endian length-prefixed frame.
public func encodeProcessFrame<M: SwiftProtobuf.Message>(_ message: M) throws -> Data {
    let payload = try message.serializedData()
    guard payload.count <= processFrameMaximumPayloadBytes,
          let length = UInt32(exactly: payload.count)
    else { throw ProcessFrameFailure.boundsExceeded }
    var bigEndianLength = length.bigEndian
    var frame = withUnsafeBytes(of: &bigEndianLength) { Data($0) }
    frame.append(payload)
    return frame
}

/// Strictly decodes one Runtime-to-XPC request frame.
public func decodeHostRequestFrame(_ frame: Data) throws -> GRVProcessHostRequestV1 {
    try decodeFrame(frame, admitsBody: validHostRequest)
}

/// Strictly decodes one XPC-to-Runtime response frame.
public func decodeHostResponseFrame(_ frame: Data) throws -> GRVProcessHostResponseV1 {
    try decodeFrame(frame, admitsBody: validHostResponse)
}

/// Strictly decodes one XPC-to-guest request frame.
public func decodeGuestRequestFrame(_ frame: Data) throws -> GRVProcessGuestRequestV1 {
    try decodeFrame(frame, admitsBody: validGuestRequest)
}

/// Strictly decodes one guest-to-XPC response frame.
public func decodeGuestResponseFrame(_ frame: Data) throws -> GRVProcessGuestResponseV1 {
    try decodeFrame(frame, admitsBody: validGuestResponse)
}

private func validHostRequest(_ value: GRVProcessHostRequestV1) -> Bool {
    switch value.command {
    case let .preflight(dispatch)?, let .start(dispatch)?:
        validDispatch(dispatch)
    case let .query(request)?, let .terminate(request)?:
        request.hasIdentity
    case let .acknowledge(request)?:
        request.hasIdentity && request.receiptDigest.count == 32
    case nil:
        false
    }
}

private func validHostResponse(_ value: GRVProcessHostResponseV1) -> Bool {
    switch value.result {
    case let .preflighted(result)?: result.hasIdentity
    case let .status(status)?: validStatus(status)
    case let .terminal(receipt)?: validTerminalShape(receipt)
    case let .error(error)?: validProtocolError(error)
    case nil: false
    }
}

private func validGuestRequest(_ value: GRVProcessGuestRequestV1) -> Bool {
    switch value.command {
    case let .hello(hello)?: hello.hasIdentity && hello.challenge.count == 32
    case let .execute(execute)?:
        execute.hasIdentity && execute.hasWorkload && validWorkspaceMode(execute.workload.workspaceMode)
    case let .terminate(request)?: request.hasIdentity
    case nil: false
    }
}

private func validGuestResponse(_ value: GRVProcessGuestResponseV1) -> Bool {
    switch value.result {
    case let .ready(ready)?:
        ready.hasIdentity && ready.challenge.count == 32 && !ready.guestAgentRevision.isEmpty
    case let .terminal(receipt)?: validTerminalShape(receipt)
    case let .error(error)?: validProtocolError(error)
    case nil: false
    }
}

private func validDispatch(_ value: GRVProcessDispatchV1) -> Bool {
    value.hasIdentity && value.hasVmPlan && value.hasWorkload
        && validWorkspaceMode(value.vmPlan.workspaceMode)
        && validWorkspaceMode(value.workload.workspaceMode)
}

private func validStatus(_ value: GRVProcessStatusV1) -> Bool {
    guard value.hasIdentity else { return false }
    return switch value.state {
    case .processServiceStateAbsent, .processServiceStateStarting, .processServiceStateRunning:
        !value.hasTerminal
    case .processServiceStateTerminalRetained:
        value.hasTerminal && validTerminalShape(value.terminal)
    case .processServiceStateUnspecified, .UNRECOGNIZED:
        false
    }
}

private func validTerminalShape(_ value: GRVProcessTerminalReceiptV1) -> Bool {
    value.hasIdentity && value.hasExit && value.exit.classification != nil
}

private func validProtocolError(_ value: GRVProcessProtocolErrorV1) -> Bool {
    switch value.failure {
    case .processProtocolFailureMalformed, .processProtocolFailureVersionMismatch,
         .processProtocolFailureBoundsExceeded, .processProtocolFailureIdentityMismatch,
         .processProtocolFailureStateConflict, .processProtocolFailureResourceUnavailable,
         .processProtocolFailureServiceUnavailable, .processProtocolFailureStateUnknown:
        true
    case .processProtocolFailureUnspecified, .UNRECOGNIZED:
        false
    }
}

private func validWorkspaceMode(_ value: GRVProcessWorkspaceModeV1) -> Bool {
    value == .processWorkspaceModeReadOnly || value == .processWorkspaceModeReadWrite
}

private func decodeFrame<M: SwiftProtobuf.Message>(
    _ frame: Data,
    admitsBody: (M) -> Bool
) throws -> M {
    guard frame.count >= 4 else { throw ProcessFrameFailure.malformed }
    let declared = frame.prefix(4).reduce(UInt32.zero) { ($0 << 8) | UInt32($1) }
    guard declared <= processFrameMaximumPayloadBytes else {
        throw ProcessFrameFailure.boundsExceeded
    }
    let payload = frame.dropFirst(4)
    guard payload.count == Int(declared) else { throw ProcessFrameFailure.malformed }
    do {
        var options = BinaryDecodingOptions()
        options.discardUnknownFields = true
        let message = try M(serializedBytes: payload, options: options)
        guard admitsBody(message), try message.serializedData() == payload else {
            throw ProcessFrameFailure.malformed
        }
        return message
    } catch let failure as ProcessFrameFailure {
        throw failure
    } catch {
        throw ProcessFrameFailure.malformed
    }
}
