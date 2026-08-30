import ApplicationServices
import CoreGraphics

/// Stable permission state returned without triggering a system prompt.
public enum NativePermissionState: String, Sendable {
    /// The operating-system capability is currently available.
    case granted
    /// The user must grant the capability through an explained product flow.
    case required
}

/// Side-effect-free permission posture for the two Computer Use observation surfaces.
public struct NativePermissionPosture: Equatable, Sendable {
    /// Accessibility trust required for semantic observation and actions.
    public let accessibility: NativePermissionState
    /// Screen Recording trust required only when an admitted capture is requested.
    public let screenCapture: NativePermissionState

    /// Maps native trust booleans without requesting either permission.
    public init(accessibilityTrusted: Bool, screenCaptureTrusted: Bool) {
        accessibility = accessibilityTrusted ? .granted : .required
        screenCapture = screenCaptureTrusted ? .granted : .required
    }
}

/// Reads current macOS trust state without displaying system authorization UI.
public struct SystemNativePermissionInspector: Sendable {
    /// Creates the stateless native inspector.
    public init() {}

    /// Returns the current Accessibility and Screen Recording posture.
    public func inspect() -> NativePermissionPosture {
        NativePermissionPosture(
            accessibilityTrusted: AXIsProcessTrusted(),
            screenCaptureTrusted: CGPreflightScreenCaptureAccess()
        )
    }
}
