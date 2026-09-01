import CryptoKit
import Foundation

private let maximumArguments = 256
private let maximumArgumentBytes = 16_384
private let maximumArgumentsTotalBytes = 262_144
private let maximumEnvironmentEntries = 128
private let maximumEnvironmentValueBytes = 16_384
private let maximumEnvironmentTotalBytes = 262_144
private let maximumOutputBytes = 1_048_576

/// Closed failures for canonical process-protocol digests.
public enum ProcessDigestFailure: Error, Equatable, Sendable {
    case invalidIdentity
    case invalidWorkload
    case invalidReceipt
}

/// Validates and computes the canonical V0-B workload digest.
public func processWorkloadDigest(
    identity: GRVProcessIdentityV1,
    workload: GRVProcessWorkloadV1
) throws -> Data {
    guard processIdentityIsValid(identity) else { throw ProcessDigestFailure.invalidIdentity }
    let mode = try validate(workload)
    var input = DigestInput(label: "garive.macos-process-workload.v1")
    [
        Data(identity.protocolRevision.utf8), Data(identity.invocationID.utf8),
        Data(identity.dispatchAttemptID.utf8), Data(identity.executorRevision.utf8),
        identity.preparedDigest, identity.vmConfigurationDigest,
        Data(workload.lane.utf8), Data(workload.executable.utf8),
    ].forEach { input.field($0) }
    input.number(UInt64(workload.argv.count))
    workload.argv.forEach { input.field(Data($0.utf8)) }
    input.field(Data(workload.workingDirectory.utf8))
    input.byte(mode)
    input.number(UInt64(workload.environment.count))
    workload.environment.forEach {
        input.field(Data($0.key.utf8))
        input.field(Data($0.value.utf8))
    }
    [workload.maxOutputBytes, workload.timeoutMilliseconds,
     UInt64(workload.maxProcesses), UInt64(workload.maxOpenFiles)]
        .forEach { input.number($0) }
    let digest = input.finish()
    guard identity.workloadDigest.isEmpty || identity.workloadDigest == digest else {
        throw ProcessDigestFailure.invalidIdentity
    }
    return digest
}

/// Validates and computes the canonical V0-B terminal-receipt digest.
public func processReceiptDigest(_ receipt: GRVProcessTerminalReceiptV1) throws -> Data {
    guard receipt.hasIdentity, processIdentityIsValid(receipt.identity),
          receipt.identity.workloadDigest.count == 32,
          receipt.processTreeTerminated,
          receipt.stderr.count <= maximumOutputBytes,
          receipt.stdout.count <= maximumOutputBytes - receipt.stderr.count,
          receipt.hasExit, let classification = receipt.exit.classification
    else { throw ProcessDigestFailure.invalidReceipt }
    var input = DigestInput(label: "garive.macos-process-receipt.v1")
    input.field(receipt.identity.workloadDigest)
    switch classification {
    case let .code(value):
        input.byte(0); input.signed(value)
    case let .signal(value) where value > 0:
        input.byte(1); input.signed(value)
    case .timedOut(true):
        input.byte(2)
    default:
        throw ProcessDigestFailure.invalidReceipt
    }
    input.field(receipt.stdout)
    input.field(receipt.stderr)
    input.byte(receipt.truncated ? 1 : 0)
    input.byte(1)
    let digest = input.finish()
    guard receipt.receiptDigest.isEmpty || receipt.receiptDigest == digest else {
        throw ProcessDigestFailure.invalidReceipt
    }
    return digest
}

func processIdentityIsValid(_ value: GRVProcessIdentityV1) -> Bool {
    validProtocolRevision(value.protocolRevision)
        && [value.invocationID, value.dispatchAttemptID, value.executorRevision]
            .allSatisfy { validIdentityText($0, maximum: 256) }
        && value.preparedDigest.count == 32
        && value.vmConfigurationDigest.count == 32
        && (value.workloadDigest.isEmpty || value.workloadDigest.count == 32)
}

private func validate(_ value: GRVProcessWorkloadV1) throws -> UInt8 {
    let mode: UInt8
    switch value.workspaceMode {
    case .processWorkspaceModeReadOnly: mode = 1
    case .processWorkspaceModeReadWrite: mode = 2
    default: throw ProcessDigestFailure.invalidWorkload
    }
    var argumentTotal = 0
    for argument in value.argv {
        let length = argument.utf8.count
        let (total, overflow) = argumentTotal.addingReportingOverflow(length)
        guard (1...maximumArgumentBytes).contains(length), !argument.contains("\0"), !overflow
        else { throw ProcessDigestFailure.invalidWorkload }
        argumentTotal = total
    }
    var priorKey: [UInt8]?
    var environmentTotal = 0
    for entry in value.environment {
        let key = Array(entry.key.utf8)
        let valueBytes = Array(entry.value.utf8)
        guard priorKey.map({ $0.lexicographicallyPrecedes(key) }) ?? true,
              validEnvironmentKey(key), valueBytes.count <= maximumEnvironmentValueBytes,
              !valueBytes.contains(where: { $0 == 0 || $0 == 10 || $0 == 13 })
        else { throw ProcessDigestFailure.invalidWorkload }
        priorKey = key
        let (entryBytes, entryOverflow) = key.count.addingReportingOverflow(valueBytes.count)
        let (total, totalOverflow) = environmentTotal.addingReportingOverflow(entryBytes)
        guard !entryOverflow, !totalOverflow else { throw ProcessDigestFailure.invalidWorkload }
        environmentTotal = total
    }
    guard validIdentityText(value.lane, maximum: 128),
          validAbsoluteGuestPath(value.executable),
          validRelativeWorkspacePath(value.workingDirectory),
          !value.argv.isEmpty, value.argv.count <= maximumArguments,
          argumentTotal <= maximumArgumentsTotalBytes,
          value.environment.count <= maximumEnvironmentEntries,
          environmentTotal <= maximumEnvironmentTotalBytes,
          (1...UInt64(maximumOutputBytes)).contains(value.maxOutputBytes),
          (1...300_000).contains(value.timeoutMilliseconds),
          value.maxProcesses > 0, value.maxOpenFiles > 0
    else { throw ProcessDigestFailure.invalidWorkload }
    return mode
}

private func validProtocolRevision(_ value: String) -> Bool {
    let bytes = Array(value.utf8)
    return (1...128).contains(bytes.count)
        && bytes.first?.isASCIIAlphanumeric == true && bytes.last?.isASCIIAlphanumeric == true
        && bytes.allSatisfy { $0.isASCIILowercase || $0.isASCIIDigit || $0 == 46 || $0 == 45 }
}

private func validIdentityText(_ value: String, maximum: Int) -> Bool {
    let bytes = Array(value.utf8)
    return (1...maximum).contains(bytes.count) && bytes.allSatisfy { (0x20...0x7e).contains($0) }
        && bytes.first != 0x20 && bytes.last != 0x20
}

private func validAbsoluteGuestPath(_ value: String) -> Bool {
    let bytes = Array(value.utf8)
    return bytes.count > 1 && bytes.count <= 4_096 && value.first == "/"
        && value.dropFirst().split(separator: "/", omittingEmptySubsequences: false)
            .allSatisfy { !$0.isEmpty && $0 != "." && $0 != ".." && !$0.contains("\0") }
}

private func validRelativeWorkspacePath(_ value: String) -> Bool {
    let bytes = Array(value.utf8)
    return (1...4_096).contains(bytes.count) && !value.contains("\0")
        && (value == "." || (value.first != "/" && value.split(separator: "/", omittingEmptySubsequences: false)
            .allSatisfy { !$0.isEmpty && $0 != "." && $0 != ".." }))
}

private func validEnvironmentKey(_ bytes: [UInt8]) -> Bool {
    guard (1...128).contains(bytes.count),
          bytes[0] == 95 || bytes[0].isASCIIAlphabetic else { return false }
    return bytes.dropFirst().allSatisfy { $0 == 95 || $0.isASCIIAlphanumeric }
}

private struct DigestInput {
    private var data: Data
    init(label: String) { data = Data(label.utf8) }
    mutating func field(_ value: Data) { number(UInt64(value.count)); data.append(value) }
    mutating func number(_ value: UInt64) { append(value.bigEndian) }
    mutating func signed(_ value: Int32) { append(value.bigEndian) }
    mutating func byte(_ value: UInt8) { data.append(value) }
    mutating private func append<T>(_ value: T) {
        var copy = value
        withUnsafeBytes(of: &copy) { data.append(contentsOf: $0) }
    }
    func finish() -> Data { Data(SHA256.hash(data: data)) }
}

private extension UInt8 {
    var isASCIIDigit: Bool { (48...57).contains(self) }
    var isASCIILowercase: Bool { (97...122).contains(self) }
    var isASCIIAlphabetic: Bool { isASCIILowercase || (65...90).contains(self) }
    var isASCIIAlphanumeric: Bool { isASCIIAlphabetic || isASCIIDigit }
}
