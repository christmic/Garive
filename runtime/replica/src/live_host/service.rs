use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    path::Path,
    sync::Arc,
};

use garive_goal::{GoalDefinitionV1, GoalState};
use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload,
    CommitDisposition, CommitResult, DurableFact, ExecutionId, FactDraft, FactId, FactKind,
    LedgerError, SessionId, TurnId,
};
use garive_plan::{PlanState, StepState};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    commit_goal_command, commit_planned_turn, get_turn,
    goal_recovery::reconstruct_goal_graph_from_facts, plan_cancel_turn, plan_continue_turn,
    plan_create_goal, plan_goal_transition, plan_start_turn, reconstruct_goal,
    reconstruct_plan_graph, reconstruct_suspended_turn, CancelReason, CancelTurnCommand,
    ContinuationInput, ContinueTurnCommand, GetTurnQuery, GoalCommandContext, GoalRuntimeError,
    GoalRuntimeTransition, InteractionInputRepresentation, LiveOutputHub, LiveOutputSubscriber,
    RuntimeCommandError, RuntimeCommandId, RuntimeSuspensionKind, RuntimeTurnStatus, SqliteLedger,
    SqliteLedgerError, StartTurnCommand,
};

use super::{
    completion_text, internal_turn::InternalPlannerTurns, project_activities, project_fact,
    AgentDefinitionPageV1, AgentDefinitionSummary, AgentDefinitionSummaryV1, CommittedTurn,
    CreateSessionResponse, GoalCommandAuthority, GoalCommandAuthorityError, GoalCommandResponseV1,
    GoalPageV1, GoalSummaryV1, HostArtifact, HostArtifactPage, HostClock, HostContinuationInput,
    HostEventPage, HostReadLimits, HostWorkspaceAttachment, HostWorkspaceContextEntry,
    HostWorkspaceDetachment, InstalledAgent, LiveHostError, LiveHostEvent, LiveHostLimits,
    LiveHostState, PlanPageV1, PlanSummaryV1, SessionPageV1, SessionSummary, SessionViewV1,
    TurnCommandResponse, TurnDispatcher, TurnSuspensionView, TurnTimelineItem, TurnTimelinePage,
    TurnTimelinePageV1,
};
use super::{read_cursor, read_model, timeline_projection};

const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;

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
        Self::new_catalogue_with_read_limits(
            database_path,
            [installed],
            limits,
            HostReadLimits::PRODUCT_DEFAULT,
            clock,
            dispatcher,
        )
    }

    /// Constructs a Host with an explicitly shared H4 live-output hub.
    pub fn new_with_live_output(
        database_path: impl AsRef<Path>,
        installed: InstalledAgent,
        limits: LiveHostLimits,
        clock: Arc<dyn HostClock>,
        dispatcher: Arc<dyn TurnDispatcher>,
        live_output: LiveOutputHub,
    ) -> Result<Self, LiveHostError> {
        Self::construct(
            database_path,
            [installed],
            limits,
            HostReadLimits::PRODUCT_DEFAULT,
            clock,
            dispatcher,
            Some(live_output),
        )
    }

    /// Constructs a multi-Agent Host with one explicitly shared H4 output hub.
    pub fn new_catalogue_with_live_output(
        database_path: impl AsRef<Path>,
        installed: impl IntoIterator<Item = InstalledAgent>,
        limits: LiveHostLimits,
        clock: Arc<dyn HostClock>,
        dispatcher: Arc<dyn TurnDispatcher>,
        live_output: LiveOutputHub,
    ) -> Result<Self, LiveHostError> {
        Self::construct(
            database_path,
            installed,
            limits,
            HostReadLimits::PRODUCT_DEFAULT,
            clock,
            dispatcher,
            Some(live_output),
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
        Self::new_catalogue_with_read_limits(
            database_path,
            [installed],
            limits,
            read_limits,
            clock,
            dispatcher,
        )
    }

    /// Constructs a Host from a non-empty identity-unique Agent catalogue.
    pub fn new_catalogue(
        database_path: impl AsRef<Path>,
        installed: impl IntoIterator<Item = InstalledAgent>,
        limits: LiveHostLimits,
        clock: Arc<dyn HostClock>,
        dispatcher: Arc<dyn TurnDispatcher>,
    ) -> Result<Self, LiveHostError> {
        Self::new_catalogue_with_read_limits(
            database_path,
            installed,
            limits,
            HostReadLimits::PRODUCT_DEFAULT,
            clock,
            dispatcher,
        )
    }

    /// Constructs a multi-Agent Host with explicit independent read bounds.
    pub fn new_catalogue_with_read_limits(
        database_path: impl AsRef<Path>,
        installed: impl IntoIterator<Item = InstalledAgent>,
        limits: LiveHostLimits,
        read_limits: HostReadLimits,
        clock: Arc<dyn HostClock>,
        dispatcher: Arc<dyn TurnDispatcher>,
    ) -> Result<Self, LiveHostError> {
        Self::construct(
            database_path,
            installed,
            limits,
            read_limits,
            clock,
            dispatcher,
            None,
        )
    }

    /// Constructs a Headless-compatible Host bound to one explicit local dispatch queue.
    ///
    /// This is the canonical entry point for binary callers that wire
    /// [`crate::LocalExecutionWorker`] themselves (i.e. outside `DesktopHost`).
    /// The returned `dispatcher` is what callers wire into their
    /// [`crate::LocalExecutionWorker`]; the returned `queue` is what
    /// [`crate::drive_pending`] drains.
    pub fn new_with_worker(
        database_path: impl AsRef<Path>,
        installed: impl IntoIterator<Item = InstalledAgent>,
        limits: LiveHostLimits,
        clock: Arc<dyn HostClock>,
        queue_capacity: usize,
    ) -> Result<
        (
            Self,
            Arc<crate::LocalTurnDispatcher>,
            crate::LocalDispatchQueue,
        ),
        crate::LocalWorkerError,
    > {
        let (dispatcher, queue) = crate::local_dispatch_queue(queue_capacity)?;
        let host = Self::new_catalogue_with_read_limits(
            database_path,
            installed,
            limits,
            HostReadLimits::PRODUCT_DEFAULT,
            clock,
            dispatcher.clone(),
        )
        .map_err(|_| crate::LocalWorkerError::InvalidComposition)?;
        Ok((host, dispatcher, queue))
    }

    fn construct(
        database_path: impl AsRef<Path>,
        installed: impl IntoIterator<Item = InstalledAgent>,
        limits: LiveHostLimits,
        read_limits: HostReadLimits,
        clock: Arc<dyn HostClock>,
        dispatcher: Arc<dyn TurnDispatcher>,
        live_output: Option<LiveOutputHub>,
    ) -> Result<Self, LiveHostError> {
        if !read_limits.valid() {
            return Err(LiveHostError::InvalidRequest);
        }
        let mut catalogue = BTreeMap::new();
        for installation in installed {
            validate_installed(&installation, limits)?;
            if catalogue
                .insert(installation.definition_id.clone(), installation)
                .is_some()
            {
                return Err(LiveHostError::InvalidRequest);
            }
        }
        if catalogue.is_empty() || catalogue.len() > read_limits.max_definitions {
            return Err(LiveHostError::InvalidRequest);
        }
        SqliteLedger::open(database_path.as_ref()).map_err(map_sqlite)?;
        Ok(Self {
            state: Arc::new(LiveHostState {
                database_path: database_path.as_ref().to_owned(),
                installed: catalogue,
                limits,
                read_limits,
                clock,
                dispatcher,
                live_output,
                goal_authority: None,
                management_validator: Arc::new(crate::management::AllowAllValidator),
            }),
        })
    }

    /// Returns an equivalent Host with an explicit product-owned Goal command authority.
    pub fn with_goal_authority(self, authority: Arc<dyn GoalCommandAuthority>) -> Self {
        Self {
            state: Arc::new(LiveHostState {
                database_path: self.state.database_path.clone(),
                installed: self.state.installed.clone(),
                limits: self.state.limits,
                read_limits: self.state.read_limits,
                clock: Arc::clone(&self.state.clock),
                dispatcher: Arc::clone(&self.state.dispatcher),
                live_output: self.state.live_output.clone(),
                goal_authority: Some(authority),
                management_validator: Arc::clone(&self.state.management_validator),
            }),
        }
    }

    /// Returns an equivalent Host whose management-port `POST /setup` calls
    /// run [`crate::management::ManagementValidator::validate`] before the
    /// DAO commit.
    ///
    /// Commit 3 default: the per-field DAO validation still runs, so a body
    /// with an unknown `profile_id` is rejected with
    /// `management_profile_unknown` once a Registry-backed validator is
    /// attached (wired in by the `garive-host` binary in commit 4).
    pub fn with_management_validator(
        self,
        validator: Arc<dyn crate::management::ManagementValidator>,
    ) -> Self {
        Self {
            state: Arc::new(LiveHostState {
                database_path: self.state.database_path.clone(),
                installed: self.state.installed.clone(),
                limits: self.state.limits,
                read_limits: self.state.read_limits,
                clock: Arc::clone(&self.state.clock),
                dispatcher: Arc::clone(&self.state.dispatcher),
                live_output: self.state.live_output.clone(),
                goal_authority: self.state.goal_authority.clone(),
                management_validator: validator,
            }),
        }
    }

    /// Borrows the active [`crate::management::ManagementValidator`].
    pub(crate) fn management_validator(&self) -> &dyn crate::management::ManagementValidator {
        self.state.management_validator.as_ref()
    }

    /// Returns the configured H4 hub for an explicitly shared worker composition.
    pub fn live_output_hub(&self) -> Option<LiveOutputHub> {
        self.state.live_output.clone()
    }

    /// Subscribes to bounded ephemeral output for one existing Session.
    pub fn subscribe_live_output(
        &self,
        session: &str,
    ) -> Result<LiveOutputSubscriber, LiveHostError> {
        let session_id = identity::<SessionId>(session)?;
        let exists = self
            .ledger()?
            .session_watermark(&session_id)
            .map_err(map_sqlite)?
            .is_some();
        if !exists {
            return Err(LiveHostError::NotFound);
        }
        self.state
            .live_output
            .as_ref()
            .ok_or(LiveHostError::NotFound)?
            .subscribe(session)
            .map_err(|_| LiveHostError::DurabilityUnavailable)
    }

    /// Lists installed Agent definitions without exposing Runtime configuration.
    pub fn list_agent_definitions(&self) -> Result<AgentDefinitionPageV1, LiveHostError> {
        let page = AgentDefinitionPageV1 {
            api_version: "v1",
            definitions: self
                .state
                .installed
                .values()
                .map(|installed| AgentDefinitionSummaryV1 {
                    api_version: "v1",
                    definition_id: installed.definition_id.clone(),
                    definition_revision: installed.definition_revision.clone(),
                    capabilities: installed.public_capabilities.clone(),
                })
                .collect(),
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
        if watermark.max_position > MAX_SAFE_JSON_INTEGER {
            return Err(LiveHostError::ReadBoundExceeded);
        }
        let facts = ledger
            .read_facts(&session_id, 0, watermark.max_position, None)
            .map_err(map_sqlite)?;
        let installed = self.installed_for_facts(&facts)?;
        let view = read_model::project_session(
            &session_id,
            watermark.max_position,
            &facts,
            installed,
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

    /// Creates or exactly replays one authority-admitted canonical Goal definition.
    pub fn create_goal(
        &self,
        idempotency_key: &str,
        session: &str,
        expected_session_version: u64,
        definition_json: &str,
    ) -> Result<GoalCommandResponseV1, LiveHostError> {
        validate_key(idempotency_key)?;
        validate_text(definition_json, self.state.limits.max_command_bytes)?;
        if expected_session_version == 0 {
            return Err(LiveHostError::InvalidRequest);
        }
        let session_id = identity::<SessionId>(session)?;
        let definition = GoalDefinitionV1::from_canonical_json(definition_json)
            .map_err(|_| LiveHostError::InvalidRequest)?;
        let ledger = self.ledger()?;
        let actor_reference =
            match replayed_goal_actor(&ledger, &session_id, idempotency_key, definition_json)? {
                Some(actor) => actor,
                None => self
                    .state
                    .goal_authority
                    .as_ref()
                    .ok_or(LiveHostError::PreconditionFailed)?
                    .authorize_create(session, &definition)
                    .map_err(map_goal_authority)?,
            };
        validate_text(&actor_reference, 512)?;
        let planned = plan_create_goal(
            &ledger,
            &session_id,
            &GoalCommandContext {
                command_id: idempotency_key.into(),
                actor_reference,
                recorded_at: self.recorded_at()?,
            },
            definition,
        )
        .map_err(map_goal_runtime)?;
        let mut ledger = self.ledger()?;
        let committed = commit_goal_command(
            &mut ledger,
            session_id.clone(),
            expected_session_version,
            &planned,
        )
        .map_err(map_goal_runtime)?;
        let position = only_position(&committed.positions)?;
        if committed.session_version > MAX_SAFE_JSON_INTEGER || position > MAX_SAFE_JSON_INTEGER {
            return Err(LiveHostError::CorruptState);
        }
        Ok(GoalCommandResponseV1 {
            api_version: "v1",
            session_id: session.into(),
            goal_id: planned.next.snapshot.definition().goal_id().as_str().into(),
            revision: planned.next.snapshot.revision(),
            state: goal_state(planned.next.snapshot.state()),
            session_version: committed.session_version,
            committed_position: position,
        })
    }

    /// Cancels or exactly replays one authority-admitted non-terminal Goal.
    pub fn cancel_goal(
        &self,
        idempotency_key: &str,
        session: &str,
        goal_id: &str,
        expected_session_version: u64,
        expected_revision: u64,
        reason: &str,
    ) -> Result<GoalCommandResponseV1, LiveHostError> {
        validate_key(idempotency_key)?;
        validate_text(reason, 512)?;
        if expected_session_version == 0 || expected_revision == 0 {
            return Err(LiveHostError::InvalidRequest);
        }
        let session_id = identity::<SessionId>(session)?;
        let ledger = self.ledger()?;
        if let Some(response) = replayed_cancel_response(
            &ledger,
            &session_id,
            idempotency_key,
            goal_id,
            expected_revision,
            reason,
        )? {
            return Ok(response);
        }
        let current = reconstruct_goal(&ledger, &session_id, goal_id).map_err(map_goal_runtime)?;
        if current.session_version != expected_session_version {
            return Err(LiveHostError::ConcurrentModification);
        }
        let transition = GoalRuntimeTransition::Cancel {
            reason: reason.into(),
        };
        let actor_reference = self
            .state
            .goal_authority
            .as_ref()
            .ok_or(LiveHostError::PreconditionFailed)?
            .authorize_transition(session, &current, &transition)
            .map_err(map_goal_authority)?;
        validate_text(&actor_reference, 512)?;
        let planned = plan_goal_transition(
            &ledger,
            &session_id,
            goal_id,
            expected_revision,
            &GoalCommandContext {
                command_id: idempotency_key.into(),
                actor_reference,
                recorded_at: self.recorded_at()?,
            },
            transition,
        )
        .map_err(map_goal_runtime)?;
        let mut ledger = self.ledger()?;
        let committed =
            commit_goal_command(&mut ledger, session_id, expected_session_version, &planned)
                .map_err(map_goal_runtime)?;
        goal_command_response(session, &planned.next, &committed)
    }

    /// Revises or exactly replays one authority-admitted non-terminal Goal definition.
    #[allow(clippy::too_many_arguments)]
    pub fn revise_goal(
        &self,
        idempotency_key: &str,
        session: &str,
        goal_id: &str,
        expected_session_version: u64,
        expected_revision: u64,
        definition_json: &str,
        replacement_reason: &str,
    ) -> Result<GoalCommandResponseV1, LiveHostError> {
        validate_key(idempotency_key)?;
        validate_text(definition_json, self.state.limits.max_command_bytes)?;
        validate_text(replacement_reason, 512)?;
        if expected_session_version == 0 || expected_revision == 0 {
            return Err(LiveHostError::InvalidRequest);
        }
        let definition = GoalDefinitionV1::from_canonical_json(definition_json)
            .map_err(|_| LiveHostError::InvalidRequest)?;
        if definition.goal_id().as_str() != goal_id {
            return Err(LiveHostError::InvalidRequest);
        }
        let session_id = identity::<SessionId>(session)?;
        let ledger = self.ledger()?;
        if let Some(response) = replayed_revise_response(
            &ledger,
            &session_id,
            idempotency_key,
            goal_id,
            expected_revision,
            definition_json,
            replacement_reason,
        )? {
            return Ok(response);
        }
        let current = reconstruct_goal(&ledger, &session_id, goal_id).map_err(map_goal_runtime)?;
        if current.session_version != expected_session_version {
            return Err(LiveHostError::ConcurrentModification);
        }
        let transition = GoalRuntimeTransition::Revise {
            definition: Box::new(definition),
            replacement_reason: replacement_reason.into(),
        };
        let actor_reference = self
            .state
            .goal_authority
            .as_ref()
            .ok_or(LiveHostError::PreconditionFailed)?
            .authorize_transition(session, &current, &transition)
            .map_err(map_goal_authority)?;
        validate_text(&actor_reference, 512)?;
        let planned = plan_goal_transition(
            &ledger,
            &session_id,
            goal_id,
            expected_revision,
            &GoalCommandContext {
                command_id: idempotency_key.into(),
                actor_reference,
                recorded_at: self.recorded_at()?,
            },
            transition,
        )
        .map_err(map_goal_runtime)?;
        let mut ledger = self.ledger()?;
        let committed =
            commit_goal_command(&mut ledger, session_id, expected_session_version, &planned)
                .map_err(map_goal_runtime)?;
        goal_command_response(session, &planned.next, &committed)
    }

    /// Reads all current Goals from one verified fixed Session prefix.
    pub fn get_goals(&self, session: &str) -> Result<GoalPageV1, LiveHostError> {
        let session_id = identity::<SessionId>(session)?;
        let ledger = self.ledger()?;
        let watermark = ledger
            .session_watermark(&session_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::NotFound)?;
        if watermark.max_position > MAX_SAFE_JSON_INTEGER
            || watermark.session_version > MAX_SAFE_JSON_INTEGER
        {
            return Err(LiveHostError::ReadBoundExceeded);
        }
        let facts = ledger
            .read_facts(&session_id, 0, watermark.max_position, None)
            .map_err(map_sqlite)?;
        if facts.len() > self.state.read_limits.max_facts {
            return Err(LiveHostError::ReadBoundExceeded);
        }
        let installed = self.installed_for_facts(&facts)?;
        read_model::project_session(
            &session_id,
            watermark.max_position,
            &facts,
            installed,
            self.state.read_limits,
        )?;
        let graph = reconstruct_goal_graph_from_facts(
            &facts,
            watermark.session_version,
            watermark.max_position,
        )
        .map_err(|_| LiveHostError::CorruptState)?;
        if graph.len() > self.state.read_limits.max_goals {
            return Err(LiveHostError::ReadBoundExceeded);
        }
        let goals = graph
            .into_values()
            .map(|state| {
                let definition = state.snapshot.definition();
                let (objective, objective_truncated) = bounded_text(
                    definition.objective(),
                    self.state.read_limits.max_goal_objective_bytes,
                );
                let criteria_total = u32::try_from(definition.criteria().len())
                    .map_err(|_| LiveHostError::ReadBoundExceeded)?;
                let criteria_satisfied = u32::try_from(state.snapshot.terminal_evidence().len())
                    .map_err(|_| LiveHostError::ReadBoundExceeded)?;
                Ok(GoalSummaryV1 {
                    api_version: "v1",
                    goal_id: definition.goal_id().as_str().into(),
                    revision: state.snapshot.revision(),
                    state: goal_state(state.snapshot.state()),
                    definition_digest: definition
                        .digest()
                        .map_err(|_| LiveHostError::CorruptState)?,
                    objective,
                    objective_truncated,
                    parent_goal_id: definition
                        .parent_goal_id()
                        .map(|value| value.as_str().into()),
                    attempt_number: state.attempt_number,
                    criteria_total,
                    criteria_satisfied,
                })
            })
            .collect::<Result<Vec<_>, LiveHostError>>()?;
        let page = GoalPageV1 {
            api_version: "v1",
            session_id: session.into(),
            goals,
            session_version: watermark.session_version,
            observed_max_position: watermark.max_position,
        };
        ensure_response_bound(&page, self.state.read_limits.max_response_bytes)?;
        Ok(page)
    }

    /// Reads all current Plan revisions from one verified fixed Session prefix.
    pub fn get_plans(&self, session: &str) -> Result<PlanPageV1, LiveHostError> {
        let session_id = identity::<SessionId>(session)?;
        let ledger = self.ledger()?;
        let watermark = ledger
            .session_watermark(&session_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::NotFound)?;
        if watermark.max_position > MAX_SAFE_JSON_INTEGER
            || watermark.session_version > MAX_SAFE_JSON_INTEGER
        {
            return Err(LiveHostError::ReadBoundExceeded);
        }
        let facts = ledger
            .read_facts(&session_id, 0, watermark.max_position, None)
            .map_err(map_sqlite)?;
        if facts.len() > self.state.read_limits.max_facts {
            return Err(LiveHostError::ReadBoundExceeded);
        }
        let installed = self.installed_for_facts(&facts)?;
        read_model::project_session(
            &session_id,
            watermark.max_position,
            &facts,
            installed,
            self.state.read_limits,
        )?;
        let graph = reconstruct_plan_graph(&ledger, &session_id)
            .map_err(|_| LiveHostError::CorruptState)?;
        if graph.len() > self.state.read_limits.max_plans {
            return Err(LiveHostError::ReadBoundExceeded);
        }
        let plans = graph
            .into_iter()
            .map(|((plan_id, revision), state)| {
                let definition = state.snapshot.definition();
                let mut ready = 0u32;
                let mut active = 0u32;
                let mut completed = 0u32;
                let mut failed = 0u32;
                for step in definition.steps() {
                    match state
                        .snapshot
                        .step(step.step_id())
                        .ok_or(LiveHostError::CorruptState)?
                        .state()
                    {
                        StepState::Ready => ready += 1,
                        StepState::Claimed | StepState::Running | StepState::Suspended => {
                            active += 1
                        }
                        StepState::Completed => completed += 1,
                        StepState::Failed => failed += 1,
                        StepState::Pending => {}
                    }
                }
                Ok(PlanSummaryV1 {
                    api_version: "v1",
                    plan_id,
                    revision,
                    state: plan_state(state.snapshot.state()),
                    definition_digest: definition
                        .digest()
                        .map_err(|_| LiveHostError::CorruptState)?,
                    goal_id: definition.goal_id().into(),
                    goal_revision: definition.goal_revision(),
                    state_version: state.state_version,
                    steps_total: u32::try_from(definition.steps().len())
                        .map_err(|_| LiveHostError::ReadBoundExceeded)?,
                    steps_ready: ready,
                    steps_active: active,
                    steps_completed: completed,
                    steps_failed: failed,
                    total_attempts: state.snapshot.total_attempts(),
                })
            })
            .collect::<Result<Vec<_>, LiveHostError>>()?;
        let page = PlanPageV1 {
            api_version: "v1",
            session_id: session.into(),
            plans,
            session_version: watermark.session_version,
            observed_max_position: watermark.max_position,
        };
        ensure_response_bound(&page, self.state.read_limits.max_response_bytes)?;
        Ok(page)
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

    /// Projects content-free durable transition coordinates for the private Gateway monitor.
    pub(crate) fn mobile_wake_page(
        &self,
        limit: usize,
        before: Option<&str>,
    ) -> Result<super::MobileWakePage, LiveHostError> {
        let page = self.list_sessions(limit, before)?;
        let observations = page
            .sessions
            .iter()
            .map(|session| super::MobileWakeObservation {
                session_id: session.session_id.clone(),
                latest_position: session.latest_position,
                wake_category: match session.latest_turn_state.as_deref() {
                    Some("suspended") => Some("attention"),
                    Some("completed") => Some("completed"),
                    Some("failed") => Some("failed"),
                    _ => None,
                },
            })
            .collect();
        Ok(super::MobileWakePage {
            api_version: "v1",
            observations,
            next_before: page.next_before,
        })
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
        if after_position > MAX_SAFE_JSON_INTEGER || after_position > watermark.max_position {
            return Err(LiveHostError::InvalidRequest);
        }
        if watermark.max_position > MAX_SAFE_JSON_INTEGER
            || watermark.session_version > MAX_SAFE_JSON_INTEGER
        {
            return Err(LiveHostError::ReadBoundExceeded);
        }
        let facts = ledger
            .read_facts(&session_id, 0, watermark.max_position, None)
            .map_err(map_sqlite)?;
        let installed = self.installed_for_facts(&facts)?;
        read_model::project_session(
            &session_id,
            watermark.max_position,
            &facts,
            installed,
            self.state.read_limits,
        )?;
        let activities = match (
            installed.public_activity_catalogue.as_ref(),
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

    /// Returns bounded installed-Agent discovery without granting new authority.
    pub fn agent_definitions(&self) -> Vec<AgentDefinitionSummary> {
        self.state
            .installed
            .values()
            .map(|installed| AgentDefinitionSummary {
                api_version: "v1",
                definition_id: installed.definition_id.clone(),
                definition_revision: installed.definition_revision.clone(),
                capabilities: Vec::new(),
            })
            .collect()
    }

    /// Returns restart-safe Sessions ordered by open time and identity descending.
    pub fn recent_sessions(&self, limit: usize) -> Result<Vec<SessionSummary>, LiveHostError> {
        if limit == 0 || limit > self.state.limits.event_batch_size as usize {
            return Err(LiveHostError::InvalidRequest);
        }
        let ledger = self.ledger()?;
        let mut sessions = ledger
            .list_sessions()
            .map_err(map_sqlite)?
            .into_iter()
            .map(|session_id| self.project_session(&ledger, &session_id))
            .collect::<Result<Vec<_>, _>>()?;
        sessions.sort_by(|left, right| {
            right
                .opened_at
                .cmp(&left.opened_at)
                .then_with(|| right.session_id.cmp(&left.session_id))
        });
        sessions.truncate(limit);
        Ok(sessions)
    }

    /// Durably attaches one path-free Workspace grant to a Session.
    pub fn attach_workspace(
        &self,
        idempotency_key: &str,
        session: &str,
        workspace_id: &str,
        display_name: &str,
        grant_revision: u64,
        access: &str,
    ) -> Result<HostWorkspaceAttachment, LiveHostError> {
        validate_key(idempotency_key)?;
        validate_key(workspace_id)?;
        validate_text(display_name, 128)?;
        if grant_revision == 0 || !matches!(access, "enumerate" | "read_write") {
            return Err(LiveHostError::InvalidRequest);
        }
        let session_id = identity::<SessionId>(session)?;
        let mut ledger = self.ledger()?;
        let watermark = ledger
            .session_watermark(&session_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::NotFound)?;
        let facts = ledger
            .read_facts(&session_id, 0, watermark.max_position, None)
            .map_err(map_sqlite)?;
        for fact in facts
            .iter()
            .filter(|fact| fact.kind.as_str() == "workspace.attached")
        {
            let payload: WorkspaceAttached = decode_payload(fact)?;
            if payload.command_id == idempotency_key {
                if payload.workspace_id != workspace_id
                    || payload.display_name != display_name
                    || payload.grant_revision != grant_revision
                    || payload.access != access
                {
                    return Err(LiveHostError::CommandConflict);
                }
                return Ok(workspace_attachment(&session_id, &payload, fact.position));
            }
        }
        reject_other_command(&facts, idempotency_key)?;
        let payload = serde_json::json!({
            "command_id":idempotency_key,
            "workspace_id":workspace_id,
            "display_name":display_name,
            "grant_revision":grant_revision,
            "access":access,
        });
        let fact = FactDraft {
            fact_id: FactId::try_from(
                format!("workspace-{}", digest(idempotency_key.as_bytes())).as_str(),
            )
            .map_err(|_| LiveHostError::InvalidRequest)?,
            turn_id: None,
            execution_id: None,
            model_request_id: None,
            tool_invocation_id: None,
            kind: FactKind::new("workspace.attached").map_err(|_| LiveHostError::InvalidRequest)?,
            schema_version: 1,
            payload: CanonicalPayload::from_value(&payload)
                .map_err(|_| LiveHostError::InvalidRequest)?,
            recorded_at: self.recorded_at()?,
        };
        let committed = ledger
            .commit(session_id.clone(), watermark.session_version, vec![fact])
            .map_err(map_sqlite)?;
        let position = *committed
            .positions
            .last()
            .ok_or(LiveHostError::DurabilityUnavailable)?;
        let payload: WorkspaceAttached =
            serde_json::from_value(payload).map_err(|_| LiveHostError::CorruptState)?;
        Ok(workspace_attachment(&session_id, &payload, position))
    }

    /// Returns the latest durable attachment for each opaque Workspace ID.
    pub fn session_workspaces(
        &self,
        session: &str,
    ) -> Result<Vec<HostWorkspaceAttachment>, LiveHostError> {
        let session_id = identity::<SessionId>(session)?;
        let ledger = self.ledger()?;
        let watermark = ledger
            .session_watermark(&session_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::NotFound)?;
        let facts = ledger
            .read_facts(&session_id, 0, watermark.max_position, None)
            .map_err(map_sqlite)?;
        let mut attached = BTreeMap::new();
        for fact in &facts {
            match fact.kind.as_str() {
                "workspace.attached" => {
                    let payload: WorkspaceAttached = decode_payload(fact)?;
                    attached.insert(
                        payload.workspace_id.clone(),
                        workspace_attachment(&session_id, &payload, fact.position),
                    );
                }
                "workspace.detached" => {
                    let payload: WorkspaceDetached = decode_payload(fact)?;
                    attached.remove(&payload.workspace_id);
                }
                _ => {}
            }
        }
        Ok(attached.into_values().collect())
    }

    /// Durably detaches one opaque Workspace grant from a Session.
    pub fn detach_workspace(
        &self,
        idempotency_key: &str,
        session: &str,
        workspace_id: &str,
        grant_revision: u64,
    ) -> Result<HostWorkspaceDetachment, LiveHostError> {
        validate_key(idempotency_key)?;
        validate_key(workspace_id)?;
        if grant_revision == 0 {
            return Err(LiveHostError::InvalidRequest);
        }
        let session_id = identity::<SessionId>(session)?;
        let mut ledger = self.ledger()?;
        let watermark = ledger
            .session_watermark(&session_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::NotFound)?;
        let facts = ledger
            .read_facts(&session_id, 0, watermark.max_position, None)
            .map_err(map_sqlite)?;
        for fact in facts
            .iter()
            .filter(|fact| fact.kind.as_str() == "workspace.detached")
        {
            let payload: WorkspaceDetached = decode_payload(fact)?;
            if payload.command_id == idempotency_key {
                if payload.workspace_id != workspace_id || payload.grant_revision != grant_revision
                {
                    return Err(LiveHostError::CommandConflict);
                }
                return Ok(workspace_detachment(&session_id, &payload, fact.position));
            }
        }
        reject_other_command(&facts, idempotency_key)?;
        let active = self.session_workspaces(session)?;
        let outcome = match active.iter().find(|item| item.workspace_id == workspace_id) {
            Some(item) if item.grant_revision == grant_revision => "detached",
            Some(_) => return Err(LiveHostError::CommandConflict),
            None => "already_detached",
        };
        let payload = serde_json::json!({
            "command_id":idempotency_key,
            "workspace_id":workspace_id,
            "grant_revision":grant_revision,
            "outcome":outcome,
        });
        let fact = FactDraft {
            fact_id: FactId::try_from(
                format!("workspace-detach-{}", digest(idempotency_key.as_bytes())).as_str(),
            )
            .map_err(|_| LiveHostError::InvalidRequest)?,
            turn_id: None,
            execution_id: None,
            model_request_id: None,
            tool_invocation_id: None,
            kind: FactKind::new("workspace.detached").map_err(|_| LiveHostError::InvalidRequest)?,
            schema_version: 1,
            payload: CanonicalPayload::from_value(&payload)
                .map_err(|_| LiveHostError::InvalidRequest)?,
            recorded_at: self.recorded_at()?,
        };
        let committed = ledger
            .commit(session_id.clone(), watermark.session_version, vec![fact])
            .map_err(map_sqlite)?;
        let position = *committed
            .positions
            .last()
            .ok_or(LiveHostError::DurabilityUnavailable)?;
        let payload: WorkspaceDetached =
            serde_json::from_value(payload).map_err(|_| LiveHostError::CorruptState)?;
        Ok(workspace_detachment(&session_id, &payload, position))
    }

    /// Projects one bounded fixed-prefix page of immutable Artifact revisions.
    pub fn list_artifacts(
        &self,
        session: &str,
        after_position: u64,
        limit: usize,
    ) -> Result<HostArtifactPage, LiveHostError> {
        if limit == 0 || limit > self.state.limits.event_batch_size as usize {
            return Err(LiveHostError::InvalidRequest);
        }
        let session_id = identity::<SessionId>(session)?;
        let ledger = self.ledger()?;
        let watermark = ledger
            .session_watermark(&session_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::NotFound)?;
        let facts = ledger
            .read_facts(&session_id, after_position, watermark.max_position, None)
            .map_err(map_sqlite)?;
        let mut items = facts
            .iter()
            .filter(|fact| fact.kind.as_str() == "artifact.committed")
            .map(|fact| artifact_projection(&session_id, fact))
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let scanned_through_position = items
            .last()
            .map_or(watermark.max_position, |item| item.committed_position);
        Ok(HostArtifactPage {
            api_version: "v1",
            session_id: session_id.as_str().into(),
            items,
            scanned_through_position,
            observed_max_position: watermark.max_position,
            has_more,
        })
    }

    /// Returns complete durable Turns changed after a caller watermark.
    pub fn read_timeline(
        &self,
        session: &str,
        after_position: u64,
        limit: usize,
    ) -> Result<TurnTimelinePage, LiveHostError> {
        if limit == 0 || limit > self.state.limits.event_batch_size as usize {
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
        let installed = self.installed_for_facts(&facts)?;
        let mut items = project_timeline(&facts, self.state.limits.max_command_bytes)?;
        if let (Some(catalogue), Some(limits)) = (
            installed.public_activity_catalogue.as_ref(),
            self.state.limits.activity,
        ) {
            let mut activities = project_activities(&facts, catalogue, limits)?.by_turn;
            for item in &mut items {
                item.activities = activities.remove(&item.turn_id).unwrap_or_default();
                if let Some(position) = item
                    .activities
                    .iter()
                    .map(|activity| activity.source_position)
                    .max()
                {
                    item.latest_position = item.latest_position.max(position);
                }
            }
            if !activities.is_empty() {
                return Err(LiveHostError::CorruptState);
            }
        }
        for item in items.iter_mut().filter(|item| item.state == "suspended") {
            let turn_id = identity::<TurnId>(&item.turn_id)?;
            let snapshot = ledger.load_turn(&turn_id).map_err(map_sqlite_query)?;
            let suspended = reconstruct_suspended_turn(&snapshot).map_err(map_runtime)?;
            item.suspension = Some(TurnSuspensionView {
                suspension_id: suspended.suspension_id,
                session_version: suspended.session_version,
                kind: suspension_kind(suspended.suspension_kind).into(),
            });
        }
        items.retain(|item| item.latest_position > after_position);
        let has_more = items.len() > limit;
        items.truncate(limit);
        let scanned_through_position = items
            .last()
            .map_or(watermark.max_position, |item| item.latest_position);
        Ok(TurnTimelinePage {
            api_version: "v1",
            session_id: session_id.as_str().to_owned(),
            items,
            scanned_through_position,
            observed_max_position: watermark.max_position,
            has_more,
        })
    }

    /// Creates or exactly replays one durable Session creation command.
    pub fn create_session(
        &self,
        idempotency_key: &str,
        agent_definition_id: &str,
    ) -> Result<CreateSessionResponse, LiveHostError> {
        validate_key(idempotency_key)?;
        let installed = self
            .state
            .installed
            .get(agent_definition_id)
            .ok_or(LiveHostError::NotFound)?;
        let session_id = SessionId::try_from(
            format!(
                "session-{}",
                digest(
                    format!("{}:{idempotency_key}", installed.agent_instance_namespace).as_bytes()
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
                        installed.definition_id,
                        installed.definition_revision
                    )
                    .as_bytes()
                )
            )
            .as_str(),
        )
        .map_err(|_| LiveHostError::InvalidRequest)?;
        let payload = json!({
            "command_id": idempotency_key,
            "definition_id": installed.definition_id,
            "definition_revision": installed.definition_revision,
            "snapshot_digest": installed.snapshot_digest,
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
        let installed = self.installed(binding.definition_id.as_str())?;
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
                agent_instance_id: binding.agent_instance_id.clone(),
                definition_id: binding.definition_id.clone(),
                definition_revision: binding.definition_revision.clone(),
                snapshot_digest: binding.snapshot_digest.clone(),
                trusted_input: trusted_input.to_owned(),
                limits: installed.runtime_limits,
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
                definition_id: binding.definition_id.as_str().to_owned(),
                definition_revision: binding.definition_revision.as_str().to_owned(),
                snapshot_digest: binding.snapshot_digest.clone(),
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

    /// Atomically commits selected Workspace text and the Turn that consumes it.
    #[allow(clippy::too_many_arguments)]
    pub fn start_turn_with_workspace_context(
        &self,
        idempotency_key: &str,
        session: &str,
        trusted_input: &str,
        workspace_id: &str,
        grant_revision: u64,
        entries: &[HostWorkspaceContextEntry],
    ) -> Result<TurnCommandResponse, LiveHostError> {
        validate_key(idempotency_key)?;
        validate_text(trusted_input, self.state.limits.max_command_bytes)?;
        let payload = workspace_context_payload(
            idempotency_key,
            workspace_id,
            grant_revision,
            entries,
            self.state.limits.max_command_bytes,
        )?;
        let session_id = identity::<SessionId>(session)?;
        let mut ledger = self.ledger()?;
        let binding = self.load_session(&ledger, &session_id)?;
        let installed = self.installed(binding.definition_id.as_str())?;
        let facts = ledger
            .read_facts(&session_id, 0, binding.max_position, None)
            .map_err(map_sqlite)?;
        let attached = self.session_workspaces(session)?.into_iter().any(|value| {
            value.workspace_id == workspace_id && value.grant_revision == grant_revision
        });
        if !attached {
            return Err(LiveHostError::InvalidRequest);
        }
        if let Some(response) = self.replay_contextual_start(
            &facts,
            &session_id,
            idempotency_key,
            trusted_input,
            &payload,
        )? {
            return Ok(response);
        }
        let recorded_at = self.recorded_at()?;
        let mut plan = plan_start_turn(
            &StartTurnCommand {
                command_id: RuntimeCommandId::new(idempotency_key).map_err(map_runtime)?,
                session_id: session_id.clone(),
                agent_instance_id: binding.agent_instance_id.clone(),
                definition_id: binding.definition_id.clone(),
                definition_revision: binding.definition_revision.clone(),
                snapshot_digest: binding.snapshot_digest.clone(),
                trusted_input: trusted_input.to_owned(),
                limits: installed.runtime_limits,
                recorded_at: recorded_at.clone(),
            },
            binding.max_position,
        )
        .map_err(map_runtime)?;
        let execution_id = plan
            .execution_id
            .clone()
            .ok_or(LiveHostError::CorruptState)?;
        plan.facts.insert(
            0,
            FactDraft {
                fact_id: FactId::try_from(
                    format!("workspace-context-{}", digest(idempotency_key.as_bytes())).as_str(),
                )
                .map_err(|_| LiveHostError::InvalidRequest)?,
                turn_id: None,
                execution_id: None,
                model_request_id: None,
                tool_invocation_id: None,
                kind: FactKind::new("workspace.context_selected")
                    .map_err(|_| LiveHostError::InvalidRequest)?,
                schema_version: 1,
                payload: CanonicalPayload::from_value(&payload)
                    .map_err(|_| LiveHostError::InvalidRequest)?,
                recorded_at,
            },
        );
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
                definition_id: binding.definition_id.as_str().to_owned(),
                definition_revision: binding.definition_revision.as_str().to_owned(),
                snapshot_digest: binding.snapshot_digest.clone(),
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
                definition_id: state.definition_id.as_str().to_owned(),
                definition_revision: state.definition_revision.as_str().to_owned(),
                snapshot_digest: state.snapshot_digest.clone(),
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
        if after_position > MAX_SAFE_JSON_INTEGER {
            return Err(LiveHostError::InvalidRequest);
        }
        let session_id = identity::<SessionId>(session)?;
        let ledger = self.ledger()?;
        let watermark = ledger
            .session_watermark(&session_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::NotFound)?;
        if watermark.max_position > MAX_SAFE_JSON_INTEGER {
            return Err(LiveHostError::ReadBoundExceeded);
        }
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
        let planner_kinds = BTreeSet::from([
            FactKind::new("plan.proposal.requested").map_err(|_| LiveHostError::CorruptState)?,
            FactKind::new("plan.replan.proposal.requested")
                .map_err(|_| LiveHostError::CorruptState)?,
        ]);
        let planner_facts = ledger
            .read_facts(&session_id, 0, through, Some(&planner_kinds))
            .map_err(map_sqlite)?;
        let internal = InternalPlannerTurns::from_facts(&planner_facts)?;
        let facts = ledger
            .read_facts(
                &session_id,
                if activity_enabled { 0 } else { after_position },
                through,
                None,
            )
            .map_err(map_sqlite)?;
        let activities = match self.state.limits.activity {
            Some(limits) => {
                let installed = self.installed_for_facts(&facts)?;
                let catalogue = installed
                    .public_activity_catalogue
                    .as_ref()
                    .ok_or(LiveHostError::CorruptState)?;
                Some(project_activities(&facts, catalogue, limits)?.events)
            }
            None => None,
        };
        let mut events = Vec::new();
        for fact in facts.iter().filter(|fact| fact.position > after_position) {
            if internal.contains_fact(fact) {
                continue;
            }
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

    /// Returns a host-clock RFC 3339 timestamp suitable for the management
    /// commit DAO. Same contract as [`Self::recorded_at`] but exposed for
    /// the management-port handler.
    pub(crate) fn recorded_at_string(&self) -> String {
        match self.recorded_at() {
            Ok(value) => value,
            Err(_) => self.state.clock.recorded_at(),
        }
    }

    /// Opens a fresh [`SqliteLedger`] bound to the same database path this
    /// Host was constructed against. The returned Ledger is independent of
    /// the Host's other operations; the management-port handler uses it
    /// for read/write traffic on the singleton `runtime_management_config`
    /// row.
    pub(crate) fn open_management_ledger(
        &self,
    ) -> Result<crate::SqliteLedger, crate::LiveHostError> {
        SqliteLedger::open(&self.state.database_path).map_err(map_sqlite)
    }

    fn installed(&self, definition_id: &str) -> Result<&InstalledAgent, LiveHostError> {
        self.state
            .installed
            .get(definition_id)
            .ok_or(LiveHostError::CorruptState)
    }

    fn installed_for_facts(&self, facts: &[DurableFact]) -> Result<&InstalledAgent, LiveHostError> {
        let opened = facts.first().ok_or(LiveHostError::CorruptState)?;
        if opened.position != 1 || opened.kind.as_str() != "session.opened" {
            return Err(LiveHostError::CorruptState);
        }
        let binding: SessionOpened = decode_payload(opened)?;
        let installed = self.installed(&binding.definition_id)?;
        if binding.definition_revision != installed.definition_revision
            || binding.snapshot_digest != installed.snapshot_digest
        {
            return Err(LiveHostError::CorruptState);
        }
        Ok(installed)
    }

    fn project_session(
        &self,
        ledger: &SqliteLedger,
        session_id: &SessionId,
    ) -> Result<SessionSummary, LiveHostError> {
        let binding = self.load_session(ledger, session_id)?;
        let facts = ledger
            .read_facts(session_id, 0, binding.max_position, None)
            .map_err(map_sqlite)?;
        let opened_at = facts
            .first()
            .ok_or(LiveHostError::CorruptState)?
            .recorded_at
            .clone();
        chrono::DateTime::parse_from_rfc3339(&opened_at)
            .map_err(|_| LiveHostError::CorruptState)?;
        let mut turns: Vec<(TurnId, String)> = Vec::new();
        for fact in &facts {
            let Some(turn_id) = fact.turn_id.as_ref() else {
                continue;
            };
            match fact.kind.as_str() {
                "turn.started" => {
                    let started: StartedCommand = decode_payload(fact)?;
                    if started.kind == "start" {
                        turns.push((turn_id.clone(), "running".into()));
                    } else if turns.last().map(|turn| &turn.0) != Some(turn_id) {
                        return Err(LiveHostError::CorruptState);
                    } else {
                        turns.last_mut().ok_or(LiveHostError::CorruptState)?.1 = "running".into();
                    }
                }
                "turn.suspended" => set_latest_turn_state(&mut turns, turn_id, "suspended")?,
                "turn.completed" => set_latest_turn_state(&mut turns, turn_id, "completed")?,
                "turn.stopped" => set_latest_turn_state(&mut turns, turn_id, "stopped")?,
                "turn.failed" => set_latest_turn_state(&mut turns, turn_id, "failed")?,
                _ => {}
            }
        }
        let latest = turns.last();
        Ok(SessionSummary {
            api_version: "v1",
            session_id: session_id.as_str().to_owned(),
            agent_instance_id: binding.agent_instance_id.as_str().to_owned(),
            definition_id: binding.definition_id.as_str().to_owned(),
            definition_revision: binding.definition_revision.as_str().to_owned(),
            opened_at,
            latest_position: binding.max_position,
            latest_turn_id: latest.map(|turn| turn.0.as_str().to_owned()),
            latest_turn_state: latest.map(|turn| turn.1.clone()),
            turn_count: turns.len() as u64,
        })
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
        let installed = self.installed(&payload.definition_id)?;
        if payload.command_id.is_empty()
            || payload.definition_revision != installed.definition_revision
            || payload.snapshot_digest != installed.snapshot_digest
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
        let installed = self.installed_for_facts(&facts)?;
        if facts
            .get(index.saturating_sub(1))
            .is_some_and(|fact| fact.kind.as_str() == "workspace.context_selected")
        {
            return Err(LiveHostError::CommandConflict);
        }
        if started.kind != "start"
            || started.definition_id != installed.definition_id
            || started.definition_revision != installed.definition_revision
            || started.snapshot_digest != installed.snapshot_digest
            || started.trusted_input_digest != digest(input.as_bytes())
        {
            return Err(LiveHostError::CommandConflict);
        }
        replay_started_batch(session_id, &facts, index, ReplayInput::Start(input), None)
    }

    fn replay_contextual_start(
        &self,
        facts: &[DurableFact],
        session_id: &SessionId,
        command_id: &str,
        input: &str,
        expected_context: &serde_json::Value,
    ) -> Result<Option<TurnCommandResponse>, LiveHostError> {
        let Some((index, started)) = find_started(facts, command_id)? else {
            reject_other_command(facts, command_id)?;
            return Ok(None);
        };
        let installed = self.installed_for_facts(facts)?;
        let context_index = index.checked_sub(1).ok_or(LiveHostError::CommandConflict)?;
        let context = facts
            .get(context_index)
            .filter(|fact| fact.kind.as_str() == "workspace.context_selected")
            .ok_or(LiveHostError::CommandConflict)?;
        let actual_context: serde_json::Value = decode_payload(context)?;
        if &actual_context != expected_context
            || started.kind != "start"
            || started.definition_id != installed.definition_id
            || started.definition_revision != installed.definition_revision
            || started.snapshot_digest != installed.snapshot_digest
            || started.trusted_input_digest != digest(input.as_bytes())
        {
            return Err(LiveHostError::CommandConflict);
        }
        replay_started_batch(session_id, facts, index, ReplayInput::Start(input), None)
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceAttached {
    command_id: String,
    workspace_id: String,
    display_name: String,
    grant_revision: u64,
    access: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceDetached {
    command_id: String,
    workspace_id: String,
    grant_revision: u64,
    outcome: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactCommitted {
    artifact_id: String,
    revision: u64,
    receipt_id: String,
    display_name: String,
    kind: String,
    mime_type: String,
    byte_size: u64,
    content_digest: String,
    verification: String,
    preview: String,
    workspace_id: String,
    revealable: bool,
    exportable: bool,
}

fn artifact_projection(
    session_id: &SessionId,
    fact: &DurableFact,
) -> Result<HostArtifact, LiveHostError> {
    let payload: ArtifactCommitted = decode_payload(fact)?;
    let _receipt_id = payload.receipt_id;
    Ok(HostArtifact {
        api_version: "v1",
        artifact_id: payload.artifact_id,
        revision: payload.revision,
        session_id: session_id.as_str().into(),
        turn_id: fact
            .turn_id
            .as_ref()
            .ok_or(LiveHostError::CorruptState)?
            .as_str()
            .into(),
        display_name: payload.display_name,
        kind: payload.kind,
        mime_type: payload.mime_type,
        byte_size: payload.byte_size,
        content_digest: payload.content_digest,
        committed_position: fact.position,
        verification: payload.verification,
        preview: payload.preview,
        workspace_id: Some(payload.workspace_id),
        revealable: payload.revealable,
        exportable: payload.exportable,
    })
}

fn workspace_attachment(
    session_id: &SessionId,
    payload: &WorkspaceAttached,
    position: u64,
) -> HostWorkspaceAttachment {
    HostWorkspaceAttachment {
        api_version: "v1",
        session_id: session_id.as_str().to_owned(),
        workspace_id: payload.workspace_id.clone(),
        display_name: payload.display_name.clone(),
        grant_revision: payload.grant_revision,
        access: payload.access.clone(),
        attached_position: position,
    }
}

fn workspace_detachment(
    session_id: &SessionId,
    payload: &WorkspaceDetached,
    position: u64,
) -> HostWorkspaceDetachment {
    HostWorkspaceDetachment {
        api_version: "v1",
        session_id: session_id.as_str().into(),
        workspace_id: payload.workspace_id.clone(),
        grant_revision: payload.grant_revision,
        outcome: payload.outcome.clone(),
        detached_position: position,
    }
}

fn set_latest_turn_state(
    turns: &mut [(TurnId, String)],
    turn_id: &TurnId,
    state: &str,
) -> Result<(), LiveHostError> {
    let latest = turns.last_mut().ok_or(LiveHostError::CorruptState)?;
    if &latest.0 != turn_id {
        return Err(LiveHostError::CorruptState);
    }
    latest.1 = state.to_owned();
    Ok(())
}

fn project_timeline(
    facts: &[DurableFact],
    max_text_bytes: usize,
) -> Result<Vec<TurnTimelineItem>, LiveHostError> {
    let mut items = Vec::new();
    for fact in facts {
        let Some(turn_id) = fact.turn_id.as_ref() else {
            continue;
        };
        match fact.kind.as_str() {
            "turn.started" => {
                let started: StartedCommand = decode_payload(fact)?;
                if started.kind == "start" {
                    items.push(TurnTimelineItem {
                        turn_id: turn_id.as_str().to_owned(),
                        started_position: fact.position,
                        latest_position: fact.position,
                        state: "running".into(),
                        cancellation_requested: false,
                        user_text: String::new(),
                        completion_text: None,
                        suspension: None,
                        content_truncated: false,
                        activities: Vec::new(),
                    });
                } else {
                    let item = timeline_item(&mut items, turn_id)?;
                    item.latest_position = fact.position;
                    item.state = "running".into();
                    item.completion_text = None;
                    item.suspension = None;
                }
            }
            "turn.input" => {
                let input: TurnInput = decode_payload(fact)?;
                if input.input_kind == "trusted_user" {
                    if digest(input.content.inline_utf8.as_bytes()) != input.content.digest {
                        return Err(LiveHostError::CorruptState);
                    }
                    let (text, truncated) =
                        bounded_text(&input.content.inline_utf8, max_text_bytes);
                    let item = timeline_item(&mut items, turn_id)?;
                    item.user_text = text;
                    item.content_truncated |= truncated;
                    item.latest_position = fact.position;
                }
            }
            "turn.cancel_requested" => {
                let _: Cancelled = decode_payload(fact)?;
                let item = timeline_item(&mut items, turn_id)?;
                if item.state != "running" {
                    return Err(LiveHostError::CorruptState);
                }
                item.latest_position = fact.position;
                item.cancellation_requested = true;
            }
            "turn.suspended" | "turn.stopped" | "turn.failed" => {
                let state = fact.kind.as_str().trim_start_matches("turn.");
                let item = timeline_item(&mut items, turn_id)?;
                item.latest_position = fact.position;
                item.state = state.into();
                item.cancellation_requested = false;
                if state != "suspended" {
                    item.suspension = None;
                }
            }
            "turn.completed" => {
                let (text, truncated) = bounded_text(&completion_text(fact)?, max_text_bytes);
                let item = timeline_item(&mut items, turn_id)?;
                item.latest_position = fact.position;
                item.state = "completed".into();
                item.cancellation_requested = false;
                item.completion_text = Some(text);
                item.suspension = None;
                item.content_truncated |= truncated;
            }
            _ => {}
        }
    }
    if items.iter().any(|item| item.user_text.is_empty()) {
        return Err(LiveHostError::CorruptState);
    }
    Ok(items)
}

fn timeline_item<'a>(
    items: &'a mut [TurnTimelineItem],
    turn_id: &TurnId,
) -> Result<&'a mut TurnTimelineItem, LiveHostError> {
    items
        .iter_mut()
        .rev()
        .find(|item| item.turn_id == turn_id.as_str())
        .ok_or(LiveHostError::CorruptState)
}

fn bounded_text(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }
    let boundary = value
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= max_bytes)
        .last()
        .unwrap_or(0);
    (value[..boundary].to_owned(), true)
}

fn replayed_goal_actor(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    command_id: &str,
    definition_json: &str,
) -> Result<Option<String>, LiveHostError> {
    let Some(watermark) = ledger.session_watermark(session_id).map_err(map_sqlite)? else {
        return Ok(None);
    };
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_sqlite)?;
    let matching = facts
        .iter()
        .filter(|fact| fact.fact_id.as_str() == command_id)
        .collect::<Vec<_>>();
    let [] = matching.as_slice() else {
        let [fact] = matching.as_slice() else {
            return Err(LiveHostError::CorruptState);
        };
        if fact.kind.as_str() != "goal.created" {
            return Err(LiveHostError::CommandConflict);
        }
        let value = serde_json::from_str::<serde_json::Value>(fact.payload.as_json())
            .ok()
            .and_then(|value| value.as_object().cloned())
            .ok_or(LiveHostError::CorruptState)?;
        let inline = value
            .get("definition")
            .and_then(serde_json::Value::as_object)
            .and_then(|binding| binding.get("inline_utf8"))
            .and_then(serde_json::Value::as_str);
        if value.get("command_id").and_then(serde_json::Value::as_str) != Some(command_id)
            || inline != Some(definition_json)
        {
            return Err(LiveHostError::CommandConflict);
        }
        return value
            .get("actor_reference")
            .and_then(serde_json::Value::as_str)
            .filter(|actor| !actor.is_empty())
            .map(str::to_owned)
            .map(Some)
            .ok_or(LiveHostError::CorruptState);
    };
    Ok(None)
}

fn replayed_cancel_response(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    command_id: &str,
    goal_id: &str,
    expected_revision: u64,
    reason: &str,
) -> Result<Option<GoalCommandResponseV1>, LiveHostError> {
    let Some(watermark) = ledger.session_watermark(session_id).map_err(map_sqlite)? else {
        return Ok(None);
    };
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_sqlite)?;
    let matching = facts
        .iter()
        .filter(|fact| fact.fact_id.as_str() == command_id)
        .collect::<Vec<_>>();
    let [] = matching.as_slice() else {
        let [fact] = matching.as_slice() else {
            return Err(LiveHostError::CorruptState);
        };
        let value = serde_json::from_str::<serde_json::Value>(fact.payload.as_json())
            .ok()
            .and_then(|value| value.as_object().cloned())
            .ok_or(LiveHostError::CorruptState)?;
        let revision = expected_revision
            .checked_add(1)
            .ok_or(LiveHostError::InvalidRequest)?;
        if fact.kind.as_str() != "goal.cancelled"
            || value.get("command_id").and_then(serde_json::Value::as_str) != Some(command_id)
            || value.get("goal_id").and_then(serde_json::Value::as_str) != Some(goal_id)
            || value.get("revision").and_then(serde_json::Value::as_u64) != Some(revision)
            || value.get("reason").and_then(serde_json::Value::as_str) != Some(reason)
        {
            return Err(LiveHostError::CommandConflict);
        }
        value
            .get("actor_reference")
            .and_then(serde_json::Value::as_str)
            .filter(|actor| !actor.is_empty())
            .ok_or(LiveHostError::CorruptState)?;
        let session_version = ledger
            .fact_commit_version(&fact.fact_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::CorruptState)?;
        return Ok(Some(GoalCommandResponseV1 {
            api_version: "v1",
            session_id: session_id.as_str().into(),
            goal_id: goal_id.into(),
            revision,
            state: "cancelled",
            session_version,
            committed_position: fact.position,
        }));
    };
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn replayed_revise_response(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    command_id: &str,
    goal_id: &str,
    expected_revision: u64,
    definition_json: &str,
    replacement_reason: &str,
) -> Result<Option<GoalCommandResponseV1>, LiveHostError> {
    let Some(watermark) = ledger.session_watermark(session_id).map_err(map_sqlite)? else {
        return Ok(None);
    };
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_sqlite)?;
    let matching = facts
        .iter()
        .filter(|fact| fact.fact_id.as_str() == command_id)
        .collect::<Vec<_>>();
    let [] = matching.as_slice() else {
        let [fact] = matching.as_slice() else {
            return Err(LiveHostError::CorruptState);
        };
        let value = serde_json::from_str::<serde_json::Value>(fact.payload.as_json())
            .ok()
            .and_then(|value| value.as_object().cloned())
            .ok_or(LiveHostError::CorruptState)?;
        let revision = expected_revision
            .checked_add(1)
            .ok_or(LiveHostError::InvalidRequest)?;
        let inline = value
            .get("definition")
            .and_then(serde_json::Value::as_object)
            .and_then(|binding| binding.get("inline_utf8"))
            .and_then(serde_json::Value::as_str);
        if fact.kind.as_str() != "goal.revised"
            || value.get("command_id").and_then(serde_json::Value::as_str) != Some(command_id)
            || value.get("goal_id").and_then(serde_json::Value::as_str) != Some(goal_id)
            || value
                .get("previous_revision")
                .and_then(serde_json::Value::as_u64)
                != Some(expected_revision)
            || value.get("revision").and_then(serde_json::Value::as_u64) != Some(revision)
            || value
                .get("replacement_reason")
                .and_then(serde_json::Value::as_str)
                != Some(replacement_reason)
            || inline != Some(definition_json)
        {
            return Err(LiveHostError::CommandConflict);
        }
        value
            .get("actor_reference")
            .and_then(serde_json::Value::as_str)
            .filter(|actor| !actor.is_empty())
            .ok_or(LiveHostError::CorruptState)?;
        let session_version = ledger
            .fact_commit_version(&fact.fact_id)
            .map_err(map_sqlite)?
            .ok_or(LiveHostError::CorruptState)?;
        return Ok(Some(GoalCommandResponseV1 {
            api_version: "v1",
            session_id: session_id.as_str().into(),
            goal_id: goal_id.into(),
            revision,
            state: "draft",
            session_version,
            committed_position: fact.position,
        }));
    };
    Ok(None)
}

fn goal_command_response(
    session: &str,
    state: &crate::GoalRuntimeState,
    committed: &CommitResult,
) -> Result<GoalCommandResponseV1, LiveHostError> {
    let position = only_position(&committed.positions)?;
    if committed.session_version > MAX_SAFE_JSON_INTEGER || position > MAX_SAFE_JSON_INTEGER {
        return Err(LiveHostError::CorruptState);
    }
    Ok(GoalCommandResponseV1 {
        api_version: "v1",
        session_id: session.into(),
        goal_id: state.snapshot.definition().goal_id().as_str().into(),
        revision: state.snapshot.revision(),
        state: goal_state(state.snapshot.state()),
        session_version: committed.session_version,
        committed_position: position,
    })
}

const fn goal_state(state: GoalState) -> &'static str {
    match state {
        GoalState::Draft => "draft",
        GoalState::Active => "active",
        GoalState::Suspended => "suspended",
        GoalState::Succeeded => "succeeded",
        GoalState::Failed => "failed",
        GoalState::Cancelled => "cancelled",
    }
}

const fn plan_state(state: PlanState) -> &'static str {
    match state {
        PlanState::Proposed => "proposed",
        PlanState::Adopted => "adopted",
        PlanState::Running => "running",
        PlanState::Suspended => "suspended",
        PlanState::Completed => "completed",
        PlanState::Failed => "failed",
        PlanState::Superseded => "superseded",
        PlanState::Rejected => "rejected",
    }
}

fn suspension_kind(kind: RuntimeSuspensionKind) -> &'static str {
    match kind {
        RuntimeSuspensionKind::ApprovalRequired => "approval_required",
        RuntimeSuspensionKind::ExternalInputRequired => "external_input_required",
        RuntimeSuspensionKind::OperatorReconciliation => "operator_reconciliation",
        RuntimeSuspensionKind::ResourceUnavailable => "resource_unavailable",
        RuntimeSuspensionKind::PartialOutput => "partial_output",
        RuntimeSuspensionKind::DelegationPending => "delegation_pending",
    }
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
            "session.opened"
                | "turn.started"
                | "turn.cancel_requested"
                | "workspace.attached"
                | "workspace.detached"
                | "workspace.context_selected"
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

fn workspace_context_payload(
    command_id: &str,
    workspace_id: &str,
    grant_revision: u64,
    entries: &[HostWorkspaceContextEntry],
    max_bytes: usize,
) -> Result<serde_json::Value, LiveHostError> {
    validate_key(workspace_id)?;
    if grant_revision == 0 || entries.is_empty() || entries.len() > 8 {
        return Err(LiveHostError::InvalidRequest);
    }
    let mut identities = BTreeSet::new();
    let mut total = 0usize;
    let mut values = Vec::with_capacity(entries.len());
    for entry in entries {
        validate_key(&entry.entry_id)?;
        validate_text(&entry.display_name, 128)?;
        if entry.kind != "text"
            || entry.content_digest != digest(entry.content_utf8.as_bytes())
            || !identities.insert(entry.entry_id.as_str())
        {
            return Err(LiveHostError::InvalidRequest);
        }
        total = total
            .checked_add(entry.content_utf8.len())
            .filter(|value| *value <= max_bytes.min(60 * 1_024))
            .ok_or(LiveHostError::InvalidRequest)?;
        values.push(serde_json::json!({
            "entry_id":entry.entry_id,
            "display_name":entry.display_name,
            "kind":entry.kind,
            "content":{
                "digest":entry.content_digest,
                "inline_utf8":entry.content_utf8,
            },
        }));
    }
    Ok(serde_json::json!({
        "command_id":command_id,
        "workspace_id":workspace_id,
        "grant_revision":grant_revision,
        "entries":values,
    }))
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

fn map_goal_authority(error: GoalCommandAuthorityError) -> LiveHostError {
    match error {
        GoalCommandAuthorityError::Denied => LiveHostError::PreconditionFailed,
        GoalCommandAuthorityError::Unavailable => LiveHostError::DurabilityUnavailable,
    }
}

fn map_goal_runtime(error: GoalRuntimeError) -> LiveHostError {
    match error {
        GoalRuntimeError::Invalid => LiveHostError::InvalidRequest,
        GoalRuntimeError::NotFound => LiveHostError::NotFound,
        GoalRuntimeError::RevisionConflict => LiveHostError::ConcurrentModification,
        GoalRuntimeError::CommandConflict => LiveHostError::CommandConflict,
        GoalRuntimeError::TransitionInvalid
        | GoalRuntimeError::EvidenceInsufficient
        | GoalRuntimeError::EvidenceInvalid
        | GoalRuntimeError::ScopeExceeded
        | GoalRuntimeError::Cycle => LiveHostError::PreconditionFailed,
        GoalRuntimeError::RecoveryCorrupt => LiveHostError::CorruptState,
        GoalRuntimeError::DurabilityFailure => LiveHostError::DurabilityUnavailable,
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
