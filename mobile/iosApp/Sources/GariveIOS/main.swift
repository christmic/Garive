import SwiftUI
#if canImport(GariveShared)
import GariveShared
#endif

public struct FakeHost {
    public init() {}
    public static var usesSharedFramework: Bool {
#if canImport(GariveShared)
        true
#else
        false
#endif
    }
    public func run(_ input: String) throws -> String {
        guard input == "hello" else { throw HostError.unsupportedInput }
#if canImport(GariveShared)
        return EmbeddedFakeHostBridge.shared.runText()
#else
        let events = [(1, "session.created", ""), (2, "turn.started", ""),
            (3, "output.delta", "hello "), (4, "output.delta", "from Garive"),
            (5, "turn.completed", "")]
        var previous = 0, terminal = false, output = ""
        for (position, kind, text) in events {
            guard !terminal, position == previous + 1 else { throw HostError.invalidSequence }
            previous = position
            if kind == "output.delta" { output += text }
            if kind == "turn.completed" { terminal = true }
        }
        guard terminal else { throw HostError.missingTerminal }
        return output
#endif
    }
    public enum HostError: Error { case unsupportedInput, invalidSequence, missingTerminal }
}

@main struct GariveIOSApp: App {
    @State private var output = ""
    var body: some Scene { WindowGroup { VStack(spacing: 16) {
        Text("Garive Agent").font(.largeTitle); Text("You: hello")
        Button("Run embedded host") { output = (try? FakeHost().run("hello")) ?? "failed" }
        Text(output.isEmpty ? "Ready" : "\(output) · completed").foregroundStyle(.mint)
    }.padding() } }
}
