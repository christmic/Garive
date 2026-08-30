import ApplicationServices
@testable import GariveComputerUse

final class NativeAXAccessProbe: NativeAXAccessing {
    var windowElements: [AXUIElement]
    var semanticRoot: NativeAXSemanticSnapshotBuilder.Element
    private(set) var windowsCallCount = 0
    private(set) var semanticCallCount = 0

    init(
        windowElements: [AXUIElement],
        semanticRoot: NativeAXSemanticSnapshotBuilder.Element
    ) {
        self.windowElements = windowElements
        self.semanticRoot = semanticRoot
    }

    func windows(processIdentifier _: Int32) throws -> [AXUIElement] {
        windowsCallCount += 1
        return windowElements
    }

    func isSameElement(_ left: AXUIElement, _ right: AXUIElement) -> Bool {
        CFEqual(left, right)
    }

    func semanticElement(
        root _: AXUIElement,
        bounds _: NativeAXObservationBounds
    ) throws -> NativeAXSemanticSnapshotBuilder.Element {
        semanticCallCount += 1
        return semanticRoot
    }
}
