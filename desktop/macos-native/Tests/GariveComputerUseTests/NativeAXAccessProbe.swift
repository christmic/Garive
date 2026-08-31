import ApplicationServices
@testable import GariveComputerUse

final class NativeAXAccessProbe: NativeAXAccessing {
    var windowElements: [AXUIElement]
    var semanticRoot: NativeAXSemanticSnapshotBuilder.Element
    var semanticElements: [AXUIElement]
    var focusedWindowElement: AXUIElement?
    var frontmostApplication = true
    private(set) var windowsCallCount = 0
    private(set) var semanticCallCount = 0
    private(set) var pressedElements: [AXUIElement] = []
    private(set) var setValues: [(String, AXUIElement)] = []

    init(
        windowElements: [AXUIElement],
        semanticRoot: NativeAXSemanticSnapshotBuilder.Element,
        semanticElements: [AXUIElement] = []
    ) {
        self.windowElements = windowElements
        self.semanticRoot = semanticRoot
        self.semanticElements = semanticElements
        focusedWindowElement = windowElements.first
    }

    func windows(processIdentifier _: Int32) throws -> [AXUIElement] {
        windowsCallCount += 1
        return windowElements
    }

    func focusedWindow(processIdentifier _: Int32) throws -> AXUIElement? {
        focusedWindowElement
    }

    func isFrontmostApplication(processIdentifier _: Int32) throws -> Bool {
        frontmostApplication
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

    func performPress(on element: AXUIElement) throws {
        pressedElements.append(element)
    }

    func setValue(_ value: String, on element: AXUIElement) throws {
        setValues.append((value, element))
    }
}
