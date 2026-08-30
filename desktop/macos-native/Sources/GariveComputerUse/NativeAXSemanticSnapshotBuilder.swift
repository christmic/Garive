private let secureTextFieldSubrole = "AXSecureTextField"

enum NativeAXSemanticSnapshotBuilder {
    final class Element {
        let role: String
        let subrole: String?
        let label: String?
        let value: String?
        let enabled: Bool?
        let focused: Bool?
        let selected: Bool?
        let pressSupported: Bool
        let valueSettable: Bool
        let frame: NativeAXSemanticNode.Frame?
        let children: [Element]

        init(
            role: String,
            subrole: String? = nil,
            label: String? = nil,
            value: String? = nil,
            enabled: Bool? = nil,
            focused: Bool? = nil,
            selected: Bool? = nil,
            pressSupported: Bool = false,
            valueSettable: Bool = false,
            frame: NativeAXSemanticNode.Frame? = nil,
            children: [Element] = []
        ) {
            self.role = role
            self.subrole = subrole
            self.label = label
            self.value = value
            self.enabled = enabled
            self.focused = focused
            self.selected = selected
            self.pressSupported = pressSupported
            self.valueSettable = valueSettable
            self.frame = frame
            self.children = children
        }
    }

    static func build(
        root: Element,
        bounds: NativeAXObservationBounds
    ) throws -> NativeAXSemanticSnapshot {
        var nodes: [NativeAXSemanticNode] = []
        var textBytes = 0
        var focusedNodeIndex: Int?
        var redactedValueCount = 0

        func charge(_ value: String?) throws {
            guard let value else { return }
            let (next, overflow) = textBytes.addingReportingOverflow(value.utf8.count)
            guard !overflow, next <= bounds.maxTextBytes else {
                throw NativeAXObservationFailure.resultBoundExceeded
            }
            textBytes = next
        }

        func append(_ element: Element, parentIndex: Int?) throws {
            guard !element.role.isEmpty else {
                throw NativeAXObservationFailure.invalidNativeData
            }
            guard nodes.count < bounds.maxNodes else {
                throw NativeAXObservationFailure.resultBoundExceeded
            }
            if let frame = element.frame {
                guard frame.x.isFinite, frame.y.isFinite,
                      frame.width.isFinite, frame.height.isFinite,
                      frame.width >= 0, frame.height >= 0
                else {
                    throw NativeAXObservationFailure.invalidNativeData
                }
            }

            let secure = element.subrole == secureTextFieldSubrole
            let visibleValue = secure ? nil : element.value
            try charge(element.role)
            try charge(element.subrole)
            try charge(element.label)
            try charge(visibleValue)

            let nodeIndex = nodes.count
            if element.focused == true {
                guard focusedNodeIndex == nil else {
                    throw NativeAXObservationFailure.invalidNativeData
                }
                focusedNodeIndex = nodeIndex
            }
            if secure {
                redactedValueCount += 1
            }
            var actions: [NativeAXSemanticNode.SupportedAction] = []
            if element.pressSupported { actions.append(.press) }
            if element.valueSettable, !secure { actions.append(.setValue) }
            nodes.append(NativeAXSemanticNode(
                nodeIndex: nodeIndex,
                parentIndex: parentIndex,
                role: element.role,
                subrole: element.subrole,
                label: element.label,
                valueSummary: visibleValue,
                valueSensitivity: secure ? .protected : (visibleValue == nil ? nil : .ordinary),
                valueRedacted: secure,
                enabled: element.enabled,
                focused: element.focused,
                selected: element.selected,
                supportedActions: actions,
                frame: element.frame
            ))
            for child in element.children {
                try append(child, parentIndex: nodeIndex)
            }
        }

        try append(root, parentIndex: nil)
        return NativeAXSemanticSnapshot(
            nodes: nodes,
            focusedNodeIndex: focusedNodeIndex,
            textBytes: textBytes,
            redactedValueCount: redactedValueCount
        )
    }
}
