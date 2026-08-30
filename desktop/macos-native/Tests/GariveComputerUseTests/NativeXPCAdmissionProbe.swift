import Foundation

@objc protocol NativeXPCAdmissionProbe {
    func ping(reply: @escaping (String) -> Void)
}
