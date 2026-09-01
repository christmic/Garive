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
            GoalCriterion::ChildGoals { child_goal_ids, .. } => verify_children(
                item,
                goal_id,
                child_goal_ids,
                graph,
                facts,
                success_position,
            )?,
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
    facts: &[DurableFact],
    success_position: Option<u64>,
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
                || success_position.is_some_and(|parent_position| {
                    succeeded_position(facts, child_id.as_str())
                        .is_none_or(|child_position| child_position >= parent_position)
                })
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

fn succeeded_position(facts: &[DurableFact], goal_id: &str) -> Option<u64> {
    let mut positions = facts.iter().filter_map(|fact| {
        if fact.kind.as_str() != "goal.succeeded" {
            return None;
        }
        let payload: Value = serde_json::from_str(fact.payload.as_json()).ok()?;
        (payload.get("goal_id").and_then(Value::as_str) == Some(goal_id)).then_some(fact.position)
    });
    let position = positions.next()?;
    positions.next().is_none().then_some(position)
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

#[cfg(test)]
mod tests {
    use garive_goal::{
        GoalBoundsV1, GoalCriterionId, GoalDefinitionV1, GoalEvidenceId, GoalId, GoalScopeV1,
        GoalSnapshot, GoalTransition,
    };
    use garive_ledger::{FactId, FactKind, SessionId, ToolInvocationId};

    use super::*;

    const DIGEST_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DIGEST_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn artifact_and_user_acceptance_require_exact_durable_bindings() {
        let artifact = fact(
            "artifact-fact",
            3,
            "artifact.committed",
            json!({"kind":"file","content_digest":DIGEST_A}),
            None,
        );
        let artifact_item = evidence(
            "artifact-evidence",
            "artifact",
            GoalEvidenceKind::Artifact,
            "artifact-fact",
            DIGEST_A,
        );
        let artifact_facts = BTreeMap::from([("artifact-fact", &artifact)]);
        assert!(verify_artifact(
            &artifact_item,
            "file",
            Some(DIGEST_A),
            &artifact_facts,
            Some(4)
        )
        .is_ok());
        assert_eq!(
            verify_artifact(
                &artifact_item,
                "text",
                Some(DIGEST_A),
                &artifact_facts,
                Some(4)
            ),
            Err(GoalRuntimeError::EvidenceInvalid)
        );

        let tool = ToolInvocationId::try_from("tool-1").unwrap();
        let requested = fact(
            "request",
            1,
            "interaction.requested",
            json!({"interaction_id":"interaction-1","suspension_id":"suspension-1","prepared_digest":DIGEST_B,"response_schema_digest":DIGEST_A}),
            Some(tool.clone()),
        );
        let resolved = fact(
            "resolved",
            2,
            "interaction.resolved",
            json!({"interaction_id":"interaction-1","suspension_id":"suspension-1","prepared_digest":DIGEST_B,"response":{"digest":DIGEST_B}}),
            Some(tool),
        );
        let acceptance = evidence(
            "acceptance-evidence",
            "acceptance",
            GoalEvidenceKind::UserAcceptance,
            "resolved",
            DIGEST_B,
        );
        let interaction_facts = BTreeMap::from([("request", &requested), ("resolved", &resolved)]);
        assert!(verify_user_acceptance(&acceptance, DIGEST_A, &interaction_facts, Some(3)).is_ok());
        assert_eq!(
            verify_user_acceptance(&acceptance, DIGEST_B, &interaction_facts, Some(3)),
            Err(GoalRuntimeError::EvidenceInvalid)
        );
    }

    #[test]
    fn child_goal_evidence_requires_success_before_parent_success() {
        let child_id = GoalId::new("child").unwrap();
        let definition = GoalDefinitionV1::new(
            child_id.clone(),
            "Child",
            vec![GoalCriterion::UserAcceptance {
                criterion_id: GoalCriterionId::new("accepted").unwrap(),
                response_schema_digest: DIGEST_A.into(),
            }],
            GoalScopeV1::new(Some("session-1".into()), []).unwrap(),
            GoalBoundsV1::new(1, 1, 1, None, None).unwrap(),
            Some(GoalId::new("parent").unwrap()),
            [],
        )
        .unwrap();
        let child_terminal = GoalSnapshot::new(definition)
            .apply(1, GoalTransition::Activate)
            .unwrap()
            .apply(
                2,
                GoalTransition::Succeed(vec![evidence(
                    "child-acceptance",
                    "accepted",
                    GoalEvidenceKind::UserAcceptance,
                    "resolved",
                    DIGEST_A,
                )]),
            )
            .unwrap();
        let graph = BTreeMap::from([(
            "child".into(),
            GoalRuntimeState {
                snapshot: child_terminal,
                attempt_number: 1,
                session_version: 5,
                through_position: 5,
            },
        )]);
        let entries = json!([{
            "goal_id":"child",
            "revision":3,
            "definition_digest":graph["child"].snapshot.definition().digest().unwrap(),
        }]);
        let digest = CanonicalPayload::from_value(&entries)
            .unwrap()
            .sha256()
            .to_owned();
        let item = evidence(
            "children-evidence",
            "children",
            GoalEvidenceKind::ChildGoals,
            &format!("goal-set:{digest}"),
            &digest,
        );
        let child_ids = BTreeSet::from([child_id]);
        let child_success = fact(
            "child-success",
            4,
            "goal.succeeded",
            json!({"goal_id":"child"}),
            None,
        );
        assert!(verify_children(
            &item,
            "parent",
            &child_ids,
            &graph,
            std::slice::from_ref(&child_success),
            Some(5)
        )
        .is_ok());
        assert_eq!(
            verify_children(
                &item,
                "parent",
                &child_ids,
                &graph,
                &[child_success],
                Some(4)
            ),
            Err(GoalRuntimeError::EvidenceInvalid)
        );
    }

    fn evidence(
        evidence_id: &str,
        criterion_id: &str,
        kind: GoalEvidenceKind,
        reference: &str,
        digest: &str,
    ) -> GoalEvidenceV1 {
        GoalEvidenceV1::new(
            GoalEvidenceId::new(evidence_id).unwrap(),
            GoalCriterionId::new(criterion_id).unwrap(),
            kind,
            reference,
            digest,
            5,
        )
        .unwrap()
    }

    fn fact(
        id: &str,
        position: u64,
        kind: &str,
        payload: Value,
        tool_invocation_id: Option<ToolInvocationId>,
    ) -> DurableFact {
        DurableFact {
            fact_id: FactId::try_from(id).unwrap(),
            session_id: SessionId::try_from("session-1").unwrap(),
            position,
            turn_id: None,
            execution_id: None,
            model_request_id: None,
            tool_invocation_id,
            kind: FactKind::new(kind).unwrap(),
            schema_version: 1,
            payload: CanonicalPayload::from_value(&payload).unwrap(),
            recorded_at: "2026-09-01T00:00:00Z".into(),
        }
    }
}
