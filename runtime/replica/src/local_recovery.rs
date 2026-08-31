//! Startup recovery coordinator for local model-only Turn dispatches.

use futures::executor::block_on;
use garive_core::{AgentOutcome, ExecutionReport, SuspensionReason, UsageSummary};
use garive_ledger::{DurableFact, TurnSnapshot};
use garive_llm::TokenCount;
use garive_tools::{
    EffectReceipt, GovernedToolResult, GrantId, InteractionKind, ReceiptId, SuspensionRequirement,
    TerminalClassification, ToolInvocationId, T1_PROCESS_RUN,
};
use serde_json::Value;

use crate::runtime_turn::recovered_completed_iterations;
use crate::{
    derive_knowledge_recovery, derive_runtime_recovery, plan_core_terminal,
    plan_knowledge_recovery_uncertain, plan_recovery_action_facts, plan_recovery_restart,
    recover_f0_prepared_with_port, select_runtime_recovery, CommittedTurn, CoreTerminalContext,
    EffectiveRuntimeLimits, ExecutorRecoveryRequest, GovernedEffectConfig, KnowledgeRecoveryAction,
    KnowledgeRecoveryContext, LocalGovernedExecutionFactory, RecoveryRestartCommand,
    RuntimeCommandId, RuntimeRecoveryAction, SqliteGovernedEffectPort, SqliteLedger,
};

/// Stable secret-free local restart failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRecoveryError {
    /// Explicit recovery limits or clock values are invalid.
    InvalidConfiguration,
    /// SQLite state could not be opened, verified or committed.
    DurabilityUnavailable,
    /// Durable positions cannot form an accepted recovery action.
    CorruptRecoveryState,
    /// Prepared-v3 recovery requires configured Safety and Sandbox brokers.
    F0GovernanceRequired,
    /// Configured F0 dependencies could not prove or finish the same invocation.
    F0RecoveryFailed,
}
impl LocalRecoveryError {
    /// Returns the stable operational code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid_composition",
            Self::DurabilityUnavailable => "durability_unavailable",
            Self::CorruptRecoveryState => "reconstruction_failed",
            Self::F0GovernanceRequired => "f0_governance_required",
            Self::F0RecoveryFailed => "f0_recovery_failed",
        }
    }
}

/// Bounded result from one startup recovery scan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalRecoveryReport {
    /// Replacement or resumed Executions that still require worker dispatch.
    pub dispatches: Vec<CommittedTurn>,
    /// Number of recoverable Turns inspected by this scan.
    pub processed_turns: usize,
    /// Whether the explicit Turn bound stopped discovery before exhaustion.
    pub has_more: bool,
}

/// Applies C6 recovery to every local Session and returns replacement dispatches.
pub fn recover_local_dispatches(
    ledger: &mut SqliteLedger,
    max_recoveries: u64,
    recorded_at: &str,
) -> Result<Vec<CommittedTurn>, LocalRecoveryError> {
    recover_local_dispatches_inner(ledger, max_recoveries, usize::MAX, recorded_at, None)
        .map(|report| report.dispatches)
}

/// Applies at most `max_turns` model-only recovery decisions in one scan.
pub fn recover_local_dispatches_bounded(
    ledger: &mut SqliteLedger,
    max_recoveries: u64,
    max_turns: usize,
    recorded_at: &str,
) -> Result<LocalRecoveryReport, LocalRecoveryError> {
    recover_local_dispatches_inner(ledger, max_recoveries, max_turns, recorded_at, None)
}

/// Applies startup recovery including Prepared-v3 broker continuation.
pub fn recover_local_dispatches_with_f0(
    ledger: &mut SqliteLedger,
    max_recoveries: u64,
    recorded_at: &str,
    factory: &dyn LocalGovernedExecutionFactory,
    max_arguments_bytes: usize,
) -> Result<Vec<CommittedTurn>, LocalRecoveryError> {
    if max_arguments_bytes == 0 {
        return Err(LocalRecoveryError::InvalidConfiguration);
    }
    recover_local_dispatches_inner(
        ledger,
        max_recoveries,
        usize::MAX,
        recorded_at,
        Some((factory, max_arguments_bytes)),
    )
    .map(|report| report.dispatches)
}

/// Applies at most `max_turns` F0-aware recovery decisions in one scan.
pub fn recover_local_dispatches_with_f0_bounded(
    ledger: &mut SqliteLedger,
    max_recoveries: u64,
    max_turns: usize,
    recorded_at: &str,
    factory: &dyn LocalGovernedExecutionFactory,
    max_arguments_bytes: usize,
) -> Result<LocalRecoveryReport, LocalRecoveryError> {
    if max_arguments_bytes == 0 {
        return Err(LocalRecoveryError::InvalidConfiguration);
    }
    recover_local_dispatches_inner(
        ledger,
        max_recoveries,
        max_turns,
        recorded_at,
        Some((factory, max_arguments_bytes)),
    )
}

fn recover_local_dispatches_inner(
    ledger: &mut SqliteLedger,
    max_recoveries: u64,
    max_turns: usize,
    recorded_at: &str,
    f0: Option<(&dyn LocalGovernedExecutionFactory, usize)>,
) -> Result<LocalRecoveryReport, LocalRecoveryError> {
    if max_recoveries == 0 || max_turns == 0 || !canonical_utc(recorded_at) {
        return Err(LocalRecoveryError::InvalidConfiguration);
    }
    let sessions = ledger
        .list_sessions()
        .map_err(|_| LocalRecoveryError::DurabilityUnavailable)?;
    let mut output = Vec::new();
    let mut processed_turns = 0;
    for session_id in sessions {
        let turns = ledger
            .list_recoverable_turns(&session_id)
            .map_err(|_| LocalRecoveryError::DurabilityUnavailable)?;
        for turn_id in turns {
            if processed_turns == max_turns {
                return Ok(LocalRecoveryReport {
                    dispatches: output,
                    processed_turns,
                    has_more: true,
                });
            }
            processed_turns += 1;
            loop {
                let snapshot = ledger
                    .load_turn(&turn_id)
                    .map_err(|_| LocalRecoveryError::DurabilityUnavailable)?;
                match recover_pending_knowledge(
                    ledger,
                    &session_id,
                    &turn_id,
                    &snapshot,
                    recorded_at,
                )? {
                    PendingKnowledgeRecovery::CommittedUncertainty => continue,
                    PendingKnowledgeRecovery::ResumeCurrentExecution => {
                        let execution_id = latest(&snapshot, "execution.started")?
                            .execution_id
                            .clone()
                            .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
                        output.push(committed_turn(
                            &snapshot,
                            session_id.clone(),
                            turn_id.clone(),
                            execution_id,
                            snapshot.session_version,
                            snapshot.through_position,
                        )?);
                        break;
                    }
                    PendingKnowledgeRecovery::None => {}
                }
                let recovery = derive_runtime_recovery(&snapshot, max_recoveries)
                    .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?;
                let action = select_runtime_recovery(recovery);
                match action {
                    RuntimeRecoveryAction::AbandonAndRestart => {
                        let plan = restart_plan(&snapshot, recorded_at)?;
                        let execution_id = plan
                            .execution_id
                            .clone()
                            .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
                        let committed = ledger
                            .commit(session_id.clone(), snapshot.session_version, plan.facts)
                            .map_err(|_| LocalRecoveryError::DurabilityUnavailable)?;
                        output.push(committed_turn(
                            &snapshot,
                            session_id.clone(),
                            plan.turn_id,
                            execution_id,
                            committed.session_version,
                            *committed
                                .positions
                                .last()
                                .ok_or(LocalRecoveryError::CorruptRecoveryState)?,
                        )?);
                        break;
                    }
                    RuntimeRecoveryAction::ClassifyEffectUncertain => {
                        if let Some((factory, _)) = f0 {
                            reconcile_lost_process(&session_id, &turn_id, &snapshot, factory)?;
                        }
                        commit_action(ledger, &session_id, &snapshot, action, recorded_at)?;
                        break;
                    }
                    RuntimeRecoveryAction::ClassifyModelUncertain
                    | RuntimeRecoveryAction::FailRecoveryBound => {
                        commit_action(ledger, &session_id, &snapshot, action, recorded_at)?;
                        break;
                    }
                    RuntimeRecoveryAction::RecoverReceiptTerminal => {
                        let Some((factory, _)) = f0 else {
                            return Err(LocalRecoveryError::F0GovernanceRequired);
                        };
                        acknowledge_recovered_receipt(&session_id, &turn_id, &snapshot, factory)?;
                        commit_action(ledger, &session_id, &snapshot, action, recorded_at)?;
                    }
                    RuntimeRecoveryAction::AwaitContinuation
                    | RuntimeRecoveryAction::ReturnCommittedTerminal => break,
                    RuntimeRecoveryAction::ReevaluateEffectSafety
                    | RuntimeRecoveryAction::ResumeEffectAdmission
                    | RuntimeRecoveryAction::RevalidateAndDispatchEffect => {
                        let Some((factory, max_arguments_bytes)) = f0 else {
                            return Err(LocalRecoveryError::F0GovernanceRequired);
                        };
                        resume_f0(
                            ledger,
                            &session_id,
                            &turn_id,
                            &snapshot,
                            factory,
                            max_arguments_bytes,
                            recorded_at,
                        )?;
                    }
                    RuntimeRecoveryAction::FailCorruptLedger => {
                        return Err(LocalRecoveryError::CorruptRecoveryState)
                    }
                }
            }
        }
    }
    Ok(LocalRecoveryReport {
        dispatches: output,
        processed_turns,
        has_more: false,
    })
}

enum PendingKnowledgeRecovery {
    None,
    ResumeCurrentExecution,
    CommittedUncertainty,
}

fn recover_pending_knowledge(
    ledger: &mut SqliteLedger,
    session_id: &garive_ledger::SessionId,
    turn_id: &garive_ledger::TurnId,
    snapshot: &TurnSnapshot,
    recorded_at: &str,
) -> Result<PendingKnowledgeRecovery, LocalRecoveryError> {
    let execution_id = latest(snapshot, "execution.started")?
        .execution_id
        .clone()
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let mut request_ids = snapshot
        .facts
        .iter()
        .filter(|fact| {
            fact.kind.as_str() == "knowledge.requested"
                && fact.execution_id.as_ref() == Some(&execution_id)
        })
        .map(|fact| payload(fact).and_then(|value| text(&value, "request_id").map(str::to_owned)))
        .collect::<Result<Vec<_>, _>>()?;
    request_ids.sort();
    if request_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(LocalRecoveryError::CorruptRecoveryState);
    }
    let mut facts = Vec::new();
    let mut resume = false;
    for request_id in request_ids {
        let context = KnowledgeRecoveryContext {
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            execution_id: execution_id.clone(),
            through_position: snapshot.through_position,
            request_id,
        };
        match derive_knowledge_recovery(ledger, &context)
            .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?
        {
            KnowledgeRecoveryAction::ClassifyUncertain { .. } => facts.push(
                plan_knowledge_recovery_uncertain(ledger, &context, recorded_at)
                    .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?,
            ),
            KnowledgeRecoveryAction::RedispatchSameRequest { .. } => resume = true,
            KnowledgeRecoveryAction::ReturnTerminal {
                completed: true, ..
            } => resume = true,
            KnowledgeRecoveryAction::ReturnTerminal {
                completed: false, ..
            } => {}
        }
    }
    if !facts.is_empty() {
        ledger
            .commit(session_id.clone(), snapshot.session_version, facts)
            .map_err(|_| LocalRecoveryError::DurabilityUnavailable)?;
        return Ok(PendingKnowledgeRecovery::CommittedUncertainty);
    }
    Ok(if resume {
        PendingKnowledgeRecovery::ResumeCurrentExecution
    } else {
        PendingKnowledgeRecovery::None
    })
}

fn reconcile_lost_process(
    session_id: &garive_ledger::SessionId,
    turn_id: &garive_ledger::TurnId,
    snapshot: &TurnSnapshot,
    factory: &dyn LocalGovernedExecutionFactory,
) -> Result<(), LocalRecoveryError> {
    let started = latest(snapshot, "effect.started")?;
    let invocation = started
        .tool_invocation_id
        .as_ref()
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let prepared = snapshot
        .facts
        .iter()
        .rfind(|fact| {
            fact.kind.as_str() == "effect.prepared"
                && fact.tool_invocation_id.as_ref() == Some(invocation)
        })
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let prepared_value = payload(prepared)?;
    if text(&prepared_value, "tool_name")? != T1_PROCESS_RUN {
        return Ok(());
    }
    if text(&prepared_value, "replay_class")? != "never_replay" {
        return Err(LocalRecoveryError::CorruptRecoveryState);
    }
    let value = payload(started)?;
    let prepared_digest = text(&value, "prepared_digest")?;
    if text(&prepared_value, "prepared_digest")? != prepared_digest {
        return Err(LocalRecoveryError::CorruptRecoveryState);
    }
    let execution_id = started
        .execution_id
        .clone()
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let committed = committed_turn(
        snapshot,
        session_id.clone(),
        turn_id.clone(),
        execution_id,
        snapshot.session_version,
        snapshot.through_position,
    )?;
    let mut governed = factory
        .create(&committed)
        .map_err(|_| LocalRecoveryError::F0RecoveryFailed)?;
    governed
        .executor
        .reconcile_started_loss(ExecutorRecoveryRequest {
            invocation_id: &ToolInvocationId::new(invocation.as_str())
                .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?,
            prepared_digest,
            executor_id: text(&value, "executor_id")?,
            executor_revision: text(&value, "executor_revision")?,
            dispatch_attempt_id: text(&value, "dispatch_attempt_id")?,
        })
        .map_err(|_| LocalRecoveryError::F0RecoveryFailed)
}

fn acknowledge_recovered_receipt(
    session_id: &garive_ledger::SessionId,
    turn_id: &garive_ledger::TurnId,
    snapshot: &TurnSnapshot,
    factory: &dyn LocalGovernedExecutionFactory,
) -> Result<(), LocalRecoveryError> {
    let started = latest(snapshot, "execution.started")?;
    let execution_id = started
        .execution_id
        .clone()
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let source = snapshot
        .facts
        .iter()
        .rfind(|fact| {
            fact.kind.as_str() == "effect.receipt"
                && fact.execution_id.as_ref() == Some(&execution_id)
        })
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let invocation_id = source
        .tool_invocation_id
        .as_ref()
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let value = payload(source)?;
    let evidence = value
        .get("result_or_evidence")
        .and_then(Value::as_object)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let classification = match text(&value, "classification")? {
        "completed" => TerminalClassification::Completed,
        "failed" => TerminalClassification::Failed,
        _ => return Err(LocalRecoveryError::CorruptRecoveryState),
    };
    let receipt = EffectReceipt {
        receipt_id: ReceiptId::new(text(&value, "receipt_id")?)
            .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?,
        invocation_id: ToolInvocationId::new(invocation_id.as_str())
            .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?,
        prepared_digest: text(&value, "prepared_digest")?.into(),
        grant_id: GrantId::new(text(&value, "grant_id")?)
            .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?,
        executor_id: text(&value, "executor_id")?.into(),
        executor_revision: text(&value, "executor_revision")?.into(),
        terminal_classification: classification,
        result_digest: evidence
            .get("digest")
            .and_then(Value::as_str)
            .ok_or(LocalRecoveryError::CorruptRecoveryState)?
            .into(),
    };
    receipt
        .validate()
        .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?;
    let committed = committed_turn(
        snapshot,
        session_id.clone(),
        turn_id.clone(),
        execution_id,
        snapshot.session_version,
        snapshot.through_position,
    )?;
    let mut governed = factory
        .create(&committed)
        .map_err(|_| LocalRecoveryError::F0RecoveryFailed)?;
    governed
        .executor
        .acknowledge_receipt(&receipt.invocation_id, &receipt)
        .map_err(|_| LocalRecoveryError::F0RecoveryFailed)
}

#[allow(clippy::too_many_arguments)]
fn resume_f0(
    ledger: &mut SqliteLedger,
    session_id: &garive_ledger::SessionId,
    turn_id: &garive_ledger::TurnId,
    snapshot: &TurnSnapshot,
    factory: &dyn LocalGovernedExecutionFactory,
    max_arguments_bytes: usize,
    recorded_at: &str,
) -> Result<(), LocalRecoveryError> {
    let started = latest(snapshot, "execution.started")?;
    let execution_id = started
        .execution_id
        .clone()
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let pending = snapshot
        .facts
        .iter()
        .rfind(|fact| {
            fact.execution_id.as_ref() == Some(&execution_id)
                && fact.tool_invocation_id.is_some()
                && matches!(
                    fact.kind.as_str(),
                    "effect.prepared"
                        | "safety.decided"
                        | "effect.authorized"
                        | "sandbox.bound"
                        | "sandbox.preflighted"
                )
        })
        .and_then(|fact| fact.tool_invocation_id.as_ref())
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let committed = committed_turn(
        snapshot,
        session_id.clone(),
        turn_id.clone(),
        execution_id.clone(),
        snapshot.session_version,
        snapshot.through_position,
    )?;
    let mut governed = factory
        .create(&committed)
        .map_err(|_| LocalRecoveryError::F0RecoveryFailed)?;
    let mut f0 = governed.f0;
    let recovered = recover_f0_prepared_with_port(
        snapshot,
        pending.as_str(),
        f0.preparation.as_ref(),
        f0.recovery_content.as_mut(),
        max_arguments_bytes,
    )
    .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?;
    if !governed.capabilities.definitions.iter().any(|definition| {
        definition.name() == recovered.prepared.tool_name()
            && definition.revision() == recovered.prepared.tool_revision()
    }) {
        return Err(LocalRecoveryError::CorruptRecoveryState);
    }
    let completed_iterations = recovered_completed_iterations(snapshot, started)
        .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?;
    let mut port = SqliteGovernedEffectPort::new(
        ledger,
        governed.authority.as_mut(),
        governed.executor.as_mut(),
        GovernedEffectConfig {
            session_id: session_id.clone(),
            expected_session_version: snapshot.session_version,
            initial_through_position: snapshot.through_position,
            turn_id: turn_id.clone(),
            execution_id: execution_id.clone(),
            recorded_at: recorded_at.into(),
        },
    )
    .and_then(|port| port.with_f0_governance(f0.safety.as_mut(), f0.sandbox.as_mut(), f0.context))
    .map_err(|_| LocalRecoveryError::F0RecoveryFailed)?;
    let result = block_on(port.resume_f0(snapshot, recovered))
        .map_err(|_| LocalRecoveryError::F0RecoveryFailed)?;
    let version = port
        .session_version()
        .map_err(|_| LocalRecoveryError::F0RecoveryFailed)?;
    drop(port);
    if let GovernedToolResult::Suspend(SuspensionRequirement::Interaction(interaction)) =
        result.result
    {
        let reason = if interaction.kind == InteractionKind::Approval {
            SuspensionReason::ApprovalRequired
        } else {
            SuspensionReason::ExternalInputRequired
        };
        let terminal = plan_core_terminal(
            &CoreTerminalContext {
                turn_id: turn_id.clone(),
                execution_id,
                recorded_at: recorded_at.into(),
            },
            &ExecutionReport {
                outcome: AgentOutcome::Suspended {
                    reason,
                    partial_items: vec![],
                    last_durable_position: result.through_position,
                    governed_binding: result.suspension_binding,
                },
                completed_iterations: u32::try_from(completed_iterations)
                    .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?,
                usage: UsageSummary {
                    input_tokens: TokenCount::Unknown,
                    output_tokens: TokenCount::Unknown,
                    estimated: true,
                },
            },
        )
        .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?;
        ledger
            .commit(session_id.clone(), version, terminal)
            .map_err(|_| LocalRecoveryError::DurabilityUnavailable)?;
    }
    Ok(())
}

fn commit_action(
    ledger: &mut SqliteLedger,
    session_id: &garive_ledger::SessionId,
    snapshot: &TurnSnapshot,
    action: RuntimeRecoveryAction,
    recorded_at: &str,
) -> Result<(), LocalRecoveryError> {
    let facts = plan_recovery_action_facts(snapshot, action, recorded_at)
        .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?;
    ledger
        .commit(session_id.clone(), snapshot.session_version, facts)
        .map(|_| ())
        .map_err(|_| LocalRecoveryError::DurabilityUnavailable)
}

fn restart_plan(
    snapshot: &TurnSnapshot,
    recorded_at: &str,
) -> Result<crate::PlannedTurn, LocalRecoveryError> {
    let started = latest(snapshot, "execution.started")?;
    let execution_id = started
        .execution_id
        .clone()
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let value = payload(started)?;
    let recovery_ordinal = number(&value, "recovery_ordinal")?
        .checked_add(1)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let limits = value
        .get("limits")
        .and_then(Value::as_object)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let command = RecoveryRestartCommand {
        recovery_id: RuntimeCommandId::new(format!(
            "local-recovery-{}-{recovery_ordinal}",
            snapshot
                .facts
                .first()
                .and_then(|fact| fact.turn_id.as_ref())
                .ok_or(LocalRecoveryError::CorruptRecoveryState)?
                .as_str()
        ))
        .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?,
        turn_id: snapshot
            .facts
            .first()
            .and_then(|fact| fact.turn_id.clone())
            .ok_or(LocalRecoveryError::CorruptRecoveryState)?,
        lost_execution_id: execution_id,
        snapshot_digest: text(&value, "snapshot_digest")?.to_owned(),
        last_safe_position: snapshot.through_position,
        completed_iterations: recovered_completed_iterations(snapshot, started)
            .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?,
        recovery_ordinal,
        limits: EffectiveRuntimeLimits {
            max_iterations: number_map(limits, "max_iterations")?,
            max_input_tokens: optional_number_map(limits, "max_input_tokens")?,
            max_output_tokens: optional_number_map(limits, "max_output_tokens")?,
            deadline_budget_ms: optional_number_map(limits, "deadline_budget_ms")?,
        },
        recorded_at: recorded_at.to_owned(),
    };
    plan_recovery_restart(&command).map_err(|_| LocalRecoveryError::CorruptRecoveryState)
}

fn committed_turn(
    snapshot: &TurnSnapshot,
    session_id: garive_ledger::SessionId,
    turn_id: garive_ledger::TurnId,
    execution_id: garive_ledger::ExecutionId,
    session_version: u64,
    committed_position: u64,
) -> Result<CommittedTurn, LocalRecoveryError> {
    let started = latest(snapshot, "turn.started")?;
    let value = payload(started)?;
    Ok(CommittedTurn {
        session_id,
        turn_id,
        execution_id,
        definition_id: text(&value, "definition_id")?.to_owned(),
        definition_revision: text(&value, "definition_revision")?.to_owned(),
        snapshot_digest: text(&value, "snapshot_digest")?.to_owned(),
        session_version,
        committed_position,
    })
}

fn latest<'a>(
    snapshot: &'a TurnSnapshot,
    kind: &str,
) -> Result<&'a DurableFact, LocalRecoveryError> {
    snapshot
        .facts
        .iter()
        .rfind(|fact| fact.kind.as_str() == kind)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)
}

fn payload(fact: &DurableFact) -> Result<Value, LocalRecoveryError> {
    serde_json::from_str(fact.payload.as_json())
        .map_err(|_| LocalRecoveryError::CorruptRecoveryState)
}

fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, LocalRecoveryError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)
}

fn number(value: &Value, name: &str) -> Result<u64, LocalRecoveryError> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)
}

fn number_map(
    value: &serde_json::Map<String, Value>,
    name: &str,
) -> Result<u64, LocalRecoveryError> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)
}

fn optional_number_map(
    value: &serde_json::Map<String, Value>,
    name: &str,
) -> Result<Option<u64>, LocalRecoveryError> {
    value
        .get(name)
        .map(|value| {
            value
                .as_u64()
                .ok_or(LocalRecoveryError::CorruptRecoveryState)
        })
        .transpose()
}

fn canonical_utc(value: &str) -> bool {
    use chrono::{DateTime, SecondsFormat, Utc};
    DateTime::parse_from_rfc3339(value).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == value
    })
}
