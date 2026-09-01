import Foundation

/// Closed lifecycle failures for one exact process dispatch attempt.
public enum ProcessStateFailure: Error, Equatable, Sendable {
    case identityMismatch
    case stateConflict
    case invalidTerminal

    /// Maps reducer failures to the closed wire failure vocabulary.
    public var protocolFailure: GRVProcessProtocolFailureV1 {
        switch self {
        case .identityMismatch: .processProtocolFailureIdentityMismatch
        case .stateConflict: .processProtocolFailureStateConflict
        case .invalidTerminal: .processProtocolFailureMalformed
        }
    }
}

private enum OwnedProcessState: Sendable {
    case absent
    case starting
    case running
    case terminalRetained(GRVProcessTerminalReceiptV1)
}

/// Pure fail-closed lifecycle reducer bound to one never-replayed identity.
public struct ProcessStateReducer: Sendable {
    private let identity: GRVProcessIdentityV1
    private var state: OwnedProcessState = .absent
    private var startConsumed = false

    /// Creates an absent reducer for one fully validated workload identity.
    public init(identity: GRVProcessIdentityV1) throws {
        guard processIdentityIsValid(identity), identity.workloadDigest.count == 32 else {
            throw ProcessStateFailure.identityMismatch
        }
        self.identity = identity
    }

    /// Consumes the sole start authority and enters starting.
    public mutating func start(identity: GRVProcessIdentityV1) throws {
        try requireIdentity(identity)
        guard !startConsumed, case .absent = state else {
            throw ProcessStateFailure.stateConflict
        }
        startConsumed = true
        state = .starting
    }

    /// Marks that the exact guest workload may now be running.
    public mutating func markRunning(identity: GRVProcessIdentityV1) throws {
        try requireIdentity(identity)
        guard case .starting = state else { throw ProcessStateFailure.stateConflict }
        state = .running
    }

    /// Validates and retains terminal evidence only after running.
    public mutating func retainTerminal(_ value: GRVProcessTerminalReceiptV1) throws {
        guard value.hasIdentity else { throw ProcessStateFailure.invalidTerminal }
        try requireIdentity(value.identity)
        guard case .running = state else { throw ProcessStateFailure.stateConflict }
        var receipt = value
        do {
            receipt.receiptDigest = try processReceiptDigest(receipt)
        } catch {
            throw ProcessStateFailure.invalidTerminal
        }
        state = .terminalRetained(receipt)
    }

    /// Returns exact externally visible status without changing state.
    public func query(identity: GRVProcessIdentityV1) throws -> GRVProcessStatusV1 {
        try requireIdentity(identity)
        var status = GRVProcessStatusV1()
        status.identity = self.identity
        switch state {
        case .absent:
            status.state = .processServiceStateAbsent
        case .starting:
            status.state = .processServiceStateStarting
        case .running:
            status.state = .processServiceStateRunning
        case let .terminalRetained(receipt):
            status.state = .processServiceStateTerminalRetained
            status.terminal = receipt
        }
        return status
    }

    /// Terminates starting/running ownership or proves exact idempotent absence.
    public mutating func terminate(identity: GRVProcessIdentityV1) throws {
        try requireIdentity(identity)
        switch state {
        case .absent, .starting, .running:
            state = .absent
        case .terminalRetained:
            throw ProcessStateFailure.stateConflict
        }
    }

    /// Erases a retained receipt only for its exact digest and identity.
    public mutating func acknowledge(
        identity: GRVProcessIdentityV1,
        receiptDigest: Data
    ) throws {
        try requireIdentity(identity)
        switch state {
        case let .terminalRetained(receipt)
            where receiptDigest.count == 32 && receipt.receiptDigest == receiptDigest:
            state = .absent
        case .terminalRetained:
            throw ProcessStateFailure.identityMismatch
        default:
            throw ProcessStateFailure.stateConflict
        }
    }

    private func requireIdentity(_ candidate: GRVProcessIdentityV1) throws {
        guard candidate == identity else { throw ProcessStateFailure.identityMismatch }
    }
}
