import Foundation
import GariveProcessProtocol
import Testing

@Test("all four protocol directions round-trip canonical frames")
func protocolDirectionsRoundTrip() throws {
    var hostRequest = GRVProcessHostRequestV1()
    hostRequest.query = GRVProcessIdentityRequestV1()
    var hostResponse = GRVProcessHostResponseV1()
    hostResponse.error = GRVProcessProtocolErrorV1()
    var guestRequest = GRVProcessGuestRequestV1()
    guestRequest.terminate = GRVProcessIdentityRequestV1()
    var guestResponse = GRVProcessGuestResponseV1()
    guestResponse.error = GRVProcessProtocolErrorV1()

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
    request.query = GRVProcessIdentityRequestV1()
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
    host.query = GRVProcessIdentityRequestV1()
    var guest = GRVProcessGuestRequestV1()
    guest.terminate = GRVProcessIdentityRequestV1()
    #expect(throws: ProcessFrameFailure.malformed) {
        try decodeGuestRequestFrame(encodeProcessFrame(host))
    }
    #expect(throws: ProcessFrameFailure.malformed) {
        try decodeHostRequestFrame(encodeProcessFrame(guest))
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
