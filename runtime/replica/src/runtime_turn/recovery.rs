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
    /// Prepared-v3 is durable but no Safety decision exists.
    F0SafetyPending,
    /// A Safety decision is durable but admission is not yet preflighted.
    F0Decision,
    /// Prepared-v3 grant is durable but no concrete Sandbox binding exists.
    F0Authorized,
    /// Concrete Sandbox binding exists but preflight is not durable.
    F0SandboxBound,
    /// Exact Sandbox preflight is durable but Started is absent.
    F0Preflighted,
    /// Executor dispatch was crossed without receipt.
    Started,
    /// Trustworthy receipt exists but explicit result is missing.
    Receipt,
    /// Uncertain effect is durably suspended for operator reconciliation.
    Uncertain,
    /// Operator evidence and the matching observation are durable.
    Reconciled,
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
    /// Re-evaluate current Safety policy for the same Prepared-v3 invocation.
    ReevaluateEffectSafety,
    /// Resume grant/binding admission from an already durable F0 decision.
    ResumeEffectAdmission,
    /// Revalidate revisions and dispatch the same preflighted invocation.
    RevalidateAndDispatchEffect,
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

/// Derives the portable recovery positions solely from one verified Turn prefix.
pub fn derive_runtime_recovery(
    turn: &TurnSnapshot,
    max_recoveries: u64,
) -> Result<RuntimeRecoverySnapshot, RuntimeCommandError> {
    if max_recoveries == 0 || turn.facts.is_empty() {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    let execution = latest_execution(turn)?;
    let execution_position = execution_position(turn, &execution)?;
    let model = model_position(turn, &execution)?;
    let effect = effect_position(turn, &execution)?;
    let dispatch_pending = usize::from(model == ModelRecoveryPosition::Started)
        + usize::from(matches!(
            effect,
            EffectRecoveryPosition::Started | EffectRecoveryPosition::Receipt
        ));
    if dispatch_pending > 1 {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    let started = turn
        .facts
        .iter()
        .rfind(|fact| {
            fact.kind.as_str() == "execution.started"
                && fact.execution_id.as_ref() == Some(&execution)
        })
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    let recovery_ordinal = payload(started)?
        .get("recovery_ordinal")
        .and_then(Value::as_u64)
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    Ok(RuntimeRecoverySnapshot {
        execution: execution_position,
        model,
        effect,
        recovery_ordinal,
        max_recoveries,
    })
}

fn latest_execution(turn: &TurnSnapshot) -> Result<ExecutionId, RuntimeCommandError> {
    turn.facts
        .iter()
        .rfind(|fact| fact.kind.as_str() == "execution.started")
        .and_then(|fact| fact.execution_id.clone())
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn execution_position(
    turn: &TurnSnapshot,
    execution: &ExecutionId,
) -> Result<ExecutionRecoveryPosition, RuntimeCommandError> {
    let turn_lifecycle = turn
        .facts
        .iter()
        .rfind(|fact| {
            matches!(
                fact.kind.as_str(),
                "turn.started"
                    | "turn.suspended"
                    | "turn.completed"
                    | "turn.stopped"
                    | "turn.failed"
            )
        })
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    let latest = turn
        .facts
        .iter()
        .filter(|fact| fact.execution_id.as_ref() == Some(execution))
        .rfind(|fact| fact.kind.as_str().starts_with("execution."))
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    match (turn_lifecycle.kind.as_str(), latest.kind.as_str()) {
        ("turn.started", "execution.started") => Ok(ExecutionRecoveryPosition::Active),
        ("turn.suspended", "execution.suspended") => Ok(ExecutionRecoveryPosition::Suspended),
        (
            "turn.completed" | "turn.stopped" | "turn.failed",
            "execution.abandoned"
            | "execution.completed"
            | "execution.suspended"
            | "execution.stopped"
            | "execution.failed",
        ) => Ok(ExecutionRecoveryPosition::Terminal),
        _ => Err(RuntimeCommandError::CorruptLedger),
    }
}

fn model_position(
    turn: &TurnSnapshot,
    execution: &ExecutionId,
) -> Result<ModelRecoveryPosition, RuntimeCommandError> {
    let mut requests = BTreeMap::<String, &DurableFact>::new();
    for fact in turn.facts.iter().filter(|fact| {
        fact.execution_id.as_ref() == Some(execution) && fact.model_request_id.is_some()
    }) {
        requests.insert(
            fact.model_request_id.as_ref().unwrap().as_str().to_owned(),
            fact,
        );
    }
    positions(
        requests.values().map(|fact| match fact.kind.as_str() {
            "model.prepared" => Ok(ModelRecoveryPosition::Prepared),
            "model.started" => Ok(ModelRecoveryPosition::Started),
            "model.uncertain" => Ok(ModelRecoveryPosition::Uncertain),
            "model.completed" | "model.rejected" | "model.interrupted" | "model.unavailable" => {
                Ok(ModelRecoveryPosition::Terminal)
            }
            _ => Err(RuntimeCommandError::CorruptLedger),
        }),
        ModelRecoveryPosition::None,
        ModelRecoveryPosition::Terminal,
    )
}

fn effect_position(
    turn: &TurnSnapshot,
    execution: &ExecutionId,
) -> Result<EffectRecoveryPosition, RuntimeCommandError> {
    let mut tools = BTreeMap::<String, &DurableFact>::new();
    let mut interactions = BTreeMap::<String, bool>::new();
    for fact in turn
        .facts
        .iter()
        .filter(|fact| fact.execution_id.as_ref() == Some(execution))
    {
        if let Some(tool) = &fact.tool_invocation_id {
            if fact.kind.as_str().starts_with("effect.")
                || matches!(
                    fact.kind.as_str(),
                    "safety.decided" | "sandbox.bound" | "sandbox.preflighted"
                )
            {
                tools.insert(tool.as_str().to_owned(), fact);
            }
        }
        if matches!(
            fact.kind.as_str(),
            "interaction.requested" | "interaction.resolved" | "interaction.cancelled"
        ) {
            let interaction = payload(fact)?;
            let id = interaction
                .get("interaction_id")
                .and_then(Value::as_str)
                .ok_or(RuntimeCommandError::CorruptLedger)?;
            interactions.insert(id.to_owned(), fact.kind.as_str() == "interaction.requested");
        }
    }
    if interactions.values().filter(|pending| **pending).count() > 1 {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    if interactions.values().any(|pending| *pending) {
        return Ok(EffectRecoveryPosition::InteractionRequested);
    }
    positions(
        tools.values().map(|fact| match fact.kind.as_str() {
            "effect.prepared" if fact.schema_version == 3 => {
                Ok(EffectRecoveryPosition::F0SafetyPending)
            }
            "effect.prepared" => Ok(EffectRecoveryPosition::Prepared),
            "safety.decided" => Ok(EffectRecoveryPosition::F0Decision),
            "effect.authorized" if is_f0_tool(turn, fact) => {
                Ok(EffectRecoveryPosition::F0Authorized)
            }
            "effect.authorized" => Ok(EffectRecoveryPosition::Prepared),
            "sandbox.bound" => Ok(EffectRecoveryPosition::F0SandboxBound),
            "sandbox.preflighted" => Ok(EffectRecoveryPosition::F0Preflighted),
            "effect.started" => Ok(EffectRecoveryPosition::Started),
            "effect.receipt" => Ok(EffectRecoveryPosition::Receipt),
            "effect.uncertain" => Ok(EffectRecoveryPosition::Uncertain),
            "effect.reconciled" => Ok(EffectRecoveryPosition::Reconciled),
            "effect.observation"
                if fact.tool_invocation_id.as_ref().is_some_and(|tool| {
                    turn.facts.iter().any(|candidate| {
                        candidate.tool_invocation_id.as_ref() == Some(tool)
                            && candidate.kind.as_str() == "effect.reconciled"
                    })
                }) =>
            {
                Ok(EffectRecoveryPosition::Reconciled)
            }
            "effect.completed" | "effect.failed" | "effect.denied" | "effect.observation" => {
                Ok(EffectRecoveryPosition::Terminal)
            }
            _ => Err(RuntimeCommandError::CorruptLedger),
        }),
        EffectRecoveryPosition::None,
        EffectRecoveryPosition::Terminal,
    )
}

fn is_f0_tool(turn: &TurnSnapshot, fact: &DurableFact) -> bool {
    fact.tool_invocation_id.as_ref().is_some_and(|tool| {
        turn.facts.iter().any(|candidate| {
            candidate.tool_invocation_id.as_ref() == Some(tool)
                && candidate.kind.as_str() == "effect.prepared"
                && candidate.schema_version == 3
        })
    })
}

fn positions<T: Copy + Eq>(
    values: impl Iterator<Item = Result<T, RuntimeCommandError>>,
    none: T,
    terminal: T,
) -> Result<T, RuntimeCommandError> {
    let values: Vec<_> = values.collect::<Result<_, _>>()?;
    if values.is_empty() {
        return Ok(none);
    }
    let pending = values
        .iter()
        .filter(|value| **value != none && **value != terminal)
        .count();
    if pending > 1 {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    Ok(values
        .into_iter()
        .find(|value| *value != none && *value != terminal)
        .unwrap_or(terminal))
}

fn payload(fact: &DurableFact) -> Result<serde_json::Map<String, Value>, RuntimeCommandError> {
    serde_json::from_str::<Value>(fact.payload.as_json())
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(RuntimeCommandError::CorruptLedger)
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
        | (
            Execution::Suspended,
            _,
            Effect::InteractionRequested | Effect::Uncertain | Effect::Reconciled,
        ) => Action::AwaitContinuation,
        (Execution::Suspended, _, _) => Action::FailCorruptLedger,
        (Execution::Active, _, _) if snapshot.recovery_ordinal >= snapshot.max_recoveries => {
            Action::FailRecoveryBound
        }
        (Execution::Active, Model::Started, _) => Action::ClassifyModelUncertain,
        (Execution::Active, _, Effect::Started) => Action::ClassifyEffectUncertain,
        (Execution::Active, _, Effect::Receipt) => Action::RecoverReceiptTerminal,
        (Execution::Active, _, Effect::F0SafetyPending) => Action::ReevaluateEffectSafety,
        (Execution::Active, _, Effect::F0Decision | Effect::F0Authorized) => {
            Action::ResumeEffectAdmission
        }
        (Execution::Active, _, Effect::F0SandboxBound | Effect::F0Preflighted) => {
            Action::RevalidateAndDispatchEffect
        }
        (
            Execution::Active,
            Model::None | Model::Prepared | Model::Terminal,
            Effect::None | Effect::Prepared | Effect::Terminal,
        ) => Action::AbandonAndRestart,
        _ => Action::FailCorruptLedger,
    }
}
use std::collections::BTreeMap;

use garive_ledger::{DurableFact, ExecutionId, TurnSnapshot};
use serde_json::Value;

use super::RuntimeCommandError;
