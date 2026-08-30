import Testing
@testable import GariveComputerUse

@Test("permission booleans map to stable prompt-free states", arguments: [
    (true, true, NativePermissionState.granted, NativePermissionState.granted),
    (true, false, NativePermissionState.granted, NativePermissionState.required),
    (false, true, NativePermissionState.required, NativePermissionState.granted),
    (false, false, NativePermissionState.required, NativePermissionState.required),
])
func mapsPermissionPosture(
    accessibilityTrusted: Bool,
    screenCaptureTrusted: Bool,
    expectedAccessibility: NativePermissionState,
    expectedScreenCapture: NativePermissionState
) {
    let posture = NativePermissionPosture(
        accessibilityTrusted: accessibilityTrusted,
        screenCaptureTrusted: screenCaptureTrusted
    )
    #expect(posture.accessibility == expectedAccessibility)
    #expect(posture.screenCapture == expectedScreenCapture)
}
