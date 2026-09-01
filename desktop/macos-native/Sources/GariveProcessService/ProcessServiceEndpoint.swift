import Foundation
import GariveProcessProtocol

/// Closed V0-C1 endpoint that admits framing but dispatches no workload.
public final class ProcessServiceEndpoint: NSObject, ProcessServiceXPC {
    private let malformedFrame: Data
    private let boundsExceededFrame: Data
    private let serviceUnavailableFrame: Data

    private init(
        malformedFrame: Data,
        boundsExceededFrame: Data,
        serviceUnavailableFrame: Data
    ) {
        self.malformedFrame = malformedFrame
        self.boundsExceededFrame = boundsExceededFrame
        self.serviceUnavailableFrame = serviceUnavailableFrame
        super.init()
    }

    /// Precomputes every bounded response before a listener can activate.
    public static func validated() throws -> ProcessServiceEndpoint {
        try ProcessServiceEndpoint(
            malformedFrame: failureFrame(.processProtocolFailureMalformed),
            boundsExceededFrame: failureFrame(.processProtocolFailureBoundsExceeded),
            serviceUnavailableFrame: failureFrame(.processProtocolFailureServiceUnavailable)
        )
    }

    public func exchange(frame: Data, reply: @escaping (Data) -> Void) {
        do {
            _ = try decodeHostRequestFrame(frame)
            reply(serviceUnavailableFrame)
        } catch ProcessFrameFailure.boundsExceeded {
            reply(boundsExceededFrame)
        } catch {
            reply(malformedFrame)
        }
    }

    private static func failureFrame(_ failure: GRVProcessProtocolFailureV1) throws -> Data {
        var error = GRVProcessProtocolErrorV1()
        error.failure = failure
        var response = GRVProcessHostResponseV1()
        response.error = error
        return try encodeProcessFrame(response)
    }
}
