/// Closed native semantic action set; coordinate input is a separate boundary.
public enum NativeAXSemanticAction: Equatable, Sendable {
    /// Invoke the snapshot-local node's press action.
    case press(nodeIndex: Int)
    /// Replace one snapshot-local non-secure node value.
    case setValue(nodeIndex: Int, value: String)
    /// Insert bounded Unicode through native keyboard events into one focused text node.
    case typeText(nodeIndex: Int, text: String)
    /// Send one closed portable key to the uniquely focused node.
    case pressKey(NativeKeyboardKey)
}
