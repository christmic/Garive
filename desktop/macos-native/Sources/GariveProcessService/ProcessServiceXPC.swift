import Foundation

/// The sole Objective-C surface exported by the process-isolation service.
@objc public protocol ProcessServiceXPC {
    /// Exchanges one canonical process-protocol frame without exposing native DTOs.
    func exchange(frame: Data, reply: @escaping (Data) -> Void)
}
