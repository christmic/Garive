import ApplicationServices
@testable import GariveComputerUse

final class NativeAXAccessProbe: NativeAXAccessing {
    var windowElements: [AXUIElement]
    var semanticRoot: NativeAXSemanticSnapshotBuilder.Element
    var semanticElements: [AXUIElement]
    private(set) var windowsCallCount = 0
    private(set) var semanticCallCount = 0

    init(
        windowElements: [AXUIElement],
        semanticRoot: NativeAXSemanticSnapshotBuilder.Element,
        semanticElements: [AXUIElement] = []
    ) {
        self.windowElements = windowElements
        self.semanticRoot = semanticRoot
        self.semanticElements = semanticElements
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
    ) throws -> NativeAXReadResult {
        semanticCallCount += 1
        return NativeAXReadResult(root: semanticRoot, elements: semanticElements)
    }
}
