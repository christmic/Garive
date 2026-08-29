import SwiftUI
#if canImport(GariveShared)
@preconcurrency import GariveShared
#endif

/// UI-safe terminal returned by the shared KMP H1 client.
public struct MobileHostResult: Equatable, Sendable {
    public let text: String
    public let terminal: String
    public let cursor: Int64
}

/// Stable Swift bridge failures that never preserve Host content.
public enum MobileHostError: Error { case sharedFrameworkUnavailable, invalidResponse }

/// Callback bridge around the generated KMP completion-handler surface.
public struct MobileHostRunner {
    public init() {}
    public static var usesSharedFramework: Bool {
#if canImport(GariveShared)
        true
#else
        false
#endif
    }

    public func run(
        hostURL: String,
        definitionID: String,
        message: String,
        completion: @escaping @Sendable (Result<MobileHostResult, Error>) -> Void
    ) {
#if canImport(GariveShared)
        do {
            let limits = HostClientLimits(
                maxCommandBytes: 4_096, maxEventBytes: 8_192,
                maxEvents: 256, followDeadlineMs: 120_000
            )
            let client = try LiveHostClient(baseUrl: hostURL, limits: limits)
            let identity = "ios-\(DispatchTime.now().uptimeNanoseconds)"
            client.createSession(commandId: "create-\(identity)", definitionId: definitionID) { session, error in
                guard let session else { completion(.failure(error ?? MobileHostError.invalidResponse)); return }
                client.startTurn(
                    commandId: "turn-\(identity)", sessionId: session.session_id, text: message
                ) { turn, error in
                    guard let turn else { completion(.failure(error ?? MobileHostError.invalidResponse)); return }
                    client.followUntilTerminal(
                        sessionId: session.session_id, afterPosition: turn.committed_position
                    ) { view, error in
                        guard let view, let terminal = view.terminal else {
                            completion(.failure(error ?? MobileHostError.invalidResponse)); return
                        }
                        completion(.success(MobileHostResult(
                            text: view.text, terminal: terminal.name.lowercased(), cursor: view.cursor
                        )))
                    }
                }
            }
        } catch {
            completion(.failure(error))
        }
#else
        completion(.failure(MobileHostError.sharedFrameworkUnavailable))
#endif
    }
}

@main
struct GariveIOSApp: App {
    @State private var hostURL = "http://127.0.0.1:4317/"
    @State private var definitionID = "definition-main"
    @State private var message = "hello"
    @State private var output = "Ready"
    var body: some Scene {
        WindowGroup {
            VStack(spacing: 16) {
                Text("Garive Agent").font(.largeTitle)
                TextField("Loopback Host URL", text: $hostURL)
                TextField("Agent definition", text: $definitionID)
                TextField("Message", text: $message)
                Button("Run Agent") {
                    output = "running"
                    MobileHostRunner().run(
                        hostURL: hostURL, definitionID: definitionID, message: message
                    ) { result in
                        DispatchQueue.main.async {
                            output = result.fold(
                                success: { "\($0.text) · \($0.terminal) @ \($0.cursor)" },
                                failure: { _ in "transport_failure" }
                            )
                        }
                    }
                }
                Text(output).foregroundStyle(.mint)
            }.padding()
        }
    }
}

private extension Result {
    func fold<T>(success: (Success) -> T, failure: (Failure) -> T) -> T {
        switch self {
        case .success(let value): success(value)
        case .failure(let error): failure(error)
        }
    }
}
