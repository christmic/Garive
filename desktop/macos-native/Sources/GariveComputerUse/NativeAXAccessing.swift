import ApplicationServices

protocol NativeAXAccessing: AnyObject {
    func windows(processIdentifier: Int32) throws -> [AXUIElement]
    func isSameElement(_ left: AXUIElement, _ right: AXUIElement) -> Bool
    func semanticElement(
        root: AXUIElement,
        bounds: NativeAXObservationBounds
    ) throws -> NativeAXSemanticSnapshotBuilder.Element
}
