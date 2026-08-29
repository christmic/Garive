/// Durable Execution position considered by Runtime recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionRecoveryPosition {
    /// Kernel invocation was active when the process disappeared.
    Active,
    /// Turn is durably suspended for continuation.
    Suspended,
    /// Turn/Execution already reached a durable terminal.
    Terminal,
}

/// Most advanced model lifecycle position in the lost Execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelRecoveryPosition {
    /// No model request exists.
    None,
    /// Request is durable but dispatch was not crossed.
    Prepared,
    /// Provider dispatch was crossed without terminal classification.
    Started,
    /// Uncertain model dispatch is durably suspended for controlled continuation.
    Uncertain,
    /// Model lifecycle is terminal.
    Terminal,
}

/// Most advanced effect/interaction position in the lost Execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectRecoveryPosition {
    /// No effect exists.
    None,
    /// Effect is durable but dispatch was not crossed.
    Prepared,
    /// Executor dispatch was crossed without receipt.
    Started,
    /// Trustworthy receipt exists but explicit result is missing.
    Receipt,
    /// Uncertain effect is durably suspended for operator reconciliation.
    Uncertain,
    /// Interaction is durably awaiting continuation.
    InteractionRequested,
    /// Effect lifecycle is terminal.
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Minimal durable positions required to select one restart action.
pub struct RuntimeRecoverySnapshot {
    /// Execution lifecycle position.
    pub execution: ExecutionRecoveryPosition,
    /// Model lifecycle position.
    pub model: ModelRecoveryPosition,
    /// Effect/interaction lifecycle position.
    pub effect: EffectRecoveryPosition,
    /// Number of prior Runtime abandon/restart cycles.
    pub recovery_ordinal: u64,
    /// Non-zero maximum allowed recovery cycles.
    pub max_recoveries: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Unique fail-closed action selected for one durable recovery snapshot.
pub enum RuntimeRecoveryAction {
    /// Abandon the lost invocation and start a fresh Execution.
    AbandonAndRestart,
    /// Append `model.uncertain`; never fabricate a model terminal.
    ClassifyModelUncertain,
    /// Append `effect.uncertain`; never blindly replay the effect.
    ClassifyEffectUncertain,
    /// Recover the trustworthy receipt into an explicit effect terminal.
    RecoverReceiptTerminal,
    /// Keep the Turn suspended until typed continuation arrives.
    AwaitContinuation,
    /// Return the already committed terminal without appending another.
    ReturnCommittedTerminal,
    /// Stop because the frozen recovery bound is exhausted.
    FailRecoveryBound,
    /// Durable positions are mutually inconsistent.
    FailCorruptLedger,
}

/// Selects the only admissible C6 restart action without performing I/O.
pub fn select_runtime_recovery(snapshot: RuntimeRecoverySnapshot) -> RuntimeRecoveryAction {
    use EffectRecoveryPosition as Effect;
    use ExecutionRecoveryPosition as Execution;
    use ModelRecoveryPosition as Model;
    use RuntimeRecoveryAction as Action;

    if snapshot.max_recoveries == 0 {
        return Action::FailCorruptLedger;
    }
    match (snapshot.execution, snapshot.model, snapshot.effect) {
        (Execution::Terminal, _, _) => Action::ReturnCommittedTerminal,
        (Execution::Suspended, Model::Uncertain, _)
        | (Execution::Suspended, _, Effect::InteractionRequested | Effect::Uncertain) => {
            Action::AwaitContinuation
        }
        (Execution::Suspended, _, _) => Action::FailCorruptLedger,
        (Execution::Active, _, _) if snapshot.recovery_ordinal >= snapshot.max_recoveries => {
            Action::FailRecoveryBound
        }
        (Execution::Active, Model::Started, _) => Action::ClassifyModelUncertain,
        (Execution::Active, _, Effect::Started) => Action::ClassifyEffectUncertain,
        (Execution::Active, _, Effect::Receipt) => Action::RecoverReceiptTerminal,
        (
            Execution::Active,
            Model::None | Model::Prepared | Model::Terminal,
            Effect::None | Effect::Prepared | Effect::Terminal,
        ) => Action::AbandonAndRestart,
        _ => Action::FailCorruptLedger,
    }
}
