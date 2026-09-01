import Foundation
import GariveProcessProtocol
import Testing

@Test("all four protocol directions round-trip canonical frames")
func protocolDirectionsRoundTrip() throws {
    var hostRequest = GRVProcessHostRequestV1()
    var identityRequest = GRVProcessIdentityRequestV1()
    identityRequest.identity = GRVProcessIdentityV1()
    hostRequest.query = identityRequest
    var hostResponse = GRVProcessHostResponseV1()
    var protocolError = GRVProcessProtocolErrorV1()
    protocolError.failure = .processProtocolFailureMalformed
    hostResponse.error = protocolError
    var guestRequest = GRVProcessGuestRequestV1()
    guestRequest.terminate = identityRequest
    var guestResponse = GRVProcessGuestResponseV1()
    guestResponse.error = protocolError

    #expect(try decodeHostRequestFrame(encodeProcessFrame(hostRequest)) == hostRequest)
    #expect(try decodeHostResponseFrame(encodeProcessFrame(hostResponse)) == hostResponse)
    #expect(try decodeGuestRequestFrame(encodeProcessFrame(guestRequest)) == guestRequest)
    #expect(try decodeGuestResponseFrame(encodeProcessFrame(guestResponse)) == guestResponse)
}

@Test("malformed lengths, unknown fields, duplicates, and absent bodies fail closed")
func malformedFramesFailClosed() throws {
    #expect(throws: ProcessFrameFailure.malformed) { try decodeHostRequestFrame(Data()) }
    #expect(throws: ProcessFrameFailure.malformed) {
        try decodeHostRequestFrame(Data([0, 0, 0, 1]))
    }
    let oversized = UInt32(processFrameMaximumPayloadBytes + 1)
    #expect(throws: ProcessFrameFailure.boundsExceeded) {
        try decodeHostRequestFrame(Data(oversized.bigEndianBytes))
    }
    var request = GRVProcessHostRequestV1()
    var identityRequest = GRVProcessIdentityRequestV1()
    identityRequest.identity = GRVProcessIdentityV1()
    request.query = identityRequest
    let payload = try encodeProcessFrame(request).dropFirst(4)
    #expect(throws: ProcessFrameFailure.malformed) {
        try decodeHostRequestFrame(framed(with: Data(payload) + Data([0x98, 0x06, 0x01])))
    }
    #expect(throws: ProcessFrameFailure.malformed) {
        try decodeHostRequestFrame(framed(with: Data(payload) + Data(payload)))
    }
    #expect(throws: ProcessFrameFailure.malformed) {
        try decodeHostRequestFrame(encodeProcessFrame(GRVProcessHostRequestV1()))
    }
}

@Test("a frame cannot cross a protocol direction")
func protocolDirectionsAreDisjoint() throws {
    var host = GRVProcessHostRequestV1()
    var identityRequest = GRVProcessIdentityRequestV1()
    identityRequest.identity = GRVProcessIdentityV1()
    host.query = identityRequest
    var guest = GRVProcessGuestRequestV1()
    guest.terminate = identityRequest
    #expect(throws: ProcessFrameFailure.malformed) {
        try decodeGuestRequestFrame(encodeProcessFrame(host))
    }
    #expect(throws: ProcessFrameFailure.malformed) {
        try decodeHostRequestFrame(encodeProcessFrame(guest))
    }
}

@Test("unknown and unspecified protocol enums fail closed")
func unknownEnumsFailClosed() throws {
    var error = GRVProcessProtocolErrorV1()
    error.failure = .UNRECOGNIZED(99)
    var response = GRVProcessHostResponseV1(); response.error = error
    #expect(throws: ProcessFrameFailure.malformed) {
        try decodeHostResponseFrame(encodeProcessFrame(response))
    }
    error.failure = .processProtocolFailureUnspecified; response.error = error
    #expect(throws: ProcessFrameFailure.malformed) {
        try decodeHostResponseFrame(encodeProcessFrame(response))
    }
}

private func framed(with payload: Data) -> Data {
    Data(UInt32(payload.count).bigEndianBytes) + payload
}

private extension UInt32 {
    var bigEndianBytes: [UInt8] {
        let value = bigEndian
        return withUnsafeBytes(of: value) { Array($0) }
    }
}
