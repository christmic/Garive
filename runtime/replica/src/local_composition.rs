//! Fixed-prefix construction of one local model-only execution.

use std::{collections::BTreeSet, num::NonZeroU32};

use garive_core::{
    AgentCursor, AgentDefinitionId as CoreDefinitionId,
    AgentDefinitionRevision as CoreDefinitionRevision, AgentEntry,
    AgentInstanceId as CoreAgentInstanceId, AgentTurnRequest, CandidateKind, ContextCandidate,
    ContextPort, ContextPortError, ContextPurpose, ContextRequest, ExecutionId as CoreExecutionId,
    ExecutionLimits, FactRef, ModelOnlyLimits, ModelRecoveryPolicy, ResumeInput, Retention,
    SessionId as CoreSessionId, TurnId as CoreTurnId, Visibility,
};
use garive_ledger::DurableFact;
use garive_llm::{
    ModelCapability, ModelInputContent, ModelInputItem, ModelOutputSettings, ModelRole,
    ModelTargetId,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    CommittedTurn, DurableExecutionConfig, ExecutionLeaseRequest, ModelLifecycleContext,
    SqliteLedger,
};

/// Explicit model-only policy supplied by Garive configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalExecutionPolicy {
    /// Neutral target identity admitted by the constructed model port.
    pub model_target_id: String,
    /// Runtime deployment identity recorded in model facts.
    pub deployment_id: String,
    /// Exact recovery-policy revision recorded durably.
    pub recovery_policy_revision: String,
    /// Provider-neutral capabilities required from the target.
    pub required_capabilities: Vec<ModelCapability>,
    /// Frozen output constraints.
    pub model_output: ModelOutputSettings,
    /// Frozen model recovery decisions.
    pub recovery_policy: ModelRecoveryPolicy,
    /// Non-zero maximum context items.
    pub max_context_items: usize,
    /// Non-zero maximum visible context bytes.
    pub max_context_utf8_bytes: usize,
    /// Non-zero durable model dispatch-attempt bound.
    pub max_model_attempts: u64,
}

/// Explicit per-dispatch operational values supplied by Runtime ports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalExecutionAttempt {
    /// Stable Runtime worker identity.
    pub worker_owner_id: String,
    /// Unpredictable lease token created outside Engine and persistence.
    pub lease_token: String,
    /// Explicit monotonic clock value.
    pub now_ms: u64,
    /// Boot-scoped revision naming the monotonic clock domain.
    pub clock_revision: String,
    /// Non-zero lease duration.
    pub lease_duration_ms: u64,
    /// Canonical UTC observation time.
    pub recorded_at: String,
}

/// Validated Core request, durable coordinator config and input context.
pub struct ReconstructedLocalExecution {
    /// Immutable Core request reconstructed from durable facts.
    pub request: AgentTurnRequest,
    /// Durable execution lifecycle and lease configuration.
    pub durable: DurableExecutionConfig,
    /// Required fixed-prefix context port for the trusted input.
    pub context: LocalInputContext,
}

/// Stable secret-free failure before any model preflight or dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalReconstructionError {
    /// Explicit composition policy or attempt values are invalid.
    InvalidComposition,
    /// Durable storage is absent, unavailable or corrupt.
    DurabilityUnavailable,
    /// The requested durable coordinates do not form one exact start.
    ReconstructionFailed,
    /// The requested Execution is already terminal.
    AlreadyTerminal,
}
impl LocalReconstructionError {
    /// Returns the stable operational code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidComposition => "invalid_composition",
            Self::DurabilityUnavailable => "durability_unavailable",
            Self::ReconstructionFailed => "reconstruction_failed",
            Self::AlreadyTerminal => "already_terminal",
        }
    }
}

/// Reconstructs one initial Execution from verified SQLite facts.
pub fn reconstruct_local_start(
    ledger: &SqliteLedger,
    committed: &CommittedTurn,
    policy: &LocalExecutionPolicy,
    attempt: &LocalExecutionAttempt,
) -> Result<ReconstructedLocalExecution, LocalReconstructionError> {
    validate_explicit(policy, attempt)?;
    if committed.committed_position == 0 || committed.session_version == 0 {
        return Err(LocalReconstructionError::ReconstructionFailed);
    }
    let current = ledger
        .load_turn(&committed.turn_id)
        .map_err(|_| LocalReconstructionError::DurabilityUnavailable)?;
    if current.facts.iter().any(|fact| {
        fact.execution_id.as_ref() == Some(&committed.execution_id)
            && matches!(
                fact.kind.as_str(),
                "execution.completed"
                    | "execution.suspended"
                    | "execution.stopped"
                    | "execution.failed"
                    | "execution.abandoned"
            )
    }) {
        return Err(LocalReconstructionError::AlreadyTerminal);
    }
    let facts = ledger
        .read_facts(&committed.session_id, 0, committed.committed_position, None)
        .map_err(|_| LocalReconstructionError::DurabilityUnavailable)?;
    let opened = exactly_one(&facts, |fact| fact.kind.as_str() == "session.opened")?;
    let execution = exactly_one(&facts, |fact| {
        fact.turn_id.as_ref() == Some(&committed.turn_id)
            && fact.execution_id.as_ref() == Some(&committed.execution_id)
            && fact.kind.as_str() == "execution.started"
    })?;
    let opened_payload = payload(opened)?;
    let started = facts
        .iter()
        .filter(|fact| {
            fact.turn_id.as_ref() == Some(&committed.turn_id)
                && fact.kind.as_str() == "turn.started"
                && fact.position < execution.position
        })
        .max_by_key(|fact| fact.position)
        .ok_or(LocalReconstructionError::ReconstructionFailed)?;
    let started_payload = payload(started)?;
    let workspace_context = workspace_context_candidate(
        &facts,
        started,
        &started_payload,
        committed.session_id.as_str(),
    )?;
    let is_start = started_payload["kind"] == "start";
    let input = facts
        .iter()
        .filter(|fact| {
            fact.turn_id.as_ref() == Some(&committed.turn_id)
                && fact.kind.as_str() == "turn.input"
                && if is_start {
                    fact.position > started.position && fact.position < execution.position
                } else {
                    fact.position < started.position
                }
        })
        .max_by_key(|fact| fact.position)
        .ok_or(LocalReconstructionError::ReconstructionFailed)?;
    let input_payload = payload(input)?;
    let execution_payload = payload(execution)?;
    let trusted_input = text(&input_payload, &["content", "inline_utf8"])?;
    let trusted_digest = text(&input_payload, &["content", "digest"])?;
    let valid_input = if is_start {
        input_payload["input_kind"] == "trusted_user"
            && started_payload["trusted_input_digest"] == trusted_digest
    } else {
        matches!(
            input_payload["input_kind"].as_str(),
            Some("external_input" | "interaction_string" | "interaction_json")
        ) && input_payload["suspension_id"] == started_payload["prior_suspension_id"]
    };
    if !valid_input
        || digest(trusted_input.as_bytes()) != trusted_digest
        || opened_payload["agent_instance_id"] != started_payload["agent_instance_id"]
        || opened_payload["definition_id"] != started_payload["definition_id"]
        || opened_payload["definition_revision"] != started_payload["definition_revision"]
        || opened_payload["snapshot_digest"] != started_payload["snapshot_digest"]
        || execution_payload["snapshot_digest"] != started_payload["snapshot_digest"]
    {
        return Err(LocalReconstructionError::ReconstructionFailed);
    }
    let completed_iterations =
        u32::try_from(number(&execution_payload, &["completed_iterations"])?)
            .map_err(|_| LocalReconstructionError::ReconstructionFailed)?;
    let recovery_ordinal = number(&execution_payload, &["recovery_ordinal"])?;
    let is_initial_start = is_start && recovery_ordinal == 0;
    let last_safe_position = number(&execution_payload, &["through_position"])?;
    let max_iterations = number(&execution_payload, &["limits", "max_iterations"])?;
    let max_iterations = u32::try_from(max_iterations)
        .ok()
        .and_then(NonZeroU32::new)
        .ok_or(LocalReconstructionError::ReconstructionFailed)?;
    let frozen_output = optional_number(&execution_payload, &["limits", "max_output_tokens"])?;
    if frozen_output != policy.model_output.max_output_tokens {
        return Err(LocalReconstructionError::ReconstructionFailed);
    }
    let context = LocalInputContext {
        session_id: committed.session_id.as_str().to_owned(),
        position: input.position,
        text: trusted_input.to_owned(),
        workspace_context,
    };
    let frozen_input = optional_number(&execution_payload, &["limits", "max_input_tokens"])?;
    let max_total_tokens = match (frozen_input, frozen_output) {
        (Some(input), Some(output)) => Some(
            input
                .checked_add(output)
                .ok_or(LocalReconstructionError::ReconstructionFailed)?,
        ),
        _ => None,
    };
    let deadline_tick = optional_number(&execution_payload, &["limits", "deadline_budget_ms"])?
        .map(|budget| {
            attempt
                .now_ms
                .checked_add(budget)
                .ok_or(LocalReconstructionError::ReconstructionFailed)
        })
        .transpose()?;
    let request = AgentTurnRequest {
        session_id: core_identity::<CoreSessionId>(committed.session_id.as_str())?,
        turn_id: core_identity::<CoreTurnId>(committed.turn_id.as_str())?,
        execution_id: core_identity::<CoreExecutionId>(committed.execution_id.as_str())?,
        agent_instance_id: core_identity::<CoreAgentInstanceId>(text_value(
            &started_payload,
            "agent_instance_id",
        )?)?,
        definition_id: core_identity::<CoreDefinitionId>(text_value(
            &started_payload,
            "definition_id",
        )?)?,
        definition_revision: core_identity::<CoreDefinitionRevision>(text_value(
            &started_payload,
            "definition_revision",
        )?)?,
        entry: if is_initial_start {
            AgentEntry::Start {
                trusted_input: trusted_input.to_owned(),
            }
        } else if is_start {
            AgentEntry::Continue {
                resume_input: ResumeInput::ResourceReady,
            }
        } else {
            AgentEntry::Continue {
                resume_input: ResumeInput::ExternalInput(trusted_input.to_owned()),
            }
        },
        cursor: AgentCursor {
            completed_iterations,
            last_durable_position: if is_initial_start {
                0
            } else {
                last_safe_position
            },
        },
        context_request: ContextRequest {
            session_id: committed.session_id.as_str().to_owned(),
            turn_id: committed.turn_id.as_str().to_owned(),
            purpose: ContextPurpose::Inference,
            after_position: None,
            through_position: committed.committed_position,
            max_items: policy.max_context_items,
            max_utf8_bytes: policy.max_context_utf8_bytes,
        },
        activated_skills: vec![],
        capability_context_candidates: vec![],
        model_targets: vec![ModelTargetId::new(&policy.model_target_id)],
        required_capabilities: policy.required_capabilities.clone(),
        model_output: policy.model_output.clone(),
        recovery_policy: policy.recovery_policy,
        limits: ModelOnlyLimits {
            execution: ExecutionLimits::new(max_iterations),
            max_total_tokens,
            deadline_tick,
        },
    };
    request
        .validate()
        .map_err(|_| LocalReconstructionError::ReconstructionFailed)?;
    Ok(ReconstructedLocalExecution {
        request,
        durable: DurableExecutionConfig {
            session_id: committed.session_id.clone(),
            expected_session_version: committed.session_version,
            model: ModelLifecycleContext {
                turn_id: committed.turn_id.clone(),
                execution_id: committed.execution_id.clone(),
                deployment_id: policy.deployment_id.clone(),
                recovery_policy_revision: policy.recovery_policy_revision.clone(),
                max_attempts: policy.max_model_attempts,
                recorded_at: attempt.recorded_at.clone(),
            },
            lease: ExecutionLeaseRequest {
                turn_id: committed.turn_id.clone(),
                execution_id: committed.execution_id.clone(),
                owner_id: attempt.worker_owner_id.clone(),
                lease_token: attempt.lease_token.clone(),
                now_ms: attempt.now_ms,
                duration_ms: attempt.lease_duration_ms,
            },
        },
        context,
    })
}

/// Fixed-prefix context containing only the admitted trusted user input.
pub struct LocalInputContext {
    session_id: String,
    position: u64,
    text: String,
    workspace_context: Option<ContextCandidate>,
}
impl ContextPort for LocalInputContext {
    fn read_candidates(
        &mut self,
        request: &ContextRequest,
        _: u32,
    ) -> Result<Vec<ContextCandidate>, ContextPortError> {
        if request.session_id != self.session_id
            || self.position == 0
            || self.position > request.through_position
        {
            return Err(ContextPortError::PortFailure);
        }
        let item = ModelInputItem::Message {
            role: ModelRole::User,
            content: vec![ModelInputContent::Text(self.text.clone())],
        };
        let reference = FactRef {
            session_id: self.session_id.clone(),
            position: self.position,
        };
        let input = ContextCandidate {
            fact_ref: reference,
            kind: CandidateKind::UserInput,
            retention: Retention::Required,
            visibility: Visibility::Visible,
            items: vec![item],
        };
        let mut candidates = Vec::with_capacity(2);
        if let Some(workspace) = &self.workspace_context {
            if workspace.fact_ref.session_id != request.session_id
                || workspace.fact_ref.position == 0
                || workspace.fact_ref.position >= self.position
            {
                return Err(ContextPortError::PortFailure);
            }
            candidates.push(workspace.clone());
        }
        candidates.push(input);
        Ok(candidates)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceContextPayload {
    command_id: String,
    workspace_id: String,
    grant_revision: u64,
    entries: Vec<WorkspaceContextEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceContextEntry {
    entry_id: String,
    display_name: String,
    kind: String,
    content: WorkspaceInlineContent,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceInlineContent {
    digest: String,
    inline_utf8: String,
}

fn workspace_context_candidate(
    facts: &[DurableFact],
    started: &DurableFact,
    started_payload: &Value,
    session_id: &str,
) -> Result<Option<ContextCandidate>, LocalReconstructionError> {
    let Some(candidate) = facts
        .iter()
        .find(|fact| fact.position.checked_add(1) == Some(started.position))
        .filter(|fact| fact.kind.as_str() == "workspace.context_selected")
    else {
        return Ok(None);
    };
    let value: WorkspaceContextPayload = serde_json::from_value(payload(candidate)?)
        .map_err(|_| LocalReconstructionError::ReconstructionFailed)?;
    if value.command_id != text_value(started_payload, "command_id")?
        || value.workspace_id.is_empty()
        || value.grant_revision == 0
        || value.entries.is_empty()
    {
        return Err(LocalReconstructionError::ReconstructionFailed);
    }
    let items = value
        .entries
        .into_iter()
        .map(|entry| {
            if entry.entry_id.is_empty()
                || entry.display_name.is_empty()
                || entry.kind != "text"
                || digest(entry.content.inline_utf8.as_bytes()) != entry.content.digest
            {
                return Err(LocalReconstructionError::ReconstructionFailed);
            }
            Ok(ModelInputItem::Message {
                role: ModelRole::User,
                content: vec![ModelInputContent::Text(
                    json!({
                        "type":"garive.workspace_file",
                        "workspace_id":value.workspace_id,
                        "grant_revision":value.grant_revision,
                        "entry_id":entry.entry_id,
                        "display_name":entry.display_name,
                        "content_digest":entry.content.digest,
                        "content":entry.content.inline_utf8,
                    })
                    .to_string(),
                )],
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(ContextCandidate {
        fact_ref: FactRef {
            session_id: session_id.to_owned(),
            position: candidate.position,
        },
        kind: CandidateKind::Knowledge,
        retention: Retention::Required,
        visibility: Visibility::Purposes(BTreeSet::from([ContextPurpose::Inference])),
        items,
    }))
}

fn validate_explicit(
    policy: &LocalExecutionPolicy,
    attempt: &LocalExecutionAttempt,
) -> Result<(), LocalReconstructionError> {
    if policy.model_target_id.is_empty()
        || policy.deployment_id.is_empty()
        || policy.recovery_policy_revision.is_empty()
        || policy.required_capabilities.is_empty()
        || policy
            .required_capabilities
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != policy.required_capabilities.len()
        || policy.max_context_items == 0
        || policy.max_context_utf8_bytes == 0
        || policy.max_model_attempts == 0
        || attempt.worker_owner_id.is_empty()
        || attempt.lease_token.is_empty()
        || attempt.clock_revision.is_empty()
        || attempt.lease_duration_ms == 0
        || !canonical_utc(&attempt.recorded_at)
    {
        Err(LocalReconstructionError::InvalidComposition)
    } else {
        Ok(())
    }
}

fn exactly_one(
    facts: &[DurableFact],
    predicate: impl Fn(&DurableFact) -> bool,
) -> Result<&DurableFact, LocalReconstructionError> {
    let mut found = facts.iter().filter(|fact| predicate(fact));
    let value = found
        .next()
        .ok_or(LocalReconstructionError::ReconstructionFailed)?;
    if found.next().is_some() {
        Err(LocalReconstructionError::ReconstructionFailed)
    } else {
        Ok(value)
    }
}

fn payload(fact: &DurableFact) -> Result<Value, LocalReconstructionError> {
    serde_json::from_str(fact.payload.as_json())
        .map_err(|_| LocalReconstructionError::ReconstructionFailed)
}

fn text<'a>(value: &'a Value, path: &[&str]) -> Result<&'a str, LocalReconstructionError> {
    let mut current = value;
    for segment in path {
        current = current
            .get(segment)
            .ok_or(LocalReconstructionError::ReconstructionFailed)?;
    }
    current
        .as_str()
        .ok_or(LocalReconstructionError::ReconstructionFailed)
}

fn text_value<'a>(value: &'a Value, name: &str) -> Result<&'a str, LocalReconstructionError> {
    text(value, &[name])
}

fn number(value: &Value, path: &[&str]) -> Result<u64, LocalReconstructionError> {
    let mut current = value;
    for segment in path {
        current = current
            .get(segment)
            .ok_or(LocalReconstructionError::ReconstructionFailed)?;
    }
    current
        .as_u64()
        .ok_or(LocalReconstructionError::ReconstructionFailed)
}

fn optional_number(value: &Value, path: &[&str]) -> Result<Option<u64>, LocalReconstructionError> {
    let mut current = value;
    for segment in path {
        let Some(next) = current.get(segment) else {
            return Ok(None);
        };
        current = next;
    }
    current
        .as_u64()
        .map(Some)
        .ok_or(LocalReconstructionError::ReconstructionFailed)
}

fn core_identity<T>(value: &str) -> Result<T, LocalReconstructionError>
where
    for<'a> T: TryFrom<&'a str>,
{
    T::try_from(value).map_err(|_| LocalReconstructionError::ReconstructionFailed)
}

fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn canonical_utc(value: &str) -> bool {
    use chrono::{DateTime, SecondsFormat, Utc};
    DateTime::parse_from_rfc3339(value).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == value
    })
}
