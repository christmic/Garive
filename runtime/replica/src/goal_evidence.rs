use std::collections::{BTreeMap, BTreeSet};

use garive_goal::{GoalCriterion, GoalEvidenceKind, GoalEvidenceV1, GoalState};
use garive_ledger::{CanonicalPayload, DurableFact};
use serde_json::{json, Map, Value};

use crate::{GoalRuntimeError, GoalRuntimeState};

pub(crate) fn verify_goal_success_evidence(
    goal_id: &str,
    criteria: &[GoalCriterion],
    evidence: &[GoalEvidenceV1],
    graph: &BTreeMap<String, GoalRuntimeState>,
    facts: &[DurableFact],
    observed_session_version: u64,
    success_position: Option<u64>,
) -> Result<(), GoalRuntimeError> {
    let by_criterion: BTreeMap<_, _> = evidence
        .iter()
        .map(|value| (value.criterion_id(), value))
        .collect();
    let evidence_ids: BTreeSet<_> = evidence.iter().map(GoalEvidenceV1::evidence_id).collect();
    if by_criterion.len() != evidence.len()
        || evidence_ids.len() != evidence.len()
        || criteria.len() != evidence.len()
    {
        return Err(GoalRuntimeError::EvidenceInsufficient);
    }
    let facts_by_id: BTreeMap<_, _> = facts
        .iter()
        .map(|fact| (fact.fact_id.as_str(), fact))
        .collect();
    for criterion in criteria {
        let item = by_criterion
            .get(criterion.criterion_id())
            .ok_or(GoalRuntimeError::EvidenceInsufficient)?;
        if item.observed_at_commit_version() == 0
            || item.observed_at_commit_version() > observed_session_version
            || (success_position.is_none()
                && item.observed_at_commit_version() != observed_session_version)
        {
            return Err(GoalRuntimeError::EvidenceInvalid);
        }
        match criterion {
            GoalCriterion::UserAcceptance {
                response_schema_digest,
                ..
            } => verify_user_acceptance(
                item,
                response_schema_digest,
                &facts_by_id,
                success_position,
            )?,
            GoalCriterion::Artifact {
                artifact_kind,
                required_digest,
                ..
            } => verify_artifact(
                item,
                artifact_kind,
                required_digest.as_deref(),
                &facts_by_id,
                success_position,
            )?,
            GoalCriterion::DurableFact {
                fact_kind,
                subject_digest,
                ..
            } => verify_fact(
                item,
                fact_kind,
                subject_digest,
                &facts_by_id,
                success_position,
            )?,
            GoalCriterion::ChildGoals { child_goal_ids, .. } => {
                verify_children(item, goal_id, child_goal_ids, graph)?
            }
        }
    }
    Ok(())
}

fn referenced_fact<'a>(
    item: &GoalEvidenceV1,
    expected_kind: GoalEvidenceKind,
    facts: &'a BTreeMap<&str, &DurableFact>,
    success_position: Option<u64>,
) -> Result<&'a DurableFact, GoalRuntimeError> {
    if item.kind() != expected_kind {
        return Err(GoalRuntimeError::EvidenceInvalid);
    }
    let fact = facts
        .get(item.durable_reference())
        .copied()
        .ok_or(GoalRuntimeError::EvidenceInvalid)?;
    if success_position.is_some_and(|position| fact.position >= position) {
        return Err(GoalRuntimeError::EvidenceInvalid);
    }
    fact.verify()
        .map_err(|_| GoalRuntimeError::RecoveryCorrupt)?;
    Ok(fact)
}

fn verify_fact(
    item: &GoalEvidenceV1,
    fact_kind: &str,
    subject_digest: &str,
    facts: &BTreeMap<&str, &DurableFact>,
    success_position: Option<u64>,
) -> Result<(), GoalRuntimeError> {
    let fact = referenced_fact(item, GoalEvidenceKind::DurableFact, facts, success_position)?;
    if fact.kind.as_str() != fact_kind
        || fact.payload.sha256() != subject_digest
        || item.evidence_digest() != subject_digest
    {
        return Err(GoalRuntimeError::EvidenceInvalid);
    }
    Ok(())
}

fn verify_artifact(
    item: &GoalEvidenceV1,
    artifact_kind: &str,
    required_digest: Option<&str>,
    facts: &BTreeMap<&str, &DurableFact>,
    success_position: Option<u64>,
) -> Result<(), GoalRuntimeError> {
    let fact = referenced_fact(item, GoalEvidenceKind::Artifact, facts, success_position)?;
    let payload = payload(fact)?;
    let content_digest = text(&payload, "content_digest")?;
    if fact.kind.as_str() != "artifact.committed"
        || text(&payload, "kind")? != artifact_kind
        || required_digest.is_some_and(|required| required != content_digest)
        || item.evidence_digest() != content_digest
    {
        return Err(GoalRuntimeError::EvidenceInvalid);
    }
    Ok(())
}

fn verify_user_acceptance(
    item: &GoalEvidenceV1,
    response_schema_digest: &str,
    facts: &BTreeMap<&str, &DurableFact>,
    success_position: Option<u64>,
) -> Result<(), GoalRuntimeError> {
    let resolved = referenced_fact(
        item,
        GoalEvidenceKind::UserAcceptance,
        facts,
        success_position,
    )?;
    if resolved.kind.as_str() != "interaction.resolved" {
        return Err(GoalRuntimeError::EvidenceInvalid);
    }
    let tool_id = resolved
        .tool_invocation_id
        .as_ref()
        .ok_or(GoalRuntimeError::EvidenceInvalid)?;
    let resolved_payload = payload(resolved)?;
    let response_digest = content_digest(&resolved_payload, "response")?;
    let mut requested = facts.values().filter(|fact| {
        fact.kind.as_str() == "interaction.requested"
            && fact.tool_invocation_id.as_ref() == Some(tool_id)
            && fact.position < resolved.position
    });
    let request = requested.next().ok_or(GoalRuntimeError::EvidenceInvalid)?;
    if requested.next().is_some() {
        return Err(GoalRuntimeError::EvidenceInvalid);
    }
    let request_payload = payload(request)?;
    for key in ["interaction_id", "suspension_id", "prepared_digest"] {
        if text(&request_payload, key)? != text(&resolved_payload, key)? {
            return Err(GoalRuntimeError::EvidenceInvalid);
        }
    }
    if text(&request_payload, "response_schema_digest")? != response_schema_digest
        || item.evidence_digest() != response_digest
    {
        return Err(GoalRuntimeError::EvidenceInvalid);
    }
    Ok(())
}

fn verify_children(
    item: &GoalEvidenceV1,
    goal_id: &str,
    child_ids: &BTreeSet<garive_goal::GoalId>,
    graph: &BTreeMap<String, GoalRuntimeState>,
) -> Result<(), GoalRuntimeError> {
    if item.kind() != GoalEvidenceKind::ChildGoals {
        return Err(GoalRuntimeError::EvidenceInvalid);
    }
    let entries = child_ids
        .iter()
        .map(|child_id| {
            let child = graph
                .get(child_id.as_str())
                .ok_or(GoalRuntimeError::EvidenceInvalid)?;
            if child.snapshot.state() != GoalState::Succeeded
                || child.snapshot.definition().parent_goal_id().map(|id| id.as_str())
                    != Some(goal_id)
            {
                return Err(GoalRuntimeError::EvidenceInvalid);
            }
            Ok(json!({
                "goal_id": child_id.as_str(),
                "revision": child.snapshot.revision(),
                "definition_digest": child.snapshot.definition().digest().map_err(|_| GoalRuntimeError::RecoveryCorrupt)?,
            }))
        })
        .collect::<Result<Vec<_>, GoalRuntimeError>>()?;
    let digest = CanonicalPayload::from_value(&Value::Array(entries))
        .map_err(|_| GoalRuntimeError::RecoveryCorrupt)?
        .sha256()
        .to_owned();
    if item.evidence_digest() != digest || item.durable_reference() != format!("goal-set:{digest}")
    {
        return Err(GoalRuntimeError::EvidenceInvalid);
    }
    Ok(())
}

fn payload(fact: &DurableFact) -> Result<Map<String, Value>, GoalRuntimeError> {
    serde_json::from_str::<Value>(fact.payload.as_json())
        .map_err(|_| GoalRuntimeError::RecoveryCorrupt)?
        .as_object()
        .cloned()
        .ok_or(GoalRuntimeError::RecoveryCorrupt)
}

fn text<'a>(payload: &'a Map<String, Value>, key: &str) -> Result<&'a str, GoalRuntimeError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(GoalRuntimeError::EvidenceInvalid)
}

fn content_digest<'a>(
    payload: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, GoalRuntimeError> {
    payload
        .get(key)
        .and_then(Value::as_object)
        .and_then(|value| value.get("digest"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(GoalRuntimeError::EvidenceInvalid)
}
