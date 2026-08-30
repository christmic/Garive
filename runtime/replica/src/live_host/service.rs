use std::{fmt::Write, path::Path, sync::Arc};

use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload,
    CommitDisposition, DurableFact, ExecutionId, FactDraft, FactId, FactKind, LedgerError,
    SessionId, TurnId,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    commit_planned_turn, get_turn, plan_cancel_turn, plan_continue_turn, plan_start_turn,
    reconstruct_suspended_turn, CancelReason, CancelTurnCommand, ContinuationInput,
    ContinueTurnCommand, GetTurnQuery, InteractionInputRepresentation, RuntimeCommandError,
    RuntimeCommandId, RuntimeSuspensionKind, RuntimeTurnStatus, SqliteLedger, SqliteLedgerError,
    StartTurnCommand,
};

use super::{
    project_activities, project_fact, AgentDefinitionPageV1, AgentDefinitionSummaryV1,
    CommittedTurn, CreateSessionResponse, HostClock, HostContinuationInput, HostEventPage,
    HostReadLimits, InstalledAgent, LiveHostError, LiveHostEvent, LiveHostLimits, LiveHostState,
    SessionPageV1, SessionViewV1, TurnCommandResponse, TurnDispatcher, TurnTimelinePageV1,
};
use super::{read_cursor, read_model, timeline_projection};

/// Durable local Host command service shared by in-process and HTTP clients.
#[derive(Clone)]
pub struct LiveHost {
    pub(crate) state: Arc<LiveHostState>,
}

impl LiveHost {
    /// Constructs a Host from explicit storage, installation, limits, clock and dispatcher.
    pub fn new(
        database_path: impl AsRef<Path>,
        installed: InstalledAgent,
        limits: LiveHostLimits,
        clock: Arc<dyn HostClock>,
        dispatcher: Arc<dyn TurnDispatcher>,
    ) -> Result<Self, LiveHostError> {
        Self::new_with_read_limits(
            database_path,
            installed,
            limits,
            HostReadLimits::PRODUCT_DEFAULT,
            clock,
            dispatcher,
        )
    }

    /// Constructs a Host with explicit independent H2 projection bounds.
    pub fn new_with_read_limits(
        database_path: impl AsRef<Path>,
        installed: InstalledAgent,
        limits: LiveHostLimits,
        read_limits: HostReadLimits,
        clock: Arc<dyn HostClock>,
        dispatcher: Arc<dyn TurnDispatcher>,
    ) -> Result<Self, LiveHostError> {
        validate_installed(&installed, limits)?;
        if !read_limits.valid() {
            return Err(LiveHostError::InvalidRequest);
        }
        SqliteLedger::open(database_path.as_ref()).map_err(map_sqlite)?;
        Ok(Self {
            state: Arc::new(LiveHostState {
                database_path: database_path.as_ref().to_owned(),
                installed,
                limits,
                read_limits,
                clock,
                dispatcher,
            }),
        })
    }

    /// Lists installed Agent definitions without exposing Runtime configuration.
    pub fn list_agent_definitions(&self) -> Result<AgentDefinitionPageV1, LiveHostError> {
        let page = AgentDefinitionPageV1 {
            api_version: "v1",
            definitions: vec![AgentDefinitionSummaryV1 {
                api_version: "v1",
                definition_id: self.state.installed.definition_id.clone(),
                definition_revision: self.state.installed.definition_revision.clone(),
                capabilities: self.state.installed.public_capabilities.clone(),
            }],
        };
        if page.definitions.len() > self.state.read_limits.max_definitions
            || serde_json::to_vec(&page)
                .map_err(|_| LiveHostError::CorruptState)?
                .len()
                > self.state.read_limits.max_response_bytes
        {
            return Err(LiveHostError::ReadBoundExceeded);
        }
        Ok(page)
    }

    /// Reads one verified Session summary at an exact durable watermark.
    pub fn get_session(&self, session: &str) -> Result<SessionViewV1, LiveHostError> {
        let session_id = identity::<SessionId>(session)?;
        let ledger = self.ledger()?;
        let watermark = ledger
            .session_watermark(&session_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::NotFound)?;
        let facts = ledger
            .read_facts(&session_id, 0, watermark.max_position, None)
            .map_err(map_sqlite)?;
        let view = read_model::project_session(
            &session_id,
            watermark.max_position,
            &facts,
            &self.state.installed,
            self.state.read_limits,
        )?;
        if serde_json::to_vec(&view)
            .map_err(|_| LiveHostError::CorruptState)?
            .len()
            > self.state.read_limits.max_response_bytes
        {
            return Err(LiveHostError::ReadBoundExceeded);
        }
        Ok(view)
    }

    /// Lists a reverse-opened page of verified durable Sessions.
    pub fn list_sessions(
        &self,
        limit: usize,
        before: Option<&str>,
    ) -> Result<SessionPageV1, LiveHostError> {
        if limit == 0 || limit > self.state.read_limits.max_sessions {
            return Err(LiveHostError::InvalidRequest);
        }
        let mut sessions = self
            .ledger()?
            .list_sessions()
            .map_err(map_sqlite)?
            .into_iter()
            .map(|id| {
                let session = self.get_session(id.as_str())?.session;
                let opened = chrono::DateTime::parse_from_rfc3339(&session.opened_at)
                    .map_err(|_| LiveHostError::CorruptState)?;
                Ok((opened, session))
            })
            .collect::<Result<Vec<_>, _>>()?;
        sessions.sort_by(|(left_time, left), (right_time, right)| {
            right_time
                .cmp(left_time)
                .then_with(|| right.session_id.cmp(&left.session_id))
        });
        let sessions = sessions
            .into_iter()
            .map(|(_, session)| session)
            .collect::<Vec<_>>();
        let start = before.map_or(Ok(0), |token| {
            let key = read_cursor::decode(token, &self.state.installed, self.state.read_limits)?;
            sessions
                .iter()
                .position(|item| item.opened_at == key.0 && item.session_id == key.1)
                .map(|index| index + 1)
                .ok_or(LiveHostError::InvalidRequest)
        })?;
        let end = start.saturating_add(limit).min(sessions.len());
        let page_sessions = sessions[start..end].to_vec();
        let next_before = (end < sessions.len())
            .then(|| page_sessions.last())
            .flatten()
            .map(|item| read_cursor::encode(item, &self.state.installed, self.state.read_limits))
            .transpose()?;
        let page = SessionPageV1 {
            api_version: "v1",
            sessions: page_sessions,
            next_before,
        };
        ensure_response_bound(&page, self.state.read_limits.max_response_bytes)?;
        Ok(page)
    }

    /// Reads a bounded page of complete Turns from one frozen Session prefix.
    pub fn get_timeline(
        &self,
        session: &str,
        after_position: u64,
        limit: usize,
    ) -> Result<TurnTimelinePageV1, LiveHostError> {
        if limit == 0 || limit > self.state.read_limits.max_timeline_items {
            return Err(LiveHostError::InvalidRequest);
        }
        let session_id = identity::<SessionId>(session)?;
        let ledger = self.ledger()?;
        let watermark = ledger
            .session_watermark(&session_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::NotFound)?;
        if after_position > watermark.max_position {
            return Err(LiveHostError::InvalidRequest);
        }
        let facts = ledger
            .read_facts(&session_id, 0, watermark.max_position, None)
            .map_err(map_sqlite)?;
        read_model::project_session(
            &session_id,
            watermark.max_position,
            &facts,
            &self.state.installed,
            self.state.read_limits,
        )?;
        let activities = match (
            self.state.installed.public_activity_catalogue.as_ref(),
            self.state.limits.activity,
        ) {
            (Some(catalogue), Some(limits)) => {
                project_activities(&facts, catalogue, limits)?.by_turn
            }
            (None, None) => Default::default(),
            _ => return Err(LiveHostError::CorruptState),
        };
        let page =
            timeline_projection::project_timeline(timeline_projection::TimelineProjectionInput {
                session_id: &session_id,
                observed_max_position: watermark.max_position,
                session_version: watermark.session_version,
                after_position,
                limit,
                facts: &facts,
                activities,
                limits: self.state.read_limits,
            })?;
        if serde_json::to_vec(&page)
            .map_err(|_| LiveHostError::CorruptState)?
            .len()
            > self.state.read_limits.max_response_bytes
        {
            return Err(LiveHostError::ReadBoundExceeded);
        }
        Ok(page)
    }

    /// Returns explicit Host bounds used by HTTP parsing and event follow mode.
    pub fn limits(&self) -> LiveHostLimits {
        self.state.limits
    }

    /// Creates or exactly replays one durable Session creation command.
    pub fn create_session(
        &self,
        idempotency_key: &str,
        agent_definition_id: &str,
    ) -> Result<CreateSessionResponse, LiveHostError> {
        validate_key(idempotency_key)?;
        if agent_definition_id != self.state.installed.definition_id {
            return Err(LiveHostError::NotFound);
        }
        let session_id = SessionId::try_from(
            format!(
                "session-{}",
                digest(
                    format!(
                        "{}:{idempotency_key}",
                        self.state.installed.agent_instance_namespace
                    )
                    .as_bytes()
                )
            )
            .as_str(),
        )
        .map_err(|_| LiveHostError::InvalidRequest)?;
        let agent_instance_id = AgentInstanceId::try_from(
            format!(
                "agent-{}",
                digest(
                    format!(
                        "{}:{}:{}",
                        session_id.as_str(),
                        self.state.installed.definition_id,
                        self.state.installed.definition_revision
                    )
                    .as_bytes()
                )
            )
            .as_str(),
        )
        .map_err(|_| LiveHostError::InvalidRequest)?;
        let payload = json!({
            "command_id": idempotency_key,
            "definition_id": self.state.installed.definition_id,
            "definition_revision": self.state.installed.definition_revision,
            "snapshot_digest": self.state.installed.snapshot_digest,
            "agent_instance_id": agent_instance_id.as_str(),
        });
        let recorded_at = self.recorded_at()?;
        let fact = FactDraft {
            fact_id: FactId::try_from(
                format!(
                    "fact-{}",
                    digest(format!("{}:session.opened", session_id.as_str()).as_bytes())
                )
                .as_str(),
            )
            .map_err(|_| LiveHostError::InvalidRequest)?,
            turn_id: None,
            execution_id: None,
            model_request_id: None,
            tool_invocation_id: None,
            kind: FactKind::new("session.opened").map_err(|_| LiveHostError::InvalidRequest)?,
            schema_version: 1,
            payload: CanonicalPayload::from_value(&payload)
                .map_err(|_| LiveHostError::InvalidRequest)?,
            recorded_at,
        };
        let mut ledger = self.ledger()?;
        let committed = ledger
            .commit(session_id.clone(), 0, vec![fact])
            .map_err(map_sqlite)?;
        Ok(CreateSessionResponse {
            session_id: session_id.as_str().to_owned(),
            agent_instance_id: agent_instance_id.as_str().to_owned(),
            committed_position: only_position(&committed.positions)?,
        })
    }

    /// Starts or exactly replays one C6 Turn transaction, then dispatches only a new commit.
    pub fn start_turn(
        &self,
        idempotency_key: &str,
        session: &str,
        trusted_input: &str,
    ) -> Result<TurnCommandResponse, LiveHostError> {
        validate_key(idempotency_key)?;
        validate_text(trusted_input, self.state.limits.max_command_bytes)?;
        let session_id = identity::<SessionId>(session)?;
        let mut ledger = self.ledger()?;
        let binding = self.load_session(&ledger, &session_id)?;
        if let Some(response) = self.replay_start(
            &ledger,
            &session_id,
            idempotency_key,
            trusted_input,
            binding.max_position,
        )? {
            return Ok(response);
        }
        let plan = plan_start_turn(
            &StartTurnCommand {
                command_id: RuntimeCommandId::new(idempotency_key).map_err(map_runtime)?,
                session_id: session_id.clone(),
                agent_instance_id: binding.agent_instance_id,
                definition_id: binding.definition_id,
                definition_revision: binding.definition_revision,
                snapshot_digest: binding.snapshot_digest,
                trusted_input: trusted_input.to_owned(),
                limits: self.state.installed.runtime_limits,
                recorded_at: self.recorded_at()?,
            },
            binding.max_position,
        )
        .map_err(map_runtime)?;
        let execution_id = plan
            .execution_id
            .clone()
            .ok_or(LiveHostError::CorruptState)?;
        let committed = commit_planned_turn(
            &mut ledger,
            session_id.clone(),
            binding.session_version,
            &plan,
        )
        .map_err(map_runtime)?;
        let last = last_position(&committed.positions)?;
        if committed.disposition == CommitDisposition::Committed {
            let _ = self.state.dispatcher.dispatch(&CommittedTurn {
                session_id: session_id.clone(),
                turn_id: plan.turn_id.clone(),
                execution_id: execution_id.clone(),
                session_version: committed.session_version,
                committed_position: last,
            });
        }
        Ok(turn_response(
            &session_id,
            &plan.turn_id,
            Some(&execution_id),
            last,
        ))
    }

    /// Durably requests cancellation without claiming that work already stopped.
    pub fn cancel_turn(
        &self,
        idempotency_key: &str,
        session: &str,
        turn: &str,
        requested_through_position: u64,
    ) -> Result<TurnCommandResponse, LiveHostError> {
        validate_key(idempotency_key)?;
        let session_id = identity::<SessionId>(session)?;
        let turn_id = identity::<TurnId>(turn)?;
        let mut ledger = self.ledger()?;
        let view = get_turn(
            &ledger,
            &GetTurnQuery {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                through_position: None,
            },
        )
        .map_err(map_runtime_query)?;
        if let Some(position) = self.replay_cancel(
            &ledger,
            &session_id,
            &turn_id,
            idempotency_key,
            requested_through_position,
            view.through_position,
        )? {
            return Ok(turn_response(
                &session_id,
                &turn_id,
                view.execution_id.as_ref(),
                position,
            ));
        }
        if view.status != RuntimeTurnStatus::Open
            || requested_through_position == 0
            || requested_through_position > view.through_position
        {
            return Err(LiveHostError::PreconditionFailed);
        }
        let plan = plan_cancel_turn(&CancelTurnCommand {
            command_id: RuntimeCommandId::new(idempotency_key).map_err(map_runtime)?,
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            reason: CancelReason::User,
            requested_through_position,
            recorded_at: self.recorded_at()?,
        })
        .map_err(map_runtime)?;
        let committed = commit_planned_turn(
            &mut ledger,
            session_id.clone(),
            view.observed_session_version,
            &plan,
        )
        .map_err(map_runtime)?;
        Ok(turn_response(
            &session_id,
            &turn_id,
            view.execution_id.as_ref(),
            last_position(&committed.positions)?,
        ))
    }

    /// Continues one exact external-input suspension with a fresh Execution.
    pub fn continue_turn(
        &self,
        idempotency_key: &str,
        session: &str,
        turn: &str,
        suspension_id: &str,
        expected_session_version: u64,
        input: HostContinuationInput<'_>,
    ) -> Result<TurnCommandResponse, LiveHostError> {
        validate_key(idempotency_key)?;
        validate_continuation_bytes(input, self.state.limits.max_command_bytes)?;
        let session_id = identity::<SessionId>(session)?;
        let turn_id = identity::<TurnId>(turn)?;
        let mut ledger = self.ledger()?;
        if let Some(response) = self.replay_continue(
            &ledger,
            &session_id,
            &turn_id,
            ContinueReplay {
                command_id: idempotency_key,
                suspension_id,
                expected_session_version,
                input,
            },
        )? {
            return Ok(response);
        }
        let snapshot = ledger.load_turn(&turn_id).map_err(map_sqlite_query)?;
        if snapshot.facts.first().map(|fact| &fact.session_id) != Some(&session_id) {
            return Err(LiveHostError::NotFound);
        }
        let state = reconstruct_suspended_turn(&snapshot).map_err(map_runtime)?;
        if state.suspension_id != suspension_id
            || state.session_version != expected_session_version
            || !matches!(
                state.suspension_kind,
                RuntimeSuspensionKind::ApprovalRequired
                    | RuntimeSuspensionKind::ExternalInputRequired
                    | RuntimeSuspensionKind::PartialOutput
            )
        {
            return Err(LiveHostError::PreconditionFailed);
        }
        let continuation_input = continuation_input(&state, input)?;
        let plan = plan_continue_turn(
            &ContinueTurnCommand {
                command_id: RuntimeCommandId::new(idempotency_key).map_err(map_runtime)?,
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                expected_suspension_id: suspension_id.to_owned(),
                expected_session_version,
                continuation_input,
                interaction: state.interaction.clone(),
                recorded_at: self.recorded_at()?,
            },
            &state,
        )
        .map_err(map_runtime)?;
        let execution_id = plan
            .execution_id
            .clone()
            .ok_or(LiveHostError::CorruptState)?;
        let committed = commit_planned_turn(
            &mut ledger,
            session_id.clone(),
            expected_session_version,
            &plan,
        )
        .map_err(map_runtime)?;
        let last = last_position(&committed.positions)?;
        if committed.disposition == CommitDisposition::Committed {
            let _ = self.state.dispatcher.dispatch(&CommittedTurn {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                execution_id: execution_id.clone(),
                session_version: committed.session_version,
                committed_position: last,
            });
        }
        Ok(turn_response(
            &session_id,
            &turn_id,
            Some(&execution_id),
            last,
        ))
    }

    /// Scans one bounded durable position range and projects admitted public events.
    pub fn read_event_page(
        &self,
        session: &str,
        after_position: u64,
    ) -> Result<HostEventPage, LiveHostError> {
        let session_id = identity::<SessionId>(session)?;
        let ledger = self.ledger()?;
        let watermark = ledger
            .session_watermark(&session_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::NotFound)?;
        if after_position > watermark.max_position {
            return Err(LiveHostError::InvalidRequest);
        }
        if after_position == watermark.max_position {
            return Ok(HostEventPage {
                events: Vec::new(),
                scanned_through_position: after_position,
                observed_max_position: watermark.max_position,
            });
        }
        let through = after_position
            .saturating_add(self.state.limits.event_batch_size)
            .min(watermark.max_position);
        let activity_enabled = self.state.limits.activity.is_some();
        let facts = ledger
            .read_facts(
                &session_id,
                if activity_enabled { 0 } else { after_position },
                through,
                None,
            )
            .map_err(map_sqlite)?;
        let activities = match (
            self.state.installed.public_activity_catalogue.as_ref(),
            self.state.limits.activity,
        ) {
            (Some(catalogue), Some(limits)) => {
                Some(project_activities(&facts, catalogue, limits)?.events)
            }
            (None, None) => None,
            _ => return Err(LiveHostError::CorruptState),
        };
        let mut events = Vec::new();
        for fact in facts.iter().filter(|fact| fact.position > after_position) {
            if let Some(activity) = activities
                .as_ref()
                .and_then(|items| items.get(&fact.position))
            {
                events.push(LiveHostEvent {
                    api_version: "v1",
                    session_id: fact.session_id.as_str().to_owned(),
                    position: fact.position,
                    event: activity.event.into(),
                    turn_id: fact
                        .turn_id
                        .as_ref()
                        .map_or_else(String::new, |value| value.as_str().to_owned()),
                    execution_id: fact
                        .execution_id
                        .as_ref()
                        .map_or_else(String::new, |value| value.as_str().to_owned()),
                    text: String::new(),
                    activity: Some(activity.activity.clone()),
                });
            } else if let Some(event) = project_fact(fact)? {
                events.push(event);
            }
        }
        Ok(HostEventPage {
            events,
            scanned_through_position: through,
            observed_max_position: watermark.max_position,
        })
    }

    fn ledger(&self) -> Result<SqliteLedger, LiveHostError> {
        SqliteLedger::open(&self.state.database_path).map_err(map_sqlite)
    }

    fn recorded_at(&self) -> Result<String, LiveHostError> {
        let value = self.state.clock.recorded_at();
        chrono::DateTime::parse_from_rfc3339(&value)
            .map(|_| value)
            .map_err(|_| LiveHostError::InvalidRequest)
    }

    fn load_session(
        &self,
        ledger: &SqliteLedger,
        session_id: &SessionId,
    ) -> Result<SessionBinding, LiveHostError> {
        let watermark = ledger
            .session_watermark(session_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::NotFound)?;
        let facts = ledger
            .read_facts(session_id, 0, watermark.max_position, None)
            .map_err(map_sqlite)?;
        if facts
            .iter()
            .any(|fact| fact.kind.as_str() == "session.closed")
        {
            return Err(LiveHostError::PreconditionFailed);
        }
        let first = facts.first().ok_or(LiveHostError::CorruptState)?;
        if first.position != 1 || first.kind.as_str() != "session.opened" {
            return Err(LiveHostError::CorruptState);
        }
        let payload: SessionOpened = serde_json::from_str(first.payload.as_json())
            .map_err(|_| LiveHostError::CorruptState)?;
        if payload.command_id.is_empty()
            || payload.definition_id != self.state.installed.definition_id
            || payload.definition_revision != self.state.installed.definition_revision
            || payload.snapshot_digest != self.state.installed.snapshot_digest
        {
            return Err(LiveHostError::CorruptState);
        }
        Ok(SessionBinding {
            agent_instance_id: identity(&payload.agent_instance_id)
                .map_err(|_| LiveHostError::CorruptState)?,
            definition_id: identity(&payload.definition_id)
                .map_err(|_| LiveHostError::CorruptState)?,
            definition_revision: identity(&payload.definition_revision)
                .map_err(|_| LiveHostError::CorruptState)?,
            snapshot_digest: payload.snapshot_digest,
            session_version: watermark.session_version,
            max_position: watermark.max_position,
        })
    }

    fn replay_start(
        &self,
        ledger: &SqliteLedger,
        session_id: &SessionId,
        command_id: &str,
        input: &str,
        through_position: u64,
    ) -> Result<Option<TurnCommandResponse>, LiveHostError> {
        let facts = ledger
            .read_facts(session_id, 0, through_position, None)
            .map_err(map_sqlite)?;
        let Some((index, started)) = find_started(&facts, command_id)? else {
            reject_other_command(&facts, command_id)?;
            return Ok(None);
        };
        if started.kind != "start"
            || started.definition_id != self.state.installed.definition_id
            || started.definition_revision != self.state.installed.definition_revision
            || started.snapshot_digest != self.state.installed.snapshot_digest
            || started.trusted_input_digest != digest(input.as_bytes())
        {
            return Err(LiveHostError::CommandConflict);
        }
        replay_started_batch(session_id, &facts, index, ReplayInput::Start(input), None)
    }

    fn replay_continue(
        &self,
        ledger: &SqliteLedger,
        session_id: &SessionId,
        turn_id: &TurnId,
        request: ContinueReplay<'_>,
    ) -> Result<Option<TurnCommandResponse>, LiveHostError> {
        let Some(watermark) = ledger.session_watermark(session_id).map_err(map_sqlite)? else {
            return Err(LiveHostError::NotFound);
        };
        let facts = ledger
            .read_facts(session_id, 0, watermark.max_position, None)
            .map_err(map_sqlite)?;
        let Some((index, started)) = find_started(&facts, request.command_id)? else {
            reject_other_command(&facts, request.command_id)?;
            return Ok(None);
        };
        if started.kind != "continue"
            || started.prior_suspension_id.as_deref() != Some(request.suspension_id)
            || started.expected_session_version != Some(request.expected_session_version)
            || facts[index].turn_id.as_ref() != Some(turn_id)
        {
            return Err(LiveHostError::CommandConflict);
        }
        replay_started_batch(
            session_id,
            &facts,
            index,
            ReplayInput::Continue(request.input),
            Some(request.suspension_id),
        )
    }

    fn replay_cancel(
        &self,
        ledger: &SqliteLedger,
        session_id: &SessionId,
        turn_id: &TurnId,
        command_id: &str,
        requested_through_position: u64,
        through_position: u64,
    ) -> Result<Option<u64>, LiveHostError> {
        let facts = ledger
            .read_facts(session_id, 0, through_position, None)
            .map_err(map_sqlite)?;
        for fact in &facts {
            if fact.kind.as_str() != "turn.cancel_requested" {
                continue;
            }
            let payload: Cancelled = decode_payload(fact)?;
            if payload.command_id == command_id {
                if fact.turn_id.as_ref() != Some(turn_id)
                    || payload.requested_through_position != requested_through_position
                {
                    return Err(LiveHostError::CommandConflict);
                }
                return Ok(Some(fact.position));
            }
        }
        reject_other_command(&facts, command_id)?;
        Ok(None)
    }
}

struct SessionBinding {
    agent_instance_id: AgentInstanceId,
    definition_id: AgentDefinitionId,
    definition_revision: AgentDefinitionRevision,
    snapshot_digest: String,
    session_version: u64,
    max_position: u64,
}

struct ContinueReplay<'a> {
    command_id: &'a str,
    suspension_id: &'a str,
    expected_session_version: u64,
    input: HostContinuationInput<'a>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionOpened {
    command_id: String,
    definition_id: String,
    definition_revision: String,
    snapshot_digest: String,
    agent_instance_id: String,
}

#[derive(Deserialize)]
struct StartedCommand {
    command_id: String,
    kind: String,
    definition_id: String,
    definition_revision: String,
    snapshot_digest: String,
    trusted_input_digest: String,
    prior_suspension_id: Option<String>,
    expected_session_version: Option<u64>,
}

#[derive(Deserialize)]
struct TurnInput {
    input_kind: String,
    content: InlineContent,
    suspension_id: Option<String>,
}

#[derive(Deserialize)]
struct InlineContent {
    digest: String,
    inline_utf8: String,
}

#[derive(Deserialize)]
struct Cancelled {
    command_id: String,
    requested_through_position: u64,
}

fn find_started(
    facts: &[DurableFact],
    command_id: &str,
) -> Result<Option<(usize, StartedCommand)>, LiveHostError> {
    for (index, fact) in facts.iter().enumerate() {
        if fact.kind.as_str() == "turn.started" {
            let payload: StartedCommand = decode_payload(fact)?;
            if payload.command_id == command_id {
                return Ok(Some((index, payload)));
            }
        }
    }
    Ok(None)
}

fn replay_started_batch(
    session_id: &SessionId,
    facts: &[DurableFact],
    started_index: usize,
    input: ReplayInput<'_>,
    suspension_id: Option<&str>,
) -> Result<Option<TurnCommandResponse>, LiveHostError> {
    let started = facts
        .get(started_index)
        .ok_or(LiveHostError::CorruptState)?;
    let (input_index, execution_index) = if suspension_id.is_some() {
        (
            started_index
                .checked_sub(1)
                .ok_or(LiveHostError::CorruptState)?,
            started_index + 1,
        )
    } else {
        (started_index + 1, started_index + 2)
    };
    let input_fact = facts.get(input_index).ok_or(LiveHostError::CorruptState)?;
    let execution = facts
        .get(execution_index)
        .ok_or(LiveHostError::CorruptState)?;
    if input_fact.kind.as_str() != "turn.input"
        || execution.kind.as_str() != "execution.started"
        || input_fact.turn_id != started.turn_id
        || execution.turn_id != started.turn_id
    {
        return Err(LiveHostError::CorruptState);
    }
    let payload: TurnInput = decode_payload(input_fact)?;
    let expected = replay_input(&payload.input_kind, input)?;
    if payload.content.inline_utf8 != expected
        || payload.content.digest != digest(expected.as_bytes())
        || payload.suspension_id.as_deref() != suspension_id
    {
        return Err(LiveHostError::CommandConflict);
    }
    let turn_id = started
        .turn_id
        .as_ref()
        .ok_or(LiveHostError::CorruptState)?;
    let execution_id = execution
        .execution_id
        .as_ref()
        .ok_or(LiveHostError::CorruptState)?;
    Ok(Some(turn_response(
        session_id,
        turn_id,
        Some(execution_id),
        execution.position,
    )))
}

#[derive(Clone, Copy)]
enum ReplayInput<'a> {
    Start(&'a str),
    Continue(HostContinuationInput<'a>),
}

fn replay_input(kind: &str, input: ReplayInput<'_>) -> Result<String, LiveHostError> {
    match (kind, input) {
        ("trusted_user", ReplayInput::Start(value))
        | ("external_input", ReplayInput::Continue(HostContinuationInput::String(value))) => {
            Ok(value.to_owned())
        }
        ("interaction_string", ReplayInput::Continue(HostContinuationInput::String(value))) => {
            serde_jcs::to_string(&serde_json::Value::String(value.to_owned()))
                .map_err(|_| LiveHostError::InvalidRequest)
        }
        ("interaction_json", ReplayInput::Continue(HostContinuationInput::Json(value))) => {
            canonical_json(value)
        }
        _ => Err(LiveHostError::CommandConflict),
    }
}

fn decode_payload<T: for<'de> Deserialize<'de>>(fact: &DurableFact) -> Result<T, LiveHostError> {
    serde_json::from_str(fact.payload.as_json()).map_err(|_| LiveHostError::CorruptState)
}

fn reject_other_command(facts: &[DurableFact], command_id: &str) -> Result<(), LiveHostError> {
    for fact in facts {
        if !matches!(
            fact.kind.as_str(),
            "session.opened" | "turn.started" | "turn.cancel_requested"
        ) {
            continue;
        }
        let payload: serde_json::Value = decode_payload(fact)?;
        if payload
            .get("command_id")
            .and_then(serde_json::Value::as_str)
            == Some(command_id)
        {
            return Err(LiveHostError::CommandConflict);
        }
    }
    Ok(())
}

fn validate_installed(
    installed: &InstalledAgent,
    limits: LiveHostLimits,
) -> Result<(), LiveHostError> {
    if AgentDefinitionId::try_from(installed.definition_id.as_str()).is_err()
        || AgentDefinitionRevision::try_from(installed.definition_revision.as_str()).is_err()
        || installed.agent_instance_namespace.is_empty()
        || installed.agent_instance_namespace.len() > 128
        || !installed
            .agent_instance_namespace
            .bytes()
            .all(|byte| (b'!'..=b'~').contains(&byte))
        || installed.snapshot_digest.len() != 64
        || !installed
            .snapshot_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || installed.runtime_limits.max_iterations == 0
        || installed.runtime_limits.max_input_tokens == Some(0)
        || installed.runtime_limits.max_output_tokens == Some(0)
        || installed.runtime_limits.deadline_budget_ms == Some(0)
        || limits.max_command_bytes == 0
        || limits.event_batch_size == 0
        || limits.event_poll_interval_ms == 0
        || installed.public_activity_catalogue.is_some() != limits.activity.is_some()
        || installed.public_capabilities.iter().any(String::is_empty)
        || installed
            .public_capabilities
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(LiveHostError::InvalidRequest);
    }
    if let (Some(catalogue), Some(activity)) =
        (&installed.public_activity_catalogue, limits.activity)
    {
        let keys = catalogue
            .descriptors
            .iter()
            .map(|item| (item.tool_name.as_str(), item.tool_revision.as_str()))
            .collect::<Vec<_>>();
        let valid_label = |value: &str| {
            !value.is_empty()
                && value.len() <= activity.max_label_bytes
                && !value.starts_with('.')
                && !value.ends_with('.')
                && !value.contains("..")
                && value.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_')
                })
        };
        if catalogue.schema_version != 1
            || catalogue.catalogue_revision.is_empty()
            || keys.windows(2).any(|pair| pair[0] >= pair[1])
            || catalogue.descriptors.iter().any(|item| {
                item.tool_name.is_empty()
                    || item.tool_revision.is_empty()
                    || !valid_label(&item.label_key)
            })
            || activity.max_activities_per_turn == 0
            || activity.max_activity_facts == 0
            || activity.max_label_bytes == 0
            || activity.max_activity_id_bytes == 0
            || activity.max_encoded_bytes_per_turn == 0
        {
            return Err(LiveHostError::InvalidRequest);
        }
    }
    Ok(())
}

fn ensure_response_bound<T: serde::Serialize>(
    value: &T,
    max_bytes: usize,
) -> Result<(), LiveHostError> {
    if serde_json::to_vec(value)
        .map_err(|_| LiveHostError::CorruptState)?
        .len()
        > max_bytes
    {
        Err(LiveHostError::ReadBoundExceeded)
    } else {
        Ok(())
    }
}

pub(crate) fn validate_key(value: &str) -> Result<(), LiveHostError> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| (b'!'..=b'~').contains(&byte))
    {
        Err(LiveHostError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn validate_text(value: &str, maximum: usize) -> Result<(), LiveHostError> {
    if value.is_empty() || value.len() > maximum {
        Err(LiveHostError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn validate_continuation_bytes(
    input: HostContinuationInput<'_>,
    maximum: usize,
) -> Result<(), LiveHostError> {
    let value = match input {
        HostContinuationInput::String(value) | HostContinuationInput::Json(value) => value,
    };
    if value.len() > maximum {
        Err(LiveHostError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn continuation_input(
    state: &crate::SuspendedTurnState,
    input: HostContinuationInput<'_>,
) -> Result<ContinuationInput, LiveHostError> {
    if state.interaction.is_some() {
        let (canonical_json, representation) = match input {
            HostContinuationInput::String(value) => (
                serde_jcs::to_string(&serde_json::Value::String(value.to_owned()))
                    .map_err(|_| LiveHostError::InvalidRequest)?,
                InteractionInputRepresentation::StringField,
            ),
            HostContinuationInput::Json(value) => (
                canonical_json(value)?,
                InteractionInputRepresentation::JsonField,
            ),
        };
        Ok(ContinuationInput::InteractionResponse {
            canonical_json,
            representation,
        })
    } else if matches!(
        state.suspension_kind,
        RuntimeSuspensionKind::ExternalInputRequired | RuntimeSuspensionKind::PartialOutput
    ) {
        match input {
            HostContinuationInput::String(value) => {
                Ok(ContinuationInput::ExternalInput(value.to_owned()))
            }
            HostContinuationInput::Json(_) => Err(LiveHostError::InvalidRequest),
        }
    } else {
        Err(LiveHostError::PreconditionFailed)
    }
}

fn canonical_json(value: &str) -> Result<String, LiveHostError> {
    let parsed: serde_json::Value =
        serde_json::from_str(value).map_err(|_| LiveHostError::InvalidRequest)?;
    let canonical = serde_jcs::to_string(&parsed).map_err(|_| LiveHostError::InvalidRequest)?;
    if canonical == value {
        Ok(canonical)
    } else {
        Err(LiveHostError::InvalidRequest)
    }
}

fn identity<T>(value: &str) -> Result<T, LiveHostError>
where
    T: for<'a> TryFrom<&'a str>,
{
    T::try_from(value).map_err(|_| LiveHostError::InvalidRequest)
}

fn only_position(values: &[u64]) -> Result<u64, LiveHostError> {
    if let [position] = values {
        Ok(*position)
    } else {
        Err(LiveHostError::CorruptState)
    }
}

fn last_position(values: &[u64]) -> Result<u64, LiveHostError> {
    values.last().copied().ok_or(LiveHostError::CorruptState)
}

fn turn_response(
    session: &SessionId,
    turn: &TurnId,
    execution: Option<&ExecutionId>,
    committed_position: u64,
) -> TurnCommandResponse {
    TurnCommandResponse {
        session_id: session.as_str().to_owned(),
        turn_id: turn.as_str().to_owned(),
        execution_id: execution.map_or_else(String::new, |value| value.as_str().to_owned()),
        committed_position,
    }
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
}

fn map_runtime(error: RuntimeCommandError) -> LiveHostError {
    match error {
        RuntimeCommandError::InvalidCommand => LiveHostError::InvalidRequest,
        RuntimeCommandError::CommandConflict => LiveHostError::CommandConflict,
        RuntimeCommandError::ConcurrentModification => LiveHostError::ConcurrentModification,
        RuntimeCommandError::ContinuationMismatch | RuntimeCommandError::TurnNotResumable => {
            LiveHostError::PreconditionFailed
        }
        RuntimeCommandError::DurabilityFailure => LiveHostError::DurabilityUnavailable,
        RuntimeCommandError::CorruptLedger | RuntimeCommandError::InvariantViolation => {
            LiveHostError::CorruptState
        }
    }
}

fn map_runtime_query(error: RuntimeCommandError) -> LiveHostError {
    match error {
        RuntimeCommandError::InvalidCommand => LiveHostError::NotFound,
        other => map_runtime(other),
    }
}

fn map_sqlite_query(error: SqliteLedgerError) -> LiveHostError {
    match error {
        SqliteLedgerError::Domain(LedgerError::MissingReference) => LiveHostError::NotFound,
        other => map_sqlite(other),
    }
}

fn map_sqlite(error: SqliteLedgerError) -> LiveHostError {
    match error {
        SqliteLedgerError::Domain(LedgerError::IdempotencyCollision)
        | SqliteLedgerError::Domain(LedgerError::IncompleteReplay) => {
            LiveHostError::CommandConflict
        }
        SqliteLedgerError::Domain(LedgerError::ConcurrentModification) => {
            LiveHostError::ConcurrentModification
        }
        SqliteLedgerError::Storage(_) => LiveHostError::DurabilityUnavailable,
        SqliteLedgerError::Domain(LedgerError::MissingReference) => LiveHostError::NotFound,
        SqliteLedgerError::CorruptLedger(_)
        | SqliteLedgerError::UnsupportedSchema(_)
        | SqliteLedgerError::InvalidStoredValue(_)
        | SqliteLedgerError::Domain(_)
        | SqliteLedgerError::Lease(_)
        | SqliteLedgerError::ScheduleLease(_) => LiveHostError::CorruptState,
    }
}
