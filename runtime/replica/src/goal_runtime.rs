use garive_goal::{
    GoalDefinitionV1, GoalErrorCode, GoalEvidenceV1, GoalSnapshot, GoalState, GoalTransition,
};
use garive_ledger::{
    CanonicalPayload, CommitResult, FactDraft, FactId, FactKind, LedgerError, SessionId,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    goal_evidence::verify_goal_success_evidence,
    goal_recovery::{reconstruct_goal_graph_from_facts, validate_goal_graph, GoalGraphError},
    SqliteLedger, SqliteLedgerError,
};

/// Authenticated metadata bound to one idempotent Goal command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoalCommandContext {
    /// Stable command identity, reused only for exact replay.
    pub command_id: String,
    /// Authenticated actor reference; never a display name or credential.
    pub actor_reference: String,
    /// RFC 3339 observation timestamp.
    pub recorded_at: String,
}

/// Runtime metadata required around one portable Goal transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GoalRuntimeTransition {
    /// Start a new attempt from Draft, or resume the current attempt from Suspended.
    Activate {
        /// Exact adopted Plan reference, when policy requires one.
        plan_reference: Option<String>,
    },
    /// Suspend active work with a typed durable continuation reference.
    Suspend {
        /// Stable safe reason code.
        reason: String,
        /// Interaction or reconciliation reference, when present.
        suspension_reference: Option<String>,
    },
    /// Close only with complete portable evidence.
    Succeed {
        /// Ordered evidence set covering every declared criterion.
        evidence: Vec<GoalEvidenceV1>,
    },
    /// End unsuccessfully with optional canonical supporting evidence.
    Fail {
        /// Stable safe failure code.
        code: String,
        /// Optional evidence set relevant to the failure.
        evidence: Option<Vec<GoalEvidenceV1>>,
    },
    /// Cancel under the authenticated command actor.
    Cancel {
        /// Stable safe cancellation reason.
        reason: String,
    },
    /// Replace definition content and return to Draft.
    Revise {
        /// Complete replacement definition for the same Goal identity.
        definition: Box<GoalDefinitionV1>,
        /// Stable reason explaining why definition content changed.
        replacement_reason: String,
    },
}

/// Verified Goal projection reconstructed at one Session watermark.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoalRuntimeState {
    /// Pure portable lifecycle snapshot.
    pub snapshot: GoalSnapshot,
    /// Number of distinct attempts started from Draft.
    pub attempt_number: u32,
    /// Optimistic-concurrency version of the containing Session.
    pub session_version: u64,
    /// Highest durable position included in reconstruction.
    pub through_position: u64,
}

/// One validated fact batch and its predicted Goal projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedGoalCommand {
    /// Atomic Ledger batch; currently exactly one command fact.
    pub facts: Vec<FactDraft>,
    /// Projection after applying the planned fact.
    pub next: GoalRuntimeState,
}

/// Stable Runtime failure classes for durable Goal commands and recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GoalRuntimeError {
    /// Command, definition, metadata, or payload is malformed.
    Invalid,
    /// No matching Goal exists in the fixed durable prefix.
    NotFound,
    /// Expected Goal revision is stale.
    RevisionConflict,
    /// The command identity was reused with different semantics.
    CommandConflict,
    /// The requested lifecycle edge is not admitted.
    TransitionInvalid,
    /// Success lacks exact complete criterion evidence.
    EvidenceInsufficient,
    /// Success evidence does not resolve to the fixed durable Session prefix.
    EvidenceInvalid,
    /// Child scope, capability, parent identity, or bound exceeds its parent.
    ScopeExceeded,
    /// Proposed parent graph contains a cycle.
    Cycle,
    /// Persisted Goal facts cannot reconstruct one legal contiguous history.
    RecoveryCorrupt,
    /// SQLite or Ledger durability failed.
    DurabilityFailure,
}

/// Plans creation of revision 1 without mutating durable state.
pub fn plan_create_goal(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    context: &GoalCommandContext,
    definition: GoalDefinitionV1,
) -> Result<PlannedGoalCommand, GoalRuntimeError> {
    validate_context(context)?;
    if definition
        .scope()
        .session_id()
        .is_some_and(|value| value != session_id.as_str())
    {
        return Err(GoalRuntimeError::ScopeExceeded);
    }
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(map_ledger)?
        .ok_or(GoalRuntimeError::NotFound)?;
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_ledger)?;
    let mut goal_ids = BTreeSet::new();
    let mut creation_commands = BTreeMap::new();
    for fact in facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "goal.created")
    {
        let payload: Value = serde_json::from_str(fact.payload.as_json())
            .map_err(|_| GoalRuntimeError::RecoveryCorrupt)?;
        let value = payload
            .as_object()
            .ok_or(GoalRuntimeError::RecoveryCorrupt)?;
        let goal_id = stored_text(value, "goal_id")?;
        let command_id = stored_text(value, "command_id")?;
        if !goal_ids.insert(goal_id.to_owned())
            || creation_commands
                .insert(goal_id.to_owned(), command_id.to_owned())
                .is_some()
        {
            return Err(GoalRuntimeError::RecoveryCorrupt);
        }
    }
    let graph = reconstruct_goal_graph_from_facts(
        &facts,
        watermark.session_version,
        watermark.max_position,
    )?;
    if let Some(command_id) = creation_commands.get(definition.goal_id().as_str()) {
        if command_id != &context.command_id {
            return Err(GoalRuntimeError::CommandConflict);
        }
    } else if let Some(parent_id) = definition.parent_goal_id() {
        let parent = graph
            .get(parent_id.as_str())
            .ok_or(GoalRuntimeError::ScopeExceeded)?;
        if parent.snapshot.state().is_terminal() {
            return Err(GoalRuntimeError::TransitionInvalid);
        }
        definition
            .validate_child_of(parent.snapshot.definition())
            .map_err(map_goal)?;
        let child_count = graph
            .values()
            .filter(|state| state.snapshot.definition().parent_goal_id() == Some(parent_id))
            .count();
        if child_count
            >= usize::try_from(parent.snapshot.definition().bounds().max_child_goals())
                .map_err(|_| GoalRuntimeError::Invalid)?
        {
            return Err(GoalRuntimeError::ScopeExceeded);
        }
    }
    let definition_json = definition.canonical_json().map_err(map_goal)?;
    let definition_digest = definition.digest().map_err(map_goal)?;
    let payload = json!({
        "command_id": context.command_id,
        "goal_id": definition.goal_id().as_str(),
        "revision": 1,
        "definition_digest": definition_digest,
        "definition": content(&definition_json),
        "actor_reference": context.actor_reference,
    });
    let snapshot = GoalSnapshot::new(definition);
    Ok(PlannedGoalCommand {
        facts: vec![fact(context, "goal.created", payload)?],
        next: GoalRuntimeState {
            snapshot,
            attempt_number: 0,
            session_version: watermark.session_version,
            through_position: watermark.max_position,
        },
    })
}

fn stored_text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, GoalRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(GoalRuntimeError::RecoveryCorrupt)
}

/// Plans one exact-revision transition without mutating durable state.
pub fn plan_goal_transition(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    expected_revision: u64,
    context: &GoalCommandContext,
    request: GoalRuntimeTransition,
) -> Result<PlannedGoalCommand, GoalRuntimeError> {
    if goal_id.is_empty() {
        return Err(GoalRuntimeError::Invalid);
    }
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(map_ledger)?
        .ok_or(GoalRuntimeError::NotFound)?;
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_ledger)?;
    let mut graph = reconstruct_goal_graph_from_facts(
        &facts,
        watermark.session_version,
        watermark.max_position,
    )?;
    let current = graph
        .get(goal_id)
        .ok_or(GoalRuntimeError::NotFound)?
        .clone();
    if let GoalRuntimeTransition::Succeed { evidence } = &request {
        verify_goal_success_evidence(
            goal_id,
            current.snapshot.definition().criteria(),
            evidence,
            &graph,
            &facts,
            watermark.session_version,
            None,
        )?;
    }
    let planned = plan_goal_transition_from_state(&current, expected_revision, context, request)?;
    graph.insert(goal_id.into(), planned.next.clone());
    validate_goal_graph(&graph).map_err(|error| match error {
        GoalGraphError::MissingParent | GoalGraphError::ScopeExceeded => {
            GoalRuntimeError::ScopeExceeded
        }
        GoalGraphError::Cycle => GoalRuntimeError::Cycle,
    })?;
    if let Some(parent_id) = planned.next.snapshot.definition().parent_goal_id() {
        let parent = graph
            .get(parent_id.as_str())
            .ok_or(GoalRuntimeError::ScopeExceeded)?;
        if parent.snapshot.state().is_terminal() {
            return Err(GoalRuntimeError::TransitionInvalid);
        }
        let child_count = graph
            .values()
            .filter(|state| state.snapshot.definition().parent_goal_id() == Some(parent_id))
            .count();
        if child_count
            > usize::try_from(parent.snapshot.definition().bounds().max_child_goals())
                .map_err(|_| GoalRuntimeError::Invalid)?
        {
            return Err(GoalRuntimeError::ScopeExceeded);
        }
    }
    Ok(planned)
}

fn plan_goal_transition_from_state(
    current: &GoalRuntimeState,
    expected_revision: u64,
    context: &GoalCommandContext,
    request: GoalRuntimeTransition,
) -> Result<PlannedGoalCommand, GoalRuntimeError> {
    validate_context(context)?;
    if expected_revision != current.snapshot.revision() {
        return Err(GoalRuntimeError::RevisionConflict);
    }
    let goal_id = current.snapshot.definition().goal_id().as_str();
    let previous_digest = current.snapshot.definition().digest().map_err(map_goal)?;
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(GoalRuntimeError::Invalid)?;
    let (kind, payload, transition, attempt_number) = match request {
        GoalRuntimeTransition::Activate { plan_reference } => {
            optional_non_empty(&plan_reference)?;
            let attempt = if current.snapshot.state() == GoalState::Draft {
                current
                    .attempt_number
                    .checked_add(1)
                    .ok_or(GoalRuntimeError::Invalid)?
            } else {
                current.attempt_number
            };
            if attempt > current.snapshot.definition().bounds().max_attempts() {
                return Err(GoalRuntimeError::TransitionInvalid);
            }
            let mut payload = common_payload(context, goal_id, next_revision);
            payload.insert("attempt_number".into(), json!(attempt));
            insert_optional(&mut payload, "plan_reference", plan_reference);
            (
                "goal.activated",
                Value::Object(payload),
                GoalTransition::Activate,
                attempt,
            )
        }
        GoalRuntimeTransition::Suspend {
            reason,
            suspension_reference,
        } => {
            require_non_empty(&reason)?;
            optional_non_empty(&suspension_reference)?;
            let mut payload = common_payload(context, goal_id, next_revision);
            payload.insert("reason".into(), json!(reason.clone()));
            insert_optional(&mut payload, "suspension_reference", suspension_reference);
            (
                "goal.suspended",
                Value::Object(payload),
                GoalTransition::Suspend(reason),
                current.attempt_number,
            )
        }
        GoalRuntimeTransition::Succeed { evidence } => {
            let evidence_json = GoalEvidenceV1::canonical_json(&evidence).map_err(map_goal)?;
            let mut payload = common_payload(context, goal_id, next_revision);
            payload.insert("evidence".into(), content(&evidence_json));
            (
                "goal.succeeded",
                Value::Object(payload),
                GoalTransition::Succeed(evidence),
                current.attempt_number,
            )
        }
        GoalRuntimeTransition::Fail { code, evidence } => {
            require_non_empty(&code)?;
            let mut payload = common_payload(context, goal_id, next_revision);
            payload.insert("code".into(), json!(code.clone()));
            if let Some(values) = &evidence {
                payload.insert(
                    "evidence".into(),
                    content(&GoalEvidenceV1::canonical_json(values).map_err(map_goal)?),
                );
            }
            (
                "goal.failed",
                Value::Object(payload),
                GoalTransition::Fail(code),
                current.attempt_number,
            )
        }
        GoalRuntimeTransition::Cancel { reason } => {
            require_non_empty(&reason)?;
            let mut payload = common_payload(context, goal_id, next_revision);
            payload.insert("reason".into(), json!(reason.clone()));
            payload.insert("actor_reference".into(), json!(context.actor_reference));
            (
                "goal.cancelled",
                Value::Object(payload),
                GoalTransition::Cancel(reason),
                current.attempt_number,
            )
        }
        GoalRuntimeTransition::Revise {
            definition,
            replacement_reason,
        } => {
            require_non_empty(&replacement_reason)?;
            let definition_json = definition.canonical_json().map_err(map_goal)?;
            let definition_digest = definition.digest().map_err(map_goal)?;
            let mut payload = common_payload(context, goal_id, next_revision);
            payload.insert("previous_revision".into(), json!(expected_revision));
            payload.insert("previous_definition_digest".into(), json!(previous_digest));
            payload.insert("definition_digest".into(), json!(definition_digest));
            payload.insert("definition".into(), content(&definition_json));
            payload.insert("replacement_reason".into(), json!(replacement_reason));
            payload.insert("actor_reference".into(), json!(context.actor_reference));
            (
                "goal.revised",
                Value::Object(payload),
                GoalTransition::Revise(definition),
                current.attempt_number,
            )
        }
    };
    let snapshot = current
        .snapshot
        .apply(expected_revision, transition)
        .map_err(map_goal)?;
    Ok(PlannedGoalCommand {
        facts: vec![fact(context, kind, payload)?],
        next: GoalRuntimeState {
            snapshot,
            attempt_number,
            session_version: current.session_version,
            through_position: current.through_position,
        },
    })
}

/// Commits an already validated Goal command with Session optimistic concurrency.
pub fn commit_goal_command(
    ledger: &mut SqliteLedger,
    session_id: SessionId,
    expected_session_version: u64,
    planned: &PlannedGoalCommand,
) -> Result<CommitResult, GoalRuntimeError> {
    ledger
        .commit(session_id, expected_session_version, planned.facts.clone())
        .map_err(map_ledger)
}

fn validate_context(context: &GoalCommandContext) -> Result<(), GoalRuntimeError> {
    require_non_empty(&context.command_id)?;
    require_non_empty(&context.actor_reference)?;
    chrono::DateTime::parse_from_rfc3339(&context.recorded_at)
        .map(|_| ())
        .map_err(|_| GoalRuntimeError::Invalid)
}

fn common_payload(
    context: &GoalCommandContext,
    goal_id: &str,
    revision: u64,
) -> Map<String, Value> {
    Map::from_iter([
        ("command_id".into(), json!(context.command_id)),
        ("goal_id".into(), json!(goal_id)),
        ("revision".into(), json!(revision)),
    ])
}

fn fact(
    context: &GoalCommandContext,
    kind: &str,
    payload: Value,
) -> Result<FactDraft, GoalRuntimeError> {
    Ok(FactDraft {
        fact_id: FactId::try_from(context.command_id.as_str())
            .map_err(|_| GoalRuntimeError::Invalid)?,
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).map_err(|_| GoalRuntimeError::Invalid)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).map_err(|_| GoalRuntimeError::Invalid)?,
        recorded_at: context.recorded_at.clone(),
    })
}

fn content(inline: &str) -> Value {
    json!({"digest": format!("{:x}", Sha256::digest(inline.as_bytes())), "inline_utf8": inline})
}

fn require_non_empty(value: &str) -> Result<(), GoalRuntimeError> {
    if value.is_empty() {
        Err(GoalRuntimeError::Invalid)
    } else {
        Ok(())
    }
}

fn optional_non_empty(value: &Option<String>) -> Result<(), GoalRuntimeError> {
    if value.as_deref() == Some("") {
        Err(GoalRuntimeError::Invalid)
    } else {
        Ok(())
    }
}

fn insert_optional(target: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        target.insert(key.into(), json!(value));
    }
}

fn map_goal(error: garive_goal::GoalError) -> GoalRuntimeError {
    match error.code() {
        GoalErrorCode::GoalRevisionConflict => GoalRuntimeError::RevisionConflict,
        GoalErrorCode::GoalTransitionInvalid => GoalRuntimeError::TransitionInvalid,
        GoalErrorCode::GoalEvidenceInsufficient => GoalRuntimeError::EvidenceInsufficient,
        GoalErrorCode::GoalScopeExceeded => GoalRuntimeError::ScopeExceeded,
        GoalErrorCode::GoalInvalid => GoalRuntimeError::Invalid,
    }
}

fn map_ledger(error: SqliteLedgerError) -> GoalRuntimeError {
    match error {
        SqliteLedgerError::Domain(
            LedgerError::IdempotencyCollision | LedgerError::IncompleteReplay,
        ) => GoalRuntimeError::CommandConflict,
        SqliteLedgerError::Domain(LedgerError::ConcurrentModification) => {
            GoalRuntimeError::RevisionConflict
        }
        SqliteLedgerError::Domain(LedgerError::InvalidTransition) => {
            GoalRuntimeError::TransitionInvalid
        }
        SqliteLedgerError::Domain(_) => GoalRuntimeError::Invalid,
        SqliteLedgerError::CorruptLedger(_)
        | SqliteLedgerError::UnsupportedSchema(_)
        | SqliteLedgerError::InvalidStoredValue(_) => GoalRuntimeError::RecoveryCorrupt,
        SqliteLedgerError::Storage(_)
        | SqliteLedgerError::Lease(_)
        | SqliteLedgerError::ScheduleLease(_) => GoalRuntimeError::DurabilityFailure,
    }
}
