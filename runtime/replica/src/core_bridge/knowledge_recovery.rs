use std::collections::BTreeSet;

use garive_ledger::{ExecutionId, FactDraft, FactKind, SessionId, TurnId};
use serde_json::Value;

use crate::{DurableExecutionError, RuntimeCommandError, SqliteLedger};

use super::{
    knowledge_lifecycle::plan_knowledge_failed_binding, KnowledgeFailurePhase,
    KnowledgeFailureReason, KnowledgeLifecycleContext,
};

/// Exact durable scope used to reconstruct one Knowledge request lifecycle.
pub struct KnowledgeRecoveryContext {
    /// Session containing the request facts.
    pub session_id: SessionId,
    /// Turn owning the request.
    pub turn_id: TurnId,
    /// Execution owning the request.
    pub execution_id: ExecutionId,
    /// Fixed inclusive Session prefix to inspect.
    pub through_position: u64,
    /// Logical Knowledge request identity.
    pub request_id: String,
}

/// Safe Runtime action derived solely from one verified SQLite fact prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KnowledgeRecoveryAction {
    /// Requested is durable and no connector boundary was recorded.
    RedispatchSameRequest {
        /// Canonical request digest that Host must rebind.
        request_digest: String,
        /// Fixed request prefix required to reconstruct the exact request.
        request_through_position: u64,
    },
    /// One attempt may have crossed the connector boundary.
    ClassifyUncertain {
        /// Canonical request digest that Host must rebind.
        request_digest: String,
        /// Most recent durable dispatch-attempt identity.
        dispatch_attempt_id: String,
    },
    /// A trustworthy terminal fact must be returned without connector dispatch.
    ReturnTerminal {
        /// Canonical request digest bound by the terminal.
        request_digest: String,
        /// Exact durable position of the terminal fact.
        terminal_position: u64,
        /// Fixed request prefix required to reconstruct the exact request.
        request_through_position: u64,
        /// Whether the terminal is `knowledge.completed`.
        completed: bool,
    },
}

/// Reconstructs the K0 crash decision from a verified real SQLite prefix.
pub fn derive_knowledge_recovery(
    ledger: &SqliteLedger,
    context: &KnowledgeRecoveryContext,
) -> Result<KnowledgeRecoveryAction, DurableExecutionError> {
    if context.through_position == 0 || context.request_id.is_empty() {
        return Err(invalid());
    }
    let kinds = [
        "knowledge.requested",
        "knowledge.dispatched",
        "knowledge.completed",
        "knowledge.failed",
    ]
    .into_iter()
    .map(FactKind::new)
    .collect::<Result<BTreeSet<_>, _>>()
    .map_err(|_| invalid())?;
    let facts = ledger
        .read_facts(
            &context.session_id,
            0,
            context.through_position,
            Some(&kinds),
        )
        .map_err(DurableExecutionError::Ledger)?;
    let mut selected = Vec::new();
    for fact in facts {
        if fact.turn_id.as_ref() != Some(&context.turn_id)
            || fact.execution_id.as_ref() != Some(&context.execution_id)
        {
            continue;
        }
        let payload: Value = serde_json::from_str(fact.payload.as_json()).map_err(|_| corrupt())?;
        if payload.get("request_id").and_then(Value::as_str) == Some(&context.request_id) {
            selected.push((fact, payload));
        }
    }
    let Some((requested, requested_payload)) = selected.first() else {
        return Err(invalid());
    };
    if requested.kind.as_str() != "knowledge.requested"
        || selected
            .iter()
            .skip(1)
            .any(|(fact, _)| fact.kind.as_str() == "knowledge.requested")
    {
        return Err(corrupt());
    }
    let request_digest = digest(requested_payload)?;
    let request_through_position = requested_payload
        .get("through_position")
        .and_then(Value::as_u64)
        .filter(|position| *position > 0)
        .ok_or_else(corrupt)?;
    let mut attempts = BTreeSet::new();
    let mut last_attempt = None;
    let mut terminal = None;
    for (fact, payload) in selected.iter().skip(1) {
        if digest(payload)? != request_digest || terminal.is_some() {
            return Err(corrupt());
        }
        match fact.kind.as_str() {
            "knowledge.dispatched" => {
                let attempt = payload
                    .get("dispatch_attempt_id")
                    .and_then(Value::as_str)
                    .ok_or_else(corrupt)?;
                if !attempts.insert(attempt.to_owned()) {
                    return Err(corrupt());
                }
                last_attempt = Some(attempt.to_owned());
            }
            "knowledge.completed" if last_attempt.is_some() => {
                terminal = Some((fact.position, true));
            }
            "knowledge.failed" => {
                validate_failure(payload, last_attempt.is_some())?;
                terminal = Some((fact.position, false));
            }
            _ => return Err(corrupt()),
        }
    }
    if let Some((terminal_position, completed)) = terminal {
        Ok(KnowledgeRecoveryAction::ReturnTerminal {
            request_digest,
            terminal_position,
            request_through_position,
            completed,
        })
    } else if let Some(dispatch_attempt_id) = last_attempt {
        Ok(KnowledgeRecoveryAction::ClassifyUncertain {
            request_digest,
            dispatch_attempt_id,
        })
    } else {
        Ok(KnowledgeRecoveryAction::RedispatchSameRequest {
            request_digest,
            request_through_position,
        })
    }
}

/// Plans one durable uncertain terminal for a dispatch that lacked a terminal.
pub fn plan_knowledge_recovery_uncertain(
    ledger: &SqliteLedger,
    context: &KnowledgeRecoveryContext,
    recorded_at: &str,
) -> Result<FactDraft, DurableExecutionError> {
    let KnowledgeRecoveryAction::ClassifyUncertain { request_digest, .. } =
        derive_knowledge_recovery(ledger, context)?
    else {
        return Err(invalid());
    };
    plan_knowledge_failed_binding(
        &KnowledgeLifecycleContext {
            turn_id: context.turn_id.clone(),
            execution_id: context.execution_id.clone(),
            recorded_at: recorded_at.to_owned(),
        },
        &context.request_id,
        &request_digest,
        KnowledgeFailurePhase::Dispatched,
        KnowledgeFailureReason::Uncertain,
        None,
    )
    .map_err(DurableExecutionError::Command)
}

fn validate_failure(payload: &Value, dispatched: bool) -> Result<(), DurableExecutionError> {
    let phase = payload
        .get("phase")
        .and_then(Value::as_str)
        .ok_or_else(corrupt)?;
    match phase {
        "pre_dispatch" if !dispatched => Ok(()),
        "dispatched" | "response_validation" if dispatched => Ok(()),
        _ => Err(corrupt()),
    }
}

fn digest(payload: &Value) -> Result<String, DurableExecutionError> {
    payload
        .get("request_digest")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(corrupt)
}

fn invalid() -> DurableExecutionError {
    DurableExecutionError::Command(RuntimeCommandError::InvalidCommand)
}

fn corrupt() -> DurableExecutionError {
    DurableExecutionError::Command(RuntimeCommandError::InvariantViolation)
}
