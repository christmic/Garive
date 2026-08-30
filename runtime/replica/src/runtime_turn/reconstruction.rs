use garive_ledger::{CanonicalPayload, ExecutionId, TurnSnapshot};
use garive_tools::validate_portable_value_schema;
use serde_json::{Map, Value};

use super::types::{
    DelegationContinuation, EffectiveRuntimeLimits, InteractionContinuation, InteractionExpiry,
    ReconciliationTarget, RuntimeCommandError, RuntimeSuspensionKind, SuspendedTurnState,
};

/// Reconstructs a resumable Turn exclusively from one verified fixed Ledger prefix.
pub fn reconstruct_suspended_turn(
    snapshot: &TurnSnapshot,
) -> Result<SuspendedTurnState, RuntimeCommandError> {
    let turn_id = snapshot
        .facts
        .first()
        .and_then(|fact| fact.turn_id.clone())
        .ok_or(RuntimeCommandError::TurnNotResumable)?;
    let session_id = snapshot
        .facts
        .first()
        .map(|fact| fact.session_id.clone())
        .ok_or(RuntimeCommandError::TurnNotResumable)?;
    let started = snapshot
        .facts
        .iter()
        .find(|fact| fact.kind.as_str() == "turn.started")
        .ok_or(RuntimeCommandError::TurnNotResumable)?;
    let suspended = snapshot
        .facts
        .iter()
        .rfind(|fact| fact.kind.as_str() == "turn.suspended")
        .ok_or(RuntimeCommandError::TurnNotResumable)?;
    if snapshot
        .facts
        .iter()
        .filter(|fact| fact.position > suspended.position)
        .any(|fact| {
            matches!(
                fact.kind.as_str(),
                "turn.started" | "turn.completed" | "turn.stopped" | "turn.failed"
            )
        })
    {
        return Err(RuntimeCommandError::TurnNotResumable);
    }
    let suspended_payload = payload(suspended)?;
    let execution_id = ExecutionId::try_from(text(&suspended_payload, "execution_id")?)
        .map_err(|_| RuntimeCommandError::CorruptLedger)?;
    let execution_start = snapshot
        .facts
        .iter()
        .rfind(|fact| {
            fact.kind.as_str() == "execution.started"
                && fact.execution_id.as_ref() == Some(&execution_id)
        })
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    let execution_payload = payload(execution_start)?;
    let execution_suspended = snapshot
        .facts
        .iter()
        .rfind(|fact| {
            fact.kind.as_str() == "execution.suspended"
                && fact.execution_id.as_ref() == Some(&execution_id)
        })
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    let execution_suspended_payload = payload(execution_suspended)?;
    if text(&execution_suspended_payload, "suspension_id")?
        != text(&suspended_payload, "suspension_id")?
        || text(&execution_suspended_payload, "reason")? != text(&suspended_payload, "reason")?
    {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    let started_payload = payload(started)?;
    let suspension_kind = RuntimeSuspensionKind::parse(text(&suspended_payload, "reason")?)?;
    let interaction = pending_interaction(snapshot, &execution_id, &suspended_payload)?;
    let reconciliation = reconciliation_target(snapshot, &execution_id)?;
    let delegation = delegation_target(snapshot, text(&suspended_payload, "suspension_id")?)?;
    if (suspension_kind == RuntimeSuspensionKind::ApprovalRequired && interaction.is_none())
        || (interaction.is_some()
            && !matches!(
                suspension_kind,
                RuntimeSuspensionKind::ApprovalRequired
                    | RuntimeSuspensionKind::ExternalInputRequired
            ))
        || (suspension_kind == RuntimeSuspensionKind::OperatorReconciliation)
            != reconciliation.is_some()
        || (suspension_kind == RuntimeSuspensionKind::DelegationPending) != delegation.is_some()
        || (delegation.is_some() && (interaction.is_some() || reconciliation.is_some()))
    {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    Ok(SuspendedTurnState {
        session_id,
        session_version: snapshot.session_version,
        turn_id,
        suspension_id: text(&suspended_payload, "suspension_id")?.to_owned(),
        suspension_kind,
        interaction,
        reconciliation,
        delegation,
        agent_instance_id: identity(text(&started_payload, "agent_instance_id")?)?,
        definition_id: identity(text(&started_payload, "definition_id")?)?,
        definition_revision: identity(text(&started_payload, "definition_revision")?)?,
        snapshot_digest: text(&started_payload, "snapshot_digest")?.to_owned(),
        trusted_input_digest: text(&started_payload, "trusted_input_digest")?.to_owned(),
        through_position: snapshot.through_position,
        completed_iterations: unsigned(&execution_suspended_payload, "completed_iterations")?,
        recovery_ordinal: unsigned(&execution_payload, "recovery_ordinal")?,
        limits: limits(&execution_payload)?,
    })
}

fn delegation_target(
    snapshot: &TurnSnapshot,
    suspension_id: &str,
) -> Result<Option<DelegationContinuation>, RuntimeCommandError> {
    let starts: Vec<_> = snapshot
        .facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "delegation.child_started")
        .filter_map(|fact| payload(fact).ok().map(|value| (fact, value)))
        .filter(|(_, value)| {
            value.get("suspension_id").and_then(Value::as_str) == Some(suspension_id)
        })
        .collect();
    if starts.len() > 1 {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    let Some((started_fact, started)) = starts.first() else {
        return Ok(None);
    };
    let delegation_id = text(started, "delegation_id")?;
    let grant_id = text(started, "grant_id")?;
    let requested: Vec<_> = snapshot
        .facts
        .iter()
        .filter(|fact| {
            fact.position < started_fact.position && fact.kind.as_str() == "delegation.requested"
        })
        .filter_map(|fact| payload(fact).ok())
        .filter(|value| value.get("delegation_id").and_then(Value::as_str) == Some(delegation_id))
        .collect();
    let authorized: Vec<_> = snapshot
        .facts
        .iter()
        .filter(|fact| {
            fact.position < started_fact.position && fact.kind.as_str() == "delegation.authorized"
        })
        .filter_map(|fact| payload(fact).ok())
        .filter(|value| {
            value.get("delegation_id").and_then(Value::as_str) == Some(delegation_id)
                && value.get("grant_id").and_then(Value::as_str) == Some(grant_id)
        })
        .collect();
    if requested.len() != 1
        || authorized.len() != 1
        || text(&authorized[0], "intent_digest")? != text(&requested[0], "intent_digest")?
    {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    let terminals: Vec<_> = snapshot
        .facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "delegation.child_terminal")
        .filter_map(|fact| payload(fact).ok())
        .filter(|value| value.get("delegation_id").and_then(Value::as_str) == Some(delegation_id))
        .collect();
    if terminals.len() > 1 {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    let (result_id, result_digest) = terminals
        .first()
        .map(|value| {
            if text(value, "grant_id")? != grant_id
                || text(value, "suspension_id")? != suspension_id
            {
                return Err(RuntimeCommandError::CorruptLedger);
            }
            Ok((
                text(value, "result_id")?.to_owned(),
                text(value, "result_digest")?.to_owned(),
            ))
        })
        .transpose()?
        .map_or((None, None), |(id, digest)| (Some(id), Some(digest)));
    let observed = match (&result_id, &result_digest) {
        (Some(id), Some(digest)) => {
            snapshot
                .facts
                .iter()
                .filter(|fact| fact.kind.as_str() == "delegation.observed")
                .filter_map(|fact| payload(fact).ok())
                .filter(|value| {
                    value.get("delegation_id").and_then(Value::as_str) == Some(delegation_id)
                        && value.get("grant_id").and_then(Value::as_str) == Some(grant_id)
                        && value.get("result_id").and_then(Value::as_str) == Some(id.as_str())
                        && value.get("result_digest").and_then(Value::as_str)
                            == Some(digest.as_str())
                        && value.get("suspension_id").and_then(Value::as_str) == Some(suspension_id)
                })
                .count()
                == 1
        }
        _ => false,
    };
    Ok(Some(DelegationContinuation {
        delegation_id: delegation_id.to_owned(),
        intent_digest: text(&requested[0], "intent_digest")?.to_owned(),
        grant_id: grant_id.to_owned(),
        child_agent_instance_id: text(started, "child_agent_instance_id")?.to_owned(),
        child_turn_id: identity(text(started, "child_turn_id")?)?,
        result_id,
        result_digest,
        observed,
    }))
}

fn pending_interaction(
    snapshot: &TurnSnapshot,
    execution_id: &ExecutionId,
    suspended: &Map<String, Value>,
) -> Result<Option<InteractionContinuation>, RuntimeCommandError> {
    let suspension_id = text(suspended, "suspension_id")?;
    let mut pending = Vec::new();
    for requested in snapshot.facts.iter().filter(|fact| {
        fact.execution_id.as_ref() == Some(execution_id)
            && fact.kind.as_str() == "interaction.requested"
    }) {
        let request = payload(requested)?;
        if text(&request, "suspension_id")? != suspension_id {
            continue;
        }
        let interaction_id = text(&request, "interaction_id")?;
        let resolved = snapshot.facts.iter().any(|fact| {
            fact.position > requested.position
                && matches!(
                    fact.kind.as_str(),
                    "interaction.resolved" | "interaction.cancelled"
                )
                && payload(fact)
                    .ok()
                    .and_then(|value| value.get("interaction_id").cloned())
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .as_deref()
                    == Some(interaction_id)
        });
        if !resolved {
            let response_schema_digest = text(&request, "response_schema_digest")?;
            let bound_schema_digest = request
                .get("response_schema")
                .and_then(Value::as_object)
                .and_then(|binding| binding.get("digest"))
                .and_then(Value::as_str)
                .ok_or(RuntimeCommandError::CorruptLedger)?;
            if response_schema_digest != bound_schema_digest {
                return Err(RuntimeCommandError::CorruptLedger);
            }
            let response_schema = canonical_content(&request, "response_schema")?;
            validate_portable_value_schema(&response_schema)
                .map_err(|_| RuntimeCommandError::CorruptLedger)?;
            pending.push(InteractionContinuation {
                execution_id: execution_id.clone(),
                tool_invocation_id: requested
                    .tool_invocation_id
                    .clone()
                    .ok_or(RuntimeCommandError::CorruptLedger)?,
                interaction_id: interaction_id.to_owned(),
                prepared_digest: text(&request, "prepared_digest")?.to_owned(),
                response_schema_digest: response_schema_digest.to_owned(),
                response_schema,
                expiry: InteractionExpiry::parse(text(&request, "expiry_code")?)?,
            });
        }
    }
    match pending.len() {
        0 => Ok(None),
        1 => Ok(pending.pop()),
        _ => Err(RuntimeCommandError::CorruptLedger),
    }
}

fn canonical_content(value: &Map<String, Value>, key: &str) -> Result<Value, RuntimeCommandError> {
    let binding = value
        .get(key)
        .and_then(Value::as_object)
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    let canonical = CanonicalPayload::from_canonical_parts(
        text(binding, "inline_utf8")?.to_owned(),
        text(binding, "digest")?.to_owned(),
    )
    .map_err(|_| RuntimeCommandError::CorruptLedger)?;
    serde_json::from_str(canonical.as_json()).map_err(|_| RuntimeCommandError::CorruptLedger)
}

fn reconciliation_target(
    snapshot: &TurnSnapshot,
    execution_id: &ExecutionId,
) -> Result<Option<ReconciliationTarget>, RuntimeCommandError> {
    let uncertain: Vec<_> = snapshot
        .facts
        .iter()
        .filter(|fact| {
            fact.execution_id.as_ref() == Some(execution_id)
                && fact.kind.as_str() == "effect.uncertain"
        })
        .collect();
    if uncertain.len() > 1 {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    let Some(uncertain) = uncertain.first() else {
        return Ok(None);
    };
    let invocation_id = uncertain
        .tool_invocation_id
        .clone()
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    let uncertain_payload = payload(uncertain)?;
    let prepared_digest = text(&uncertain_payload, "prepared_digest")?;
    let prepared = snapshot
        .facts
        .iter()
        .find(|fact| {
            fact.tool_invocation_id.as_ref() == Some(&invocation_id)
                && fact.kind.as_str() == "effect.prepared"
        })
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    let prepared_payload = payload(prepared)?;
    if text(&prepared_payload, "prepared_digest")? != prepared_digest {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    let reconciled = snapshot.facts.iter().any(|fact| {
        fact.position > uncertain.position
            && fact.tool_invocation_id.as_ref() == Some(&invocation_id)
            && fact.kind.as_str() == "effect.reconciled"
    });
    let observed = snapshot.facts.iter().any(|fact| {
        fact.position > uncertain.position
            && fact.tool_invocation_id.as_ref() == Some(&invocation_id)
            && fact.kind.as_str() == "effect.observation"
    });
    if reconciled != observed {
        return Err(RuntimeCommandError::CorruptLedger);
    }
    Ok(Some(ReconciliationTarget {
        execution_id: execution_id.clone(),
        invocation_id,
        prepared_digest: prepared_digest.to_owned(),
        model_call_id: text(&prepared_payload, "model_call_id")?.to_owned(),
        reconciled,
        observed,
    }))
}

fn limits(value: &Map<String, Value>) -> Result<EffectiveRuntimeLimits, RuntimeCommandError> {
    let limits = value
        .get("limits")
        .and_then(Value::as_object)
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    Ok(EffectiveRuntimeLimits {
        max_iterations: unsigned(limits, "max_iterations")?,
        max_input_tokens: optional_unsigned(limits, "max_input_tokens")?,
        max_output_tokens: optional_unsigned(limits, "max_output_tokens")?,
        deadline_budget_ms: optional_unsigned(limits, "deadline_budget_ms")?,
    })
}

fn payload(fact: &garive_ledger::DurableFact) -> Result<Map<String, Value>, RuntimeCommandError> {
    let value: Value = serde_json::from_str(fact.payload.as_json())
        .map_err(|_| RuntimeCommandError::CorruptLedger)?;
    value
        .as_object()
        .cloned()
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, RuntimeCommandError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn unsigned(value: &Map<String, Value>, key: &str) -> Result<u64, RuntimeCommandError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn optional_unsigned(
    value: &Map<String, Value>,
    key: &str,
) -> Result<Option<u64>, RuntimeCommandError> {
    value.get(key).map(|_| unsigned(value, key)).transpose()
}

fn identity<'a, T>(value: &'a str) -> Result<T, RuntimeCommandError>
where
    T: TryFrom<&'a str>,
{
    T::try_from(value).map_err(|_| RuntimeCommandError::CorruptLedger)
}
