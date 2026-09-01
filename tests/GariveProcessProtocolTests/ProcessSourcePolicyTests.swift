import Foundation
import Testing

@Test("process protocol sources contain no ambient configuration, launch, or raw logging")
func processProtocolSourcePolicy() throws {
    let repository = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    let swiftSource = repository.appendingPathComponent("Sources/GariveProcessProtocol")
    let rustSource = repository.appendingPathComponent("engine/proto/src")
    let files = sourceFiles(in: swiftSource) + sourceFiles(in: rustSource).filter {
        $0.lastPathComponent.hasPrefix("process_")
    }
    let source = try files.map { try String(contentsOf: $0, encoding: .utf8) }.joined()
    let forbidden = [
        "ProcessInfo.processInfo.environment", "getenv(", "std::env", "Command::new(",
        "std::process::Command", "Process()", "/bin/sh", "/bin/bash", "/bin/zsh",
        "VZVirtualMachine(", "print(", "NSLog(", "os_log(", "log::",
        "tracing::", "localizedDescription", "String(describing:", "Error(String",
    ]
    for token in forbidden {
        #expect(!source.contains(token), "forbidden process protocol source: \(token)")
    }
}

private func sourceFiles(in directory: URL) -> [URL] {
    let entries = FileManager.default.enumerator(at: directory, includingPropertiesForKeys: nil)
    return ((entries?.allObjects as? [URL]) ?? []).filter {
        $0.pathExtension == "swift" || $0.pathExtension == "rs"
    }
}
