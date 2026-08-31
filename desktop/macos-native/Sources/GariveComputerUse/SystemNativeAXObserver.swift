import ApplicationServices

/// Prompt-free native Accessibility window enumeration and observation.
public final class SystemNativeAXObserver {
    private let access: any NativeAXAccessing
    private let keyboard: any NativeKeyboardDispatching
    private let permissionState: () -> NativePermissionState
    private let isCurrent: (NativeApplicationInstanceIdentity) throws -> Bool

    /// Creates an observer bound to one explicit application signing policy.
    public init(applicationVerifier: NativeApplicationIdentityVerifier) {
        access = SystemNativeAXAccess()
        keyboard = SystemNativeKeyboardDispatcher()
        permissionState = {
            SystemNativePermissionInspector().inspect().accessibility
        }
        isCurrent = applicationVerifier.isCurrent
    }

    init(
        access: any NativeAXAccessing,
        keyboard: any NativeKeyboardDispatching = SystemNativeKeyboardDispatcher(),
        permissionState: @escaping () -> NativePermissionState,
        isCurrent: @escaping (NativeApplicationInstanceIdentity) throws -> Bool
    ) {
        self.access = access
        self.keyboard = keyboard
        self.permissionState = permissionState
        self.isCurrent = isCurrent
    }

    /// Enumerates exact windows only for the still-current admitted process.
    public func bindWindows(
        applicationIdentity: NativeApplicationInstanceIdentity
    ) throws -> [NativeAXWindowBinding] {
        try requireAccess(to: applicationIdentity)
        let elements = try access.windows(
            processIdentifier: applicationIdentity.processIdentifier
        )
        try requireCurrent(applicationIdentity)
        let ownerIdentifier = ObjectIdentifier(access)
        return elements.map {
            NativeAXWindowBinding(
                applicationIdentity: applicationIdentity,
                element: $0,
                ownerIdentifier: ownerIdentifier
            )
        }
    }

    /// Reads one bounded semantic tree only while its exact window remains bound.
    public func observe(
        window: NativeAXWindowBinding,
        bounds: NativeAXObservationBounds
    ) throws -> NativeAXObservationBinding {
        guard window.ownerIdentifier == ObjectIdentifier(access) else {
            throw NativeAXObservationFailure.targetChanged
        }
        try requireAccess(to: window.applicationIdentity)
        try requireWindow(window)
        let read = try access.semanticElement(root: window.element, bounds: bounds)
        let snapshot = try NativeAXSemanticSnapshotBuilder.build(root: read.root, bounds: bounds)
        guard read.elements.count == snapshot.nodes.count else {
            throw NativeAXObservationFailure.invalidNativeData
        }
        try requireCurrent(window.applicationIdentity)
        try requireWindow(window)
        return NativeAXObservationBinding(
            snapshot: snapshot,
            window: window,
            elements: read.elements,
            ownerIdentifier: ObjectIdentifier(access),
            bounds: bounds
        )
    }

    /// Revalidates and dispatches one exact snapshot-local semantic action.
    public func perform(
        action: NativeAXSemanticAction,
        observation: NativeAXObservationBinding
    ) throws -> NativeAXObservationBinding {
        let nodeIndex = try validate(action: action, observation: observation)
        let usesKeyboard: Bool
        switch action {
        case .typeText, .pressKey: usesKeyboard = true
        case .press, .setValue: usesKeyboard = false
        }
        let fresh = try revalidateForAction(
            observation,
            nodeIndex: nodeIndex,
            requireFocusedWindow: usesKeyboard
        )
        let keyboardDispatch: (() -> Void)?
        switch action {
        case let .typeText(_, text):
            keyboardDispatch = try keyboard.prepareTypeText(
                text,
                processIdentifier: observation.window.applicationIdentity.processIdentifier
            )
        case let .pressKey(key):
            keyboardDispatch = try keyboard.preparePressKey(
                key,
                processIdentifier: observation.window.applicationIdentity.processIdentifier
            )
        case .press, .setValue:
            keyboardDispatch = nil
        }
        if usesKeyboard {
            try revalidateKeyboardDispatchBoundary(observation.window)
        }
        guard observation.consume() else {
            throw NativeAXActionFailure.snapshotStale
        }
        switch action {
        case .press:
            try access.performPress(on: fresh)
        case let .setValue(_, value):
            try access.setValue(value, on: fresh)
        case .typeText, .pressKey:
            keyboardDispatch?()
        }
        do {
            return try observe(window: observation.window, bounds: observation.bounds)
        } catch {
            throw NativeAXActionFailure.actionUncertain
        }
    }

    private func requireAccess(
        to identity: NativeApplicationInstanceIdentity
    ) throws {
        guard permissionState() == .granted else {
            throw NativeAXObservationFailure.permissionRequired
        }
        try requireCurrent(identity)
    }

    private func requireCurrent(
        _ identity: NativeApplicationInstanceIdentity
    ) throws {
        guard try isCurrent(identity) else {
            throw NativeAXObservationFailure.targetChanged
        }
    }

    private func requireWindow(_ binding: NativeAXWindowBinding) throws {
        let current = try access.windows(
            processIdentifier: binding.applicationIdentity.processIdentifier
        )
        guard current.contains(where: {
            access.isSameElement($0, binding.element)
        }) else {
            throw NativeAXObservationFailure.targetChanged
        }
    }

    private func validate(
        action: NativeAXSemanticAction,
        observation: NativeAXObservationBinding
    ) throws -> Int {
        guard observation.ownerIdentifier == ObjectIdentifier(access) else {
            throw NativeAXActionFailure.targetChanged
        }
        let nodeIndex: Int
        let requiredAction: NativeAXSemanticNode.SupportedAction?
        switch action {
        case let .press(index):
            nodeIndex = index
            requiredAction = .press
        case let .setValue(index, value):
            guard value.unicodeScalars.count <= 32_768,
                  value.utf8.count <= 131_072
            else {
                throw NativeAXActionFailure.invalidAction
            }
            nodeIndex = index
            requiredAction = .setValue
        case let .typeText(index, text):
            guard !text.isEmpty,
                  text.unicodeScalars.count <= 32_768,
                  text.utf8.count <= 131_072
            else {
                throw NativeAXActionFailure.invalidAction
            }
            nodeIndex = index
            requiredAction = .typeText
        case .pressKey:
            guard let focusedNodeIndex = observation.snapshot.focusedNodeIndex else {
                throw NativeAXActionFailure.focusChanged
            }
            nodeIndex = focusedNodeIndex
            requiredAction = nil
        }
        guard observation.snapshot.nodes.indices.contains(nodeIndex),
              observation.elements.indices.contains(nodeIndex)
        else {
            throw NativeAXActionFailure.nodeStale
        }
        let node = observation.snapshot.nodes[nodeIndex]
        if case .setValue = action, node.valueSensitivity == .protected {
            throw NativeAXActionFailure.sensitiveActionRequired
        }
        if case .typeText = action, node.valueSensitivity == .protected {
            throw NativeAXActionFailure.sensitiveActionRequired
        }
        if case .typeText = action, node.focused != true {
            throw NativeAXActionFailure.focusChanged
        }
        guard requiredAction.map(node.supportedActions.contains) ?? true else {
            throw NativeAXActionFailure.actionUnsupported
        }
        return nodeIndex
    }

    private func revalidateForAction(
        _ observation: NativeAXObservationBinding,
        nodeIndex: Int,
        requireFocusedWindow: Bool
    ) throws -> AXUIElement {
        guard permissionState() == .granted else {
            throw NativeAXActionFailure.permissionRevoked
        }
        do {
            try requireCurrent(observation.window.applicationIdentity)
            try requireWindow(observation.window)
            if requireFocusedWindow {
                try requireKeyboardFocus(observation.window)
            }
            let read = try access.semanticElement(
                root: observation.window.element,
                bounds: observation.bounds
            )
            let snapshot = try NativeAXSemanticSnapshotBuilder.build(
                root: read.root,
                bounds: observation.bounds
            )
            guard read.elements.count == snapshot.nodes.count,
                  snapshot == observation.snapshot
            else {
                throw NativeAXActionFailure.snapshotStale
            }
            guard read.elements.indices.contains(nodeIndex),
                  access.isSameElement(
                      read.elements[nodeIndex],
                      observation.elements[nodeIndex]
                  )
            else {
                throw NativeAXActionFailure.nodeStale
            }
            try requireCurrent(observation.window.applicationIdentity)
            try requireWindow(observation.window)
            if requireFocusedWindow {
                try requireKeyboardFocus(observation.window)
            }
            return read.elements[nodeIndex]
        } catch let failure as NativeAXActionFailure {
            throw failure
        } catch NativeAXObservationFailure.permissionRequired {
            throw NativeAXActionFailure.permissionRevoked
        } catch NativeAXObservationFailure.targetChanged {
            throw NativeAXActionFailure.targetChanged
        } catch NativeAXObservationFailure.invalidBounds,
                NativeAXObservationFailure.invalidNativeData,
                NativeAXObservationFailure.resultBoundExceeded {
            throw NativeAXActionFailure.snapshotStale
        } catch {
            throw NativeAXActionFailure.targetChanged
        }
    }

    private func requireKeyboardFocus(_ binding: NativeAXWindowBinding) throws {
        guard try access.isFrontmostApplication(
            processIdentifier: binding.applicationIdentity.processIdentifier
        ) else {
            throw NativeAXActionFailure.focusChanged
        }
        let focused = try access.focusedWindow(
            processIdentifier: binding.applicationIdentity.processIdentifier
        )
        guard let focused, access.isSameElement(focused, binding.element) else {
            throw NativeAXActionFailure.focusChanged
        }
    }

    private func revalidateKeyboardDispatchBoundary(
        _ binding: NativeAXWindowBinding
    ) throws {
        guard permissionState() == .granted else {
            throw NativeAXActionFailure.permissionRevoked
        }
        do {
            try requireCurrent(binding.applicationIdentity)
            try requireWindow(binding)
            try requireKeyboardFocus(binding)
        } catch let failure as NativeAXActionFailure {
            throw failure
        } catch NativeAXObservationFailure.permissionRequired {
            throw NativeAXActionFailure.permissionRevoked
        } catch {
            throw NativeAXActionFailure.targetChanged
        }
    }
}
