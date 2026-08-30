import Foundation

final class NativeXPCAdmissionProbeService: NSObject, NativeXPCAdmissionProbe {
    func ping(reply: @escaping (String) -> Void) {
        reply("admitted")
    }
}
