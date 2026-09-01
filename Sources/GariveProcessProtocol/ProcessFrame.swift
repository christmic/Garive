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
    try decodeFrame(frame) { $0.command != nil }
}

/// Strictly decodes one XPC-to-Runtime response frame.
public func decodeHostResponseFrame(_ frame: Data) throws -> GRVProcessHostResponseV1 {
    try decodeFrame(frame) { $0.result != nil }
}

/// Strictly decodes one XPC-to-guest request frame.
public func decodeGuestRequestFrame(_ frame: Data) throws -> GRVProcessGuestRequestV1 {
    try decodeFrame(frame) { $0.command != nil }
}

/// Strictly decodes one guest-to-XPC response frame.
public func decodeGuestResponseFrame(_ frame: Data) throws -> GRVProcessGuestResponseV1 {
    try decodeFrame(frame) { $0.result != nil }
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
