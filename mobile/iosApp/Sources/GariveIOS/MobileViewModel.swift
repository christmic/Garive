#if canImport(GariveShared)
import Foundation
@preconcurrency import GariveShared

final class UUIDIdentitySource: NSObject, CommandIdentitySource {
    func nextId() -> String { UUID().uuidString.lowercased() }
}

@MainActor
final class MobileViewModel: ObservableObject {
    @Published private(set) var state: MobileWorkState?
    @Published private(set) var credentials: ConnectionCredentials?
    @Published private(set) var errorCode: String?
    @Published var presentingNewTask = false

    private let store: ConnectionStore
    private var controller: MobileWorkController?

    init(store: ConnectionStore = ConnectionStore()) {
        self.store = store
        credentials = store.load()
        if let credentials { connect(credentials, persist: false) }
    }

    func pair(origin: String, accessGrant: String) {
        let value = ConnectionCredentials(
            origin: origin.trimmingCharacters(in: .whitespacesAndNewlines),
            accessGrant: accessGrant.trimmingCharacters(in: .whitespacesAndNewlines)
        )
        connect(value, persist: true)
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
        store.clear()
        state = controller?.signOut()
        controller = nil
        credentials = nil
        errorCode = nil
    }

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
                host: host, identities: UUIDIdentitySource(), pageLimit: 100, maxInputBytes: 16_384
            )
            self.controller = controller
            credentials = value
            if persist { try store.save(value) }
            perform { callback in controller.boot(completionHandler: callback) }
        } catch {
            errorCode = "secure_connection_failed"
            if persist { store.clear(); credentials = nil }
        }
    }

    private func perform(
        _ operation: (@escaping @Sendable (MobileWorkState?, Error?) -> Void) -> Void
    ) {
        errorCode = nil
        operation { [weak self] value, error in
            Task { @MainActor in
                guard let self else { return }
                if let value { self.state = value }
                if error != nil {
                    self.errorCode = self.controller?.state().noticeCode ?? "remote_operation_failed"
                }
            }
        }
    }
}
#endif
