#if canImport(GariveShared)
import Foundation
@preconcurrency import GariveShared

final class UUIDIdentitySource: NSObject, CommandIdentitySource {
    func nextId() -> String { UUID().uuidString.lowercased() }
}

private final class UnsafeTransfer<Value>: @unchecked Sendable {
    let value: Value
    init(_ value: Value) { self.value = value }
}

@MainActor
final class MobileViewModel: ObservableObject {
    @Published private(set) var state: MobileWorkState?
    @Published private(set) var credentials: ConnectionCredentials?
    @Published private(set) var errorCode: String?
    @Published private(set) var pairing = false
    @Published var presentingNewTask = false
    @Published private(set) var pairingSuggestion: PairingSuggestion?

    private let store: ConnectionStore
    private let workPersistence: MobileWorkPersistence
    private var controller: MobileWorkController?

    init(
        store: ConnectionStore = ConnectionStore(),
        workPersistence: MobileWorkPersistence = UserDefaultsMobileWorkPersistence()
    ) {
        self.store = store
        self.workPersistence = workPersistence
#if DEBUG
        if ProcessInfo.processInfo.arguments.contains("--garive-walkthrough") {
            connectWalkthrough()
            return
        }
#endif
        credentials = store.load()
#if os(iOS)
        MobilePushInbox.shared.attach(
            registration: { [weak self] in self?.registerPush($0) },
            wake: { [weak self] in self?.resolveWake($0) }
        )
#endif
        if let credentials { connect(credentials, persist: false) }
    }

    func pair(origin: String, accessGrant: String) {
        let service = origin.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            let client = try GatewayPairingClient(baseUrl: service, maxResponseBytes: 8_192)
            let publicKey = try store.devicePublicKey()
            pairing = true
            errorCode = nil
            client.exchange(
                code: accessGrant,
                deviceName: String(ProcessInfo.processInfo.hostName.prefix(100)),
                platform: .ios,
                devicePublicKey: publicKey
            ) { [weak self] grant, error in
                let transferredGrant = UnsafeTransfer(grant)
                let failed = error != nil
                Task { @MainActor in
                    guard let self else { return }
                    self.pairing = false
                    guard let grant = transferredGrant.value else {
                        self.errorCode = failed ? "pairing_rejected" : "invalid_pairing_response"
                        return
                    }
                    self.connect(ConnectionCredentials(origin: service, accessGrant: grant.accessGrant), persist: true)
                }
            }
        } catch {
            pairing = false
            errorCode = "secure_connection_failed"
        }
    }

    func acceptPairingURL(_ url: URL) {
        let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        let names = items.map(\.name)
        let allowed = Set(["origin", "code", "exp", "name"])
        func one(_ name: String) -> String? {
            let values = items.filter { $0.name == name }.compactMap(\.value)
            return values.count == 1 ? values[0] : nil
        }
        guard url.scheme == "garive", url.host == "pair", Set(names).isSubset(of: allowed),
              names.count == Set(names).count,
              let origin = one("origin"), let code = one("code"), let rawExpiry = one("exp"),
              let expiry = TimeInterval(rawExpiry), let serviceName = one("name"),
              expiry > Date().timeIntervalSince1970, expiry <= Date().timeIntervalSince1970 + 600,
              serviceName.count <= 100, code.count >= 6 else {
            errorCode = "invalid_pairing_link"
            pairingSuggestion = nil
            return
        }
        pairingSuggestion = PairingSuggestion(origin: origin, code: code, serviceName: serviceName)
        errorCode = nil
    }

    func refresh() { perform { callback in self.controller?.refresh(completionHandler: callback) } }
    func open(_ id: String) { perform { callback in self.controller?.openSession(sessionId: id, completionHandler: callback) } }
    func send(_ text: String) { perform { callback in self.controller?.sendTurn(text: text, completionHandler: callback) } }
    func start(definitionID: String, text: String) {
        presentingNewTask = false
        perform { callback in
            self.controller?.startTask(definitionId: definitionID, text: text, completionHandler: callback)
        }
    }
    func cancel() { perform { callback in self.controller?.cancelLatest(completionHandler: callback) } }
    func continueDecision(_ input: String) {
        perform { callback in self.controller?.continueLatest(input: input, completionHandler: callback) }
    }
    func retryExact() { perform { callback in self.controller?.retryExact(completionHandler: callback) } }

    func select(_ destination: MobileDestination) {
        guard let controller else { return }
        state = controller.selectDestination(destination: destination)
    }

    func signOut() {
        let previous = credentials
        store.clear()
        state = controller?.signOut()
        controller = nil
        credentials = nil
        errorCode = nil
        if let previous {
            if let notifications = try? GatewayNotificationClient(baseUrl: previous.origin, maxResponseBytes: 8_192) {
                notifications.unregister(accessGrant: previous.accessGrant) { _ in }
            }
            if let client = try? GatewayPairingClient(baseUrl: previous.origin, maxResponseBytes: 8_192) {
                client.revoke(accessGrant: previous.accessGrant) { _ in }
            }
        }
    }

#if os(iOS)
    private func registerPush(_ registrationID: String) {
        guard let credentials,
              let client = try? GatewayNotificationClient(baseUrl: credentials.origin, maxResponseBytes: 8_192) else { return }
        client.register(
            accessGrant: credentials.accessGrant, transport: .apns,
            registrationId: registrationID
        ) { _ in }
    }

    private func resolveWake(_ token: String) {
        guard let credentials,
              let client = try? GatewayNotificationClient(baseUrl: credentials.origin, maxResponseBytes: 8_192) else { return }
        client.resolve(accessGrant: credentials.accessGrant, routeToken: token) { [weak self] route, _ in
            let transferred = UnsafeTransfer(route)
            Task { @MainActor in
                guard let self, let route = transferred.value else { return }
                if route.destination == "session", let sessionID = route.sessionId {
                    self.open(sessionID)
                } else {
                    self.select(.settings)
                    self.refresh()
                }
            }
        }
    }
#endif

    private func connect(_ value: ConnectionCredentials, persist: Bool) {
        do {
            let limits = HostClientLimits(
                maxCommandBytes: 16_384, maxEventBytes: 65_536,
                maxEvents: 1_024, followDeadlineMs: 120_000
            )
            let host = try LiveHostClient(
                baseUrl: value.origin, bearerToken: value.accessGrant, limits: limits
            )
            let controller = MobileWorkController(
                host: host, identities: UUIDIdentitySource(), pageLimit: 100,
                maxInputBytes: 16_384, persistence: workPersistence
            )
            self.controller = controller
            credentials = value
            if persist { try store.save(value) }
#if os(iOS)
            if persist {
                MobilePushAuthorization.requestAfterPairing()
            } else {
                MobilePushAuthorization.resumeIfAuthorized()
            }
#endif
            perform { callback in controller.boot(completionHandler: callback) }
        } catch {
            errorCode = "secure_connection_failed"
            if persist { store.clear(); credentials = nil }
        }
    }

#if DEBUG
    private func connectWalkthrough() {
        let origin = "http://127.0.0.1:4318/"
        do {
            let limits = HostClientLimits(
                maxCommandBytes: 16_384, maxEventBytes: 65_536,
                maxEvents: 1_024, followDeadlineMs: 120_000
            )
            let host = try LiveHostClient(baseUrl: origin, limits: limits)
            let controller = MobileWorkController(
                host: host, identities: UUIDIdentitySource(), pageLimit: 100,
                maxInputBytes: 16_384, persistence: EphemeralMobileWorkPersistence.shared
            )
            self.controller = controller
            credentials = ConnectionCredentials(origin: origin, accessGrant: "walkthrough")
            let arguments = ProcessInfo.processInfo.arguments
            let destination: MobileDestination? = if arguments.contains("--garive-walkthrough-sessions") {
                .sessions
            } else if arguments.contains("--garive-walkthrough-agents") {
                .agents
            } else if arguments.contains("--garive-walkthrough-settings") {
                .settings
            } else {
                nil
            }
            bootWalkthrough(
                controller,
                destination: destination,
                presentNewTask: arguments.contains("--garive-walkthrough-new-task")
            )
        } catch {
            errorCode = "walkthrough_connection_failed"
        }
    }

    private func bootWalkthrough(
        _ controller: MobileWorkController,
        destination: MobileDestination?,
        presentNewTask: Bool
    ) {
        let transferredController = UnsafeTransfer(controller)
        let transferredDestination = UnsafeTransfer(destination)
        controller.boot { [weak self] value, error in
            let transferredValue = UnsafeTransfer(value)
            let failed = error != nil
            Task { @MainActor in
                guard let self else { return }
                if let value = transferredValue.value { self.state = value }
                if failed {
                    self.errorCode = transferredController.value.state().noticeCode
                        ?? "walkthrough_connection_failed"
                    return
                }
                if let destination = transferredDestination.value {
                    self.state = transferredController.value.selectDestination(destination: destination)
                }
                self.presentingNewTask = presentNewTask
            }
        }
    }
#endif

    private func perform(
        _ operation: (@escaping @Sendable (MobileWorkState?, Error?) -> Void) -> Void
    ) {
        errorCode = nil
        operation { [weak self] value, error in
            let transferredValue = UnsafeTransfer(value)
            let failed = error != nil
            Task { @MainActor in
                guard let self else { return }
                if let value = transferredValue.value {
                    self.state = value
                    if value.connection == .signedOut {
                        self.signOut()
                        return
                    }
                }
                if failed {
                    self.errorCode = self.controller?.state().noticeCode ?? "remote_operation_failed"
                }
            }
        }
    }
}

struct PairingSuggestion: Equatable {
    let origin: String
    let code: String
    let serviceName: String
}
#endif
