import Foundation
import GariveNativeXPC

/// Admits one authenticated peer before exporting the closed process endpoint.
public final class ProcessServiceListenerDelegate: NSObject, NSXPCListenerDelegate {
    private let admissionPolicy: NativeXPCPeerAdmissionPolicy
    private let endpoint: ProcessServiceEndpoint

    public init(
        admissionPolicy: NativeXPCPeerAdmissionPolicy,
        endpoint: ProcessServiceEndpoint
    ) {
        self.admissionPolicy = admissionPolicy
        self.endpoint = endpoint
    }

    public func listener(
        _: NSXPCListener,
        shouldAcceptNewConnection connection: NSXPCConnection
    ) -> Bool {
        guard (try? admissionPolicy.validate(connection)) != nil else { return false }
        connection.exportedInterface = NSXPCInterface(with: ProcessServiceXPC.self)
        connection.exportedObject = endpoint
        connection.activate()
        return true
    }
}
