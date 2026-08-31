import ApplicationServices

protocol NativeAXAccessing: AnyObject {
    func windows(processIdentifier: Int32) throws -> [AXUIElement]
    func focusedWindow(processIdentifier: Int32) throws -> AXUIElement?
    func isFrontmostApplication(processIdentifier: Int32) throws -> Bool
    func isSameElement(_ left: AXUIElement, _ right: AXUIElement) -> Bool
    func semanticElement(
        root: AXUIElement,
        bounds: NativeAXObservationBounds
    ) throws -> NativeAXReadResult
    func performPress(on element: AXUIElement) throws
    func setValue(_ value: String, on element: AXUIElement) throws
}
