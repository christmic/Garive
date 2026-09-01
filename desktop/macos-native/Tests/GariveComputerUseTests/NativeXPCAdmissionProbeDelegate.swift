import Foundation
@testable import GariveNativeXPC

final class NativeXPCAdmissionProbeDelegate: NSObject, NSXPCListenerDelegate {
    private let policy: NativeXPCPeerAdmissionPolicy
    private let service = NativeXPCAdmissionProbeService()

    init(policy: NativeXPCPeerAdmissionPolicy) {
        self.policy = policy
    }

    func listener(_: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        guard (try? policy.validate(connection)) != nil else {
            return false
        }
        connection.exportedInterface = NSXPCInterface(with: NativeXPCAdmissionProbe.self)
        connection.exportedObject = service
        connection.activate()
        return true
    }
}
