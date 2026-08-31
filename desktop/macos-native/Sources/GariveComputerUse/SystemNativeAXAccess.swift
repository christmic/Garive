import ApplicationServices
import Foundation

final class SystemNativeAXAccess: NativeAXAccessing {
    func windows(processIdentifier: Int32) throws -> [AXUIElement] {
        let application = AXUIElementCreateApplication(processIdentifier)
        return try elements(attribute: kAXWindowsAttribute, of: application)
    }

    func focusedWindow(processIdentifier: Int32) throws -> AXUIElement? {
        let application = AXUIElementCreateApplication(processIdentifier)
        guard let result = try value(attribute: kAXFocusedWindowAttribute, of: application) else {
            return nil
        }
        guard CFGetTypeID(result as CFTypeRef) == AXUIElementGetTypeID() else {
            throw NativeAXObservationFailure.invalidNativeData
        }
        return unsafeDowncast(result as AnyObject, to: AXUIElement.self)
    }

    func isFrontmostApplication(processIdentifier: Int32) throws -> Bool {
        let application = AXUIElementCreateApplication(processIdentifier)
        return try boolean(attribute: kAXFrontmostAttribute, of: application) == true
    }

    func isSameElement(_ left: AXUIElement, _ right: AXUIElement) -> Bool {
        CFEqual(left, right)
    }

    func performPress(on element: AXUIElement) throws {
        try requireActionSuccess(
            AXUIElementPerformAction(element, kAXPressAction as CFString)
        )
    }

    func setValue(_ value: String, on element: AXUIElement) throws {
        try requireActionSuccess(
            AXUIElementSetAttributeValue(
                element,
                kAXValueAttribute as CFString,
                value as CFString
            )
        )
    }

    func semanticElement(
        root: AXUIElement,
        bounds: NativeAXObservationBounds
    ) throws -> NativeAXReadResult {
        var visited: [AXUIElement] = []
        var nodeCount = 0
        var textBytes = 0

        func readShallow(
            _ element: AXUIElement
        ) throws -> NativeAXSemanticSnapshotBuilder.Element {
            guard !visited.contains(where: { CFEqual($0, element) }) else {
                throw NativeAXObservationFailure.invalidNativeData
            }
            guard nodeCount < bounds.maxNodes else {
                throw NativeAXObservationFailure.resultBoundExceeded
            }
            visited.append(element)
            nodeCount += 1

            let role = try requiredString(attribute: kAXRoleAttribute, of: element)
            let subrole = try string(attribute: kAXSubroleAttribute, of: element)
            let label = try string(attribute: kAXTitleAttribute, of: element)
                ?? string(attribute: kAXDescriptionAttribute, of: element)
            let secure = subrole == kAXSecureTextFieldSubrole
            let value = secure ? nil : try scalarString(attribute: kAXValueAttribute, of: element)
            for text in [role, subrole, label, value].compactMap({ $0 }) {
                let (next, overflow) = textBytes.addingReportingOverflow(text.utf8.count)
                guard !overflow, next <= bounds.maxTextBytes else {
                    throw NativeAXObservationFailure.resultBoundExceeded
                }
                textBytes = next
            }
            let actions = try actionNames(of: element)
            return NativeAXSemanticSnapshotBuilder.Element(
                role: role,
                subrole: subrole,
                label: label,
                value: value,
                enabled: try boolean(attribute: kAXEnabledAttribute, of: element),
                focused: try boolean(attribute: kAXFocusedAttribute, of: element),
                selected: try boolean(attribute: kAXSelectedAttribute, of: element),
                pressSupported: actions.contains(kAXPressAction),
                valueSettable: secure ? false : try isValueSettable(element),
                frame: try frame(of: element),
                children: []
            )
        }

        var semanticRoot: NativeAXSemanticSnapshotBuilder.Element?
        var nativeElements: [AXUIElement] = []
        var pending: [(
            native: AXUIElement,
            parent: NativeAXSemanticSnapshotBuilder.Element?
        )] = [
            (root, nil),
        ]
        while let item = pending.popLast() {
            let semantic = try readShallow(item.native)
            if let parent = item.parent {
                parent.children.append(semantic)
            } else {
                guard semanticRoot == nil else {
                    throw NativeAXObservationFailure.invalidNativeData
                }
                semanticRoot = semantic
            }
            nativeElements.append(item.native)
            let nativeChildren = try elements(
                attribute: kAXChildrenAttribute,
                of: item.native
            )
            for nativeChild in nativeChildren.reversed() {
                pending.append((nativeChild, semantic))
            }
        }
        guard let semanticRoot else {
            throw NativeAXObservationFailure.invalidNativeData
        }
        return NativeAXReadResult(root: semanticRoot, elements: nativeElements)
    }

    private func value(attribute: String, of element: AXUIElement) throws -> Any? {
        var result: CFTypeRef?
        let error = AXUIElementCopyAttributeValue(element, attribute as CFString, &result)
        switch error {
        case .success:
            return result
        case .noValue, .attributeUnsupported:
            return nil
        case .apiDisabled:
            throw NativeAXObservationFailure.permissionRequired
        case .invalidUIElement, .cannotComplete:
            throw NativeAXObservationFailure.targetChanged
        default:
            throw NativeAXObservationFailure.invalidNativeData
        }
    }

    private func requiredString(attribute: String, of element: AXUIElement) throws -> String {
        guard let result = try string(attribute: attribute, of: element), !result.isEmpty else {
            throw NativeAXObservationFailure.invalidNativeData
        }
        return result
    }

    private func string(attribute: String, of element: AXUIElement) throws -> String? {
        try value(attribute: attribute, of: element) as? String
    }

    private func scalarString(attribute: String, of element: AXUIElement) throws -> String? {
        guard let result = try value(attribute: attribute, of: element) else { return nil }
        if let string = result as? String { return string }
        if let number = result as? NSNumber { return number.stringValue }
        return nil
    }

    private func boolean(attribute: String, of element: AXUIElement) throws -> Bool? {
        try value(attribute: attribute, of: element) as? Bool
    }

    private func elements(attribute: String, of element: AXUIElement) throws -> [AXUIElement] {
        guard let result = try value(attribute: attribute, of: element) else { return [] }
        guard let elements = result as? [AXUIElement] else {
            throw NativeAXObservationFailure.invalidNativeData
        }
        return elements
    }

    private func actionNames(of element: AXUIElement) throws -> [String] {
        var names: CFArray?
        let error = AXUIElementCopyActionNames(element, &names)
        guard error == .success else {
            if error == .apiDisabled {
                throw NativeAXObservationFailure.permissionRequired
            }
            throw NativeAXObservationFailure.invalidNativeData
        }
        return names as? [String] ?? []
    }

    private func isValueSettable(_ element: AXUIElement) throws -> Bool {
        var settable = DarwinBoolean(false)
        let error = AXUIElementIsAttributeSettable(
            element,
            kAXValueAttribute as CFString,
            &settable
        )
        if error == .attributeUnsupported || error == .noValue { return false }
        guard error == .success else {
            throw NativeAXObservationFailure.invalidNativeData
        }
        return settable.boolValue
    }

    private func frame(of element: AXUIElement) throws -> NativeAXSemanticNode.Frame? {
        guard let positionObject = try value(attribute: kAXPositionAttribute, of: element),
              let sizeObject = try value(attribute: kAXSizeAttribute, of: element)
        else { return nil }
        guard CFGetTypeID(positionObject as CFTypeRef) == AXValueGetTypeID(),
              CFGetTypeID(sizeObject as CFTypeRef) == AXValueGetTypeID()
        else {
            throw NativeAXObservationFailure.invalidNativeData
        }
        let position = unsafeDowncast(positionObject as AnyObject, to: AXValue.self)
        let size = unsafeDowncast(sizeObject as AnyObject, to: AXValue.self)
        var point = CGPoint.zero
        var dimensions = CGSize.zero
        guard AXValueGetValue(position, .cgPoint, &point),
              AXValueGetValue(size, .cgSize, &dimensions)
        else {
            throw NativeAXObservationFailure.invalidNativeData
        }
        return NativeAXSemanticNode.Frame(
            x: point.x,
            y: point.y,
            width: dimensions.width,
            height: dimensions.height
        )
    }

    private func requireActionSuccess(_ error: AXError) throws {
        switch error {
        case .success:
            return
        case .apiDisabled:
            throw NativeAXActionFailure.permissionRevoked
        case .invalidUIElement:
            throw NativeAXActionFailure.targetChanged
        case .actionUnsupported, .attributeUnsupported:
            throw NativeAXActionFailure.actionUnsupported
        case .illegalArgument, .notEnoughPrecision:
            throw NativeAXActionFailure.invalidAction
        default:
            throw NativeAXActionFailure.actionUncertain
        }
    }
}
