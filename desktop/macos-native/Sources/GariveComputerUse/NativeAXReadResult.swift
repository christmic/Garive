import ApplicationServices

struct NativeAXReadResult {
    let root: NativeAXSemanticSnapshotBuilder.Element
    let elements: [AXUIElement]
}
