//! Agent-originated collaboration tools and their durable-command outbox.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use garive_core::ToolPreparationPort;
use garive_ledger::{CanonicalPayload, SessionId};
use garive_multiagent::{
    CollaborationToolCatalogue, COLLECT_DELEGATIONS_TOOL, DELEGATE_TOOL, FORK_SELF_TOOL,
    MESSAGE_AGENT_TOOL,
};
use garive_tools::{
    EffectReceipt, ExecutionFact, InvocationGrant, PreparedToolCall, ReceiptId,
    TerminalClassification, ToolIntent, ToolInvocationId,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::live_host::DelegationAssigneeBody;
use crate::{
    CommittedTurn, ExecutorDispatch, ExecutorDispatchError, ExecutorFuture, ExecutorPort, LiveHost,
    LocalWorkerError, PreparedExecution, SqliteLedger,
};

/// Stable executor identity for Runtime-owned collaboration commands.
pub const COLLABORATION_EXECUTOR_ID: &str = "garive.runtime.collaboration";
/// Exact first executor revision.
pub const COLLABORATION_EXECUTOR_REVISION: &str = "garive.runtime.collaboration.v1";
/// Exact Safety policy revision used by the headless composition.
pub const COLLABORATION_POLICY_REVISION: &str = "garive.collaboration.policy.v1";

/// Immutable authenticated origin derived from one active durable Execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutonomousCollaborationOrigin {
    session_id: String,
    turn_id: String,
    agent_instance_id: String,
}

impl AutonomousCollaborationOrigin {
    /// Returns the bound Session identity.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Returns the bound Agent identity; model arguments never supply it.
    pub fn agent_instance_id(&self) -> &str {
        &self.agent_instance_id
    }
}

/// One accepted command awaiting post-effect publication.
#[derive(Clone, Debug, Eq, PartialEq)]
enum AutonomousCollaborationCommand {
    Message {
        recipient: Option<String>,
        text: String,
    },
    Delegate {
        assignee: AutonomousAssignee,
        objective: String,
    },
    ForkSelf {
        objective: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AutonomousAssignee {
    Named(String),
    Anonymous(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AcceptedCommand {
    invocation_id: String,
    origin: AutonomousCollaborationOrigin,
    command: AutonomousCollaborationCommand,
}

/// Process-local publisher queue fed only after an effect receipt is durable.
#[derive(Clone, Default)]
pub struct AutonomousCollaborationOutbox {
    pending: Arc<Mutex<BTreeMap<String, AcceptedCommand>>>,
}

impl AutonomousCollaborationOutbox {
    fn enqueue(&self, command: AcceptedCommand) -> Result<(), ExecutorDispatchError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?;
        match pending.get(&command.invocation_id) {
            Some(existing) if existing != &command => Err(ExecutorDispatchError::ReceiptInvalid),
            Some(_) => Ok(()),
            None => {
                pending.insert(command.invocation_id.clone(), command);
                Ok(())
            }
        }
    }

    /// Reconstructs receipt-committed commands that were not yet published.
    pub fn recover(&self, database_path: impl AsRef<Path>) -> Result<usize, LocalWorkerError> {
        let database_path = database_path.as_ref();
        let ledger = SqliteLedger::open(database_path)
            .map_err(|_| LocalWorkerError::DurabilityUnavailable)?;
        let sessions = ledger
            .list_sessions()
            .map_err(|_| LocalWorkerError::DurabilityUnavailable)?;
        let mut recovered = 0;
        for session in sessions {
            let watermark = ledger
                .session_watermark(&session)
                .map_err(|_| LocalWorkerError::DurabilityUnavailable)?
                .ok_or(LocalWorkerError::InvalidComposition)?;
            let facts = ledger
                .read_facts(&session, 0, watermark.max_position, None)
                .map_err(|_| LocalWorkerError::DurabilityUnavailable)?;
            for prepared in facts.iter().filter(|fact| {
                fact.kind.as_str() == "effect.prepared"
                    && fact.schema_version == 3
                    && fact.tool_invocation_id.is_some()
            }) {
                let invocation = prepared
                    .tool_invocation_id
                    .as_ref()
                    .ok_or(LocalWorkerError::InvalidComposition)?;
                if facts
                    .iter()
                    .any(|fact| command_was_published(fact, invocation.as_str()))
                {
                    continue;
                }
                let has_receipt = facts.iter().any(|fact| {
                    fact.kind.as_str() == "effect.receipt"
                        && fact.tool_invocation_id.as_ref() == Some(invocation)
                        && serde_json::from_str::<Value>(fact.payload.as_json()).is_ok_and(
                            |value| {
                                value.get("executor_id").and_then(Value::as_str)
                                    == Some(COLLABORATION_EXECUTOR_ID)
                                    && value.get("classification").and_then(Value::as_str)
                                        == Some("completed")
                            },
                        )
                });
                if !has_receipt {
                    continue;
                }
                let value: Value = serde_json::from_str(prepared.payload.as_json())
                    .map_err(|_| LocalWorkerError::InvalidComposition)?;
                let tool_name =
                    text(&value, "tool_name").map_err(|_| LocalWorkerError::InvalidComposition)?;
                if !matches!(
                    tool_name,
                    MESSAGE_AGENT_TOOL | DELEGATE_TOOL | FORK_SELF_TOOL
                ) {
                    continue;
                }
                let arguments = canonical_inline(&value, "arguments")?;
                let command = parse_command_parts(tool_name, arguments.as_json())
                    .map_err(|_| LocalWorkerError::InvalidComposition)?
                    .ok_or(LocalWorkerError::InvalidComposition)?;
                let turn_id = prepared
                    .turn_id
                    .as_ref()
                    .ok_or(LocalWorkerError::InvalidComposition)?;
                let started = facts
                    .iter()
                    .find(|fact| {
                        fact.kind.as_str() == "turn.started"
                            && fact.turn_id.as_ref() == Some(turn_id)
                    })
                    .ok_or(LocalWorkerError::InvalidComposition)?;
                let started: Value = serde_json::from_str(started.payload.as_json())
                    .map_err(|_| LocalWorkerError::InvalidComposition)?;
                let origin = AutonomousCollaborationOrigin {
                    session_id: session.as_str().into(),
                    turn_id: turn_id.as_str().into(),
                    agent_instance_id: text(&started, "agent_instance_id")
                        .map_err(|_| LocalWorkerError::InvalidComposition)?
                        .into(),
                };
                validate_command(database_path, &origin, &Some(command.clone()))
                    .map_err(|_| LocalWorkerError::InvalidComposition)?;
                self.enqueue(AcceptedCommand {
                    invocation_id: invocation.as_str().into(),
                    origin,
                    command,
                })
                .map_err(|_| LocalWorkerError::InvalidComposition)?;
                recovered += 1;
            }
        }
        Ok(recovered)
    }

    /// Publishes every accepted command idempotently through the Runtime service.
    pub fn drain(&self, host: &LiveHost) -> AutonomousCollaborationDrain {
        let commands = match self.pending.lock() {
            Ok(pending) => pending.values().cloned().collect::<Vec<_>>(),
            Err(_) => {
                return AutonomousCollaborationDrain {
                    published: 0,
                    retained: 1,
                    delegation_ids: Vec::new(),
                }
            }
        };
        let mut published = 0;
        let mut delegation_ids = Vec::new();
        for accepted in commands {
            let result = publish(host, &accepted);
            if let Ok(delegation_id) = result {
                if let Some(value) = delegation_id {
                    delegation_ids.push((accepted.origin.session_id.clone(), value));
                }
                if let Ok(mut pending) = self.pending.lock() {
                    pending.remove(&accepted.invocation_id);
                    published += 1;
                }
            }
        }
        let retained = self.pending.lock().map_or(1, |pending| pending.len());
        AutonomousCollaborationDrain {
            published,
            retained,
            delegation_ids,
        }
    }
}

/// One bounded outbox drain result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutonomousCollaborationDrain {
    /// Commands published during this drain.
    pub published: usize,
    /// Commands retained after a closed failure.
    pub retained: usize,
    /// Newly or idempotently resolved delegation identities.
    pub delegation_ids: Vec<(String, String)>,
}

/// Prepared-v3 collaboration executor bound to exactly one active Agent.
pub struct AutonomousCollaborationExecutor {
    database_path: PathBuf,
    origin: AutonomousCollaborationOrigin,
    catalogue: CollaborationToolCatalogue,
    outbox: AutonomousCollaborationOutbox,
    accepted: BTreeMap<String, AcceptedCommand>,
}

/// Pure collaboration preparation port for one frozen catalogue.
pub struct AutonomousCollaborationPreparation {
    catalogue: CollaborationToolCatalogue,
}

impl AutonomousCollaborationPreparation {
    /// Constructs the preparation port from the admitted policy revision.
    pub fn new() -> Result<Self, LocalWorkerError> {
        Ok(Self {
            catalogue: CollaborationToolCatalogue::new(COLLABORATION_POLICY_REVISION)
                .map_err(|_| LocalWorkerError::InvalidComposition)?,
        })
    }
}

impl ToolPreparationPort for AutonomousCollaborationPreparation {
    fn prepare(
        &self,
        intent: &ToolIntent,
    ) -> Result<PreparedToolCall, garive_tools::PreparationError> {
        self.catalogue.prepare(intent)
    }
}

impl AutonomousCollaborationExecutor {
    /// Reconstructs an unforgeable origin from one committed Turn.
    pub fn new(
        database_path: impl Into<PathBuf>,
        committed: &CommittedTurn,
        outbox: AutonomousCollaborationOutbox,
    ) -> Result<Self, LocalWorkerError> {
        let database_path = database_path.into();
        let origin = reconstruct_origin(&database_path, committed)?;
        let catalogue = CollaborationToolCatalogue::new(COLLABORATION_POLICY_REVISION)
            .map_err(|_| LocalWorkerError::InvalidComposition)?;
        Ok(Self {
            database_path,
            origin,
            catalogue,
            outbox,
            accepted: BTreeMap::new(),
        })
    }

    /// Returns the authenticated origin captured by this executor.
    pub const fn origin(&self) -> &AutonomousCollaborationOrigin {
        &self.origin
    }
}

impl ExecutorPort for AutonomousCollaborationExecutor {
    fn prepare(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        grant: &InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        validate_binding(&self.catalogue, invocation_id, prepared, grant)?;
        let command = parse_command(prepared)?;
        validate_command(&self.database_path, &self.origin, &command)?;
        if let Some(command) = command {
            self.accepted.insert(
                invocation_id.as_str().to_owned(),
                AcceptedCommand {
                    invocation_id: invocation_id.as_str().to_owned(),
                    origin: self.origin.clone(),
                    command,
                },
            );
        }
        Ok(PreparedExecution {
            executor_id: COLLABORATION_EXECUTOR_ID.into(),
            executor_revision: COLLABORATION_EXECUTOR_REVISION.into(),
            dispatch_attempt_id: collaboration_dispatch_attempt_id(invocation_id),
        })
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        Box::pin(async move {
            validate_execution(&command)?;
            if command.prepared.tool_name() == COLLECT_DELEGATIONS_TOOL {
                return completed(
                    &command,
                    collect(&self.database_path, &self.origin, command.prepared)?,
                );
            }
            let parsed = parse_command(command.prepared)
                .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
            validate_command(&self.database_path, &self.origin, &parsed)
                .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
            if let Some(parsed) = parsed {
                self.accepted
                    .entry(command.invocation_id.as_str().to_owned())
                    .or_insert_with(|| AcceptedCommand {
                        invocation_id: command.invocation_id.as_str().to_owned(),
                        origin: self.origin.clone(),
                        command: parsed,
                    });
            }
            let accepted = self
                .accepted
                .get(command.invocation_id.as_str())
                .ok_or(ExecutorDispatchError::ReceiptInvalid)?;
            completed(
                &command,
                json!({"status":"accepted","command_id":accepted.invocation_id}),
            )
        })
    }

    fn acknowledge_receipt(
        &mut self,
        invocation_id: &ToolInvocationId,
        receipt: &EffectReceipt,
    ) -> Result<(), ExecutorDispatchError> {
        if receipt.executor_id != COLLABORATION_EXECUTOR_ID
            || receipt.executor_revision != COLLABORATION_EXECUTOR_REVISION
            || receipt.invocation_id != *invocation_id
        {
            return Err(ExecutorDispatchError::ReceiptInvalid);
        }
        if let Some(accepted) = self.accepted.remove(invocation_id.as_str()) {
            self.outbox.enqueue(accepted)?;
        }
        Ok(())
    }

    fn reconcile_started_loss(
        &mut self,
        request: crate::ExecutorRecoveryRequest<'_>,
    ) -> Result<(), ExecutorDispatchError> {
        if request.executor_id == COLLABORATION_EXECUTOR_ID
            && request.executor_revision == COLLABORATION_EXECUTOR_REVISION
        {
            Ok(())
        } else {
            Err(ExecutorDispatchError::ReceiptInvalid)
        }
    }
}

fn reconstruct_origin(
    database_path: &Path,
    committed: &CommittedTurn,
) -> Result<AutonomousCollaborationOrigin, LocalWorkerError> {
    let ledger =
        SqliteLedger::open(database_path).map_err(|_| LocalWorkerError::InvalidComposition)?;
    let facts = ledger
        .read_facts(&committed.session_id, 0, committed.committed_position, None)
        .map_err(|_| LocalWorkerError::InvalidComposition)?;
    let started = facts
        .iter()
        .filter(|fact| {
            fact.turn_id.as_ref() == Some(&committed.turn_id)
                && fact.kind.as_str() == "turn.started"
        })
        .max_by_key(|fact| fact.position)
        .ok_or(LocalWorkerError::InvalidComposition)?;
    let value: Value = serde_json::from_str(started.payload.as_json())
        .map_err(|_| LocalWorkerError::InvalidComposition)?;
    let agent_instance_id = value
        .get("agent_instance_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(LocalWorkerError::InvalidComposition)?;
    Ok(AutonomousCollaborationOrigin {
        session_id: committed.session_id.as_str().into(),
        turn_id: committed.turn_id.as_str().into(),
        agent_instance_id: agent_instance_id.into(),
    })
}

fn validate_binding(
    catalogue: &CollaborationToolCatalogue,
    invocation_id: &ToolInvocationId,
    prepared: &PreparedToolCall,
    grant: &InvocationGrant,
) -> Result<(), String> {
    let reconstructed = catalogue
        .prepare(&ToolIntent::new(
            prepared.model_call_id(),
            prepared.tool_name(),
            prepared.normalized_arguments(),
        ))
        .map_err(|_| "collaboration definition mismatch".to_owned())?;
    if reconstructed != *prepared
        || prepared.contract_version() != 3
        || grant.invocation_id != *invocation_id
        || grant.prepared_digest != prepared.input_digest()
        || grant.tool_name != prepared.tool_name()
        || grant.tool_revision != prepared.tool_revision()
        || grant.granted_requirements != *prepared.requirements()
    {
        return Err("invalid collaboration execution binding".into());
    }
    Ok(())
}

fn validate_execution(command: &ExecutorDispatch<'_>) -> Result<(), ExecutorDispatchError> {
    if command.execution.executor_id != COLLABORATION_EXECUTOR_ID
        || command.execution.executor_revision != COLLABORATION_EXECUTOR_REVISION
        || command.execution.dispatch_attempt_id
            != collaboration_dispatch_attempt_id(command.invocation_id)
    {
        Err(ExecutorDispatchError::ReceiptInvalid)
    } else {
        Ok(())
    }
}

fn parse_command(
    prepared: &PreparedToolCall,
) -> Result<Option<AutonomousCollaborationCommand>, String> {
    parse_command_parts(prepared.tool_name(), prepared.normalized_arguments())
}

fn parse_command_parts(
    tool_name: &str,
    arguments: &str,
) -> Result<Option<AutonomousCollaborationCommand>, String> {
    let value: Value = serde_json::from_str(arguments)
        .map_err(|_| "invalid collaboration arguments".to_owned())?;
    match tool_name {
        MESSAGE_AGENT_TOOL => Ok(Some(AutonomousCollaborationCommand::Message {
            recipient: optional_text(&value, "recipient")?,
            text: text(&value, "text")?.into(),
        })),
        DELEGATE_TOOL => {
            let assignee = value
                .get("assignee")
                .ok_or_else(|| "missing collaboration assignee".to_owned())?;
            let assignee = match text(assignee, "kind")? {
                "named" => AutonomousAssignee::Named(text(assignee, "agent_name")?.into()),
                "anonymous" => {
                    AutonomousAssignee::Anonymous(text(assignee, "definition_id")?.into())
                }
                _ => return Err("invalid collaboration assignee".into()),
            };
            Ok(Some(AutonomousCollaborationCommand::Delegate {
                assignee,
                objective: text(&value, "objective")?.into(),
            }))
        }
        FORK_SELF_TOOL => Ok(Some(AutonomousCollaborationCommand::ForkSelf {
            objective: text(&value, "objective")?.into(),
        })),
        COLLECT_DELEGATIONS_TOOL => Ok(None),
        _ => Err("unsupported collaboration tool".into()),
    }
}

fn canonical_inline(value: &Value, key: &str) -> Result<CanonicalPayload, LocalWorkerError> {
    let binding = value
        .get(key)
        .and_then(Value::as_object)
        .ok_or(LocalWorkerError::InvalidComposition)?;
    CanonicalPayload::from_canonical_parts(
        binding
            .get("inline_utf8")
            .and_then(Value::as_str)
            .ok_or(LocalWorkerError::InvalidComposition)?
            .into(),
        binding
            .get("digest")
            .and_then(Value::as_str)
            .ok_or(LocalWorkerError::InvalidComposition)?
            .into(),
    )
    .map_err(|_| LocalWorkerError::InvalidComposition)
}

fn command_was_published(fact: &garive_ledger::DurableFact, invocation_id: &str) -> bool {
    matches!(
        fact.kind.as_str(),
        "session.agent_message" | "collaboration.delegation_requested"
    ) && serde_json::from_str::<Value>(fact.payload.as_json())
        .is_ok_and(|value| value.get("command_id").and_then(Value::as_str) == Some(invocation_id))
}

fn validate_command(
    database_path: &Path,
    origin: &AutonomousCollaborationOrigin,
    command: &Option<AutonomousCollaborationCommand>,
) -> Result<(), String> {
    let ledger = SqliteLedger::open(database_path).map_err(|_| "collaboration unavailable")?;
    let session = SessionId::try_from(origin.session_id.as_str())
        .map_err(|_| "invalid collaboration origin")?;
    let watermark = ledger
        .session_watermark(&session)
        .map_err(|_| "collaboration unavailable")?
        .ok_or("collaboration unavailable")?;
    let facts = ledger
        .read_facts(&session, 0, watermark.max_position, None)
        .map_err(|_| "collaboration unavailable")?;
    let members = roster(&facts)?;
    if !members
        .iter()
        .any(|member| member.id == origin.agent_instance_id)
    {
        return Err("collaboration actor is not a Session member".into());
    }
    match command {
        Some(AutonomousCollaborationCommand::Message {
            recipient: Some(name),
            ..
        })
        | Some(AutonomousCollaborationCommand::Delegate {
            assignee: AutonomousAssignee::Named(name),
            ..
        }) => {
            let target = members
                .iter()
                .find(|member| member.name == *name)
                .ok_or_else(|| "named collaboration target is unavailable".to_owned())?;
            if target.id == origin.agent_instance_id {
                return Err("collaboration target cannot be the active Agent".into());
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn validate_collaboration_admission(
    database_path: &Path,
    origin: &AutonomousCollaborationOrigin,
    prepared: &PreparedToolCall,
) -> Result<(), String> {
    let command = parse_command(prepared)?;
    validate_command(database_path, origin, &command)
}

fn publish(host: &LiveHost, accepted: &AcceptedCommand) -> Result<Option<String>, ()> {
    let session = accepted.origin.session_id.as_str();
    let actor = accepted.origin.agent_instance_id.as_str();
    match &accepted.command {
        AutonomousCollaborationCommand::Message { recipient, text } => {
            let roster = host.get_session_agents(session).map_err(|_| ())?;
            let target = recipient
                .as_ref()
                .map(|name| {
                    roster
                        .members
                        .iter()
                        .find(|member| member.display_name == *name)
                        .map(|member| member.agent_instance_id.as_str())
                        .ok_or(())
                })
                .transpose()?;
            host.send_session_agent_message(&accepted.invocation_id, session, actor, target, text)
                .map_err(|_| ())?;
            Ok(None)
        }
        AutonomousCollaborationCommand::Delegate {
            assignee,
            objective,
        } => {
            let assignee = match assignee {
                AutonomousAssignee::Named(name) => {
                    let roster = host.get_session_agents(session).map_err(|_| ())?;
                    let target = roster
                        .members
                        .iter()
                        .find(|member| member.display_name == *name)
                        .ok_or(())?;
                    DelegationAssigneeBody::Named {
                        agent_instance_id: target.agent_instance_id.clone(),
                    }
                }
                AutonomousAssignee::Anonymous(definition_id) => DelegationAssigneeBody::Anonymous {
                    agent_definition_id: definition_id.clone(),
                },
            };
            let response = host
                .dispatch_agent_task(
                    &accepted.invocation_id,
                    session,
                    actor,
                    assignee,
                    "notify",
                    objective,
                )
                .map_err(|_| ())?;
            Ok(Some(response.delegation_id))
        }
        AutonomousCollaborationCommand::ForkSelf { objective } => {
            let response = host
                .dispatch_agent_task(
                    &accepted.invocation_id,
                    session,
                    actor,
                    DelegationAssigneeBody::ForkSelf,
                    "notify",
                    objective,
                )
                .map_err(|_| ())?;
            Ok(Some(response.delegation_id))
        }
    }
}

fn collect(
    database_path: &Path,
    origin: &AutonomousCollaborationOrigin,
    prepared: &PreparedToolCall,
) -> Result<Value, ExecutorDispatchError> {
    let arguments: Value = serde_json::from_str(prepared.normalized_arguments())
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
    let maximum = arguments
        .get("max_results")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(ExecutorDispatchError::ReceiptInvalid)?;
    let ledger = SqliteLedger::open(database_path)
        .map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?;
    let session = SessionId::try_from(origin.session_id.as_str())
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
    let watermark = ledger
        .session_watermark(&session)
        .map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?
        .ok_or(ExecutorDispatchError::ReceiptInvalid)?;
    let facts = ledger
        .read_facts(&session, 0, watermark.max_position, None)
        .map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?;
    let mut values = Vec::new();
    for fact in facts.iter().rev() {
        if fact.kind.as_str() != "collaboration.delegation_requested" {
            continue;
        }
        let request: Value = serde_json::from_str(fact.payload.as_json())
            .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
        if request
            .get("dispatcher_agent_instance_id")
            .and_then(Value::as_str)
            != Some(origin.agent_instance_id.as_str())
        {
            continue;
        }
        let delegation_id =
            text(&request, "delegation_id").map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
        let delivered = facts.iter().find(|candidate| {
            candidate.kind.as_str() == "collaboration.result_delivered"
                && serde_json::from_str::<Value>(candidate.payload.as_json()).is_ok_and(|value| {
                    value.get("delegation_id").and_then(Value::as_str) == Some(delegation_id)
                })
        });
        let value = match delivered {
            Some(delivered) => {
                let delivered: Value = serde_json::from_str(delivered.payload.as_json())
                    .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
                json!({"delegation_id":delegation_id,"state":"delivered","result":delivered.get("result").cloned().ok_or(ExecutorDispatchError::ReceiptInvalid)?})
            }
            None => json!({"delegation_id":delegation_id,"state":"active"}),
        };
        values.push(value);
        if values.len() == maximum {
            break;
        }
    }
    values.reverse();
    Ok(json!({"delegations":values,"observed_max_position":watermark.max_position}))
}

#[derive(Deserialize)]
struct RosterMember {
    id: String,
    name: String,
}

fn roster(facts: &[garive_ledger::DurableFact]) -> Result<Vec<RosterMember>, String> {
    facts
        .iter()
        .filter(|fact| {
            matches!(
                fact.kind.as_str(),
                "session.opened" | "session.agent_joined"
            )
        })
        .map(|fact| {
            let value: Value = serde_json::from_str(fact.payload.as_json())
                .map_err(|_| "invalid Session roster".to_owned())?;
            Ok(RosterMember {
                id: text(&value, "agent_instance_id")?.into(),
                name: text(&value, "agent_name")?.into(),
            })
        })
        .collect()
}

fn completed(
    command: &ExecutorDispatch<'_>,
    content: Value,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    let digest = CanonicalPayload::from_value(&content)
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?
        .sha256()
        .to_owned();
    Ok(ExecutionFact::Completed {
        receipt: Some(EffectReceipt {
            receipt_id: ReceiptId::new(command.receipt_id)
                .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?,
            invocation_id: command.invocation_id.clone(),
            prepared_digest: command.prepared.input_digest().into(),
            grant_id: command.grant.grant_id.clone(),
            executor_id: command.execution.executor_id.clone(),
            executor_revision: command.execution.executor_revision.clone(),
            terminal_classification: TerminalClassification::Completed,
            result_digest: digest,
        }),
        content,
        truncated: false,
    })
}

pub(crate) fn collaboration_dispatch_attempt_id(invocation_id: &ToolInvocationId) -> String {
    format!(
        "collaboration-dispatch-{:x}",
        Sha256::digest(invocation_id.as_str().as_bytes())
    )
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing {key}"))
}

fn optional_text(value: &Value, key: &str) -> Result<Option<String>, String> {
    match value.get(key) {
        Some(value) => value
            .as_str()
            .filter(|value| !value.is_empty())
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| format!("invalid {key}")),
        None => Ok(None),
    }
}
