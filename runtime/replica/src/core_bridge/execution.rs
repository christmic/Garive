use std::{collections::BTreeMap, sync::Mutex};

use garive_core::{
    execute_agent, execute_model_only, AgentEvent, AgentEventKind, AgentExecutionPorts,
    AgentToolCapabilities, AgentTurnRequest, ClockPort, ContextPort, EventSink, PortFailure,
};
use garive_ledger::{
    CanonicalPayload, CommitResult, FactDraft, FactId, FactKind, LedgerError, SessionId, TurnId,
};
use garive_llm::{
    ModelCancellation, ModelFuture, ModelObserver, ModelPort, ModelPortFailure, ModelRequest,
};
use serde_json::json;

use crate::{ExecutionLease, RuntimeCommandError, SqliteLedger, SqliteLedgerError};

use super::encoding::digest;
use super::{
    plan_core_terminal, plan_model_prepared, plan_model_started, plan_model_terminal,
    plan_model_uncertain, AuthorityPort, CoreTerminalContext, ExecutorPort, GovernedEffectConfig,
    ModelLifecycleContext, PlannedSkillActivation, RuntimeModelUncertainReason,
    SqliteGovernedEffectPort,
};

use super::execution_types::{
    DurableExecutionConfig, DurableExecutionError, DurableExecutionResult, TerminalPublisher,
};

/// Runs Core with a model port whose external boundaries are durably ordered.
#[allow(clippy::too_many_arguments)]
pub async fn execute_durable_model_only(
    ledger: &mut SqliteLedger,
    config: &DurableExecutionConfig,
    request: &AgentTurnRequest,
    context: &mut dyn ContextPort,
    model: &dyn ModelPort,
    events: &mut dyn EventSink,
    cancellation: &dyn ModelCancellation,
    clock: &dyn ClockPort,
    publisher: &mut dyn TerminalPublisher,
) -> Result<DurableExecutionResult, DurableExecutionError> {
    execute_durable_model_only_inner(
        ledger,
        config,
        request,
        None,
        context,
        model,
        events,
        cancellation,
        clock,
        publisher,
    )
    .await
}

/// Runs model-only Core after atomically committing one exact S0 activation.
#[allow(clippy::too_many_arguments)]
pub async fn execute_durable_model_only_with_skill_activation(
    ledger: &mut SqliteLedger,
    config: &DurableExecutionConfig,
    request: &AgentTurnRequest,
    activation: PlannedSkillActivation,
    context: &mut dyn ContextPort,
    model: &dyn ModelPort,
    events: &mut dyn EventSink,
    cancellation: &dyn ModelCancellation,
    clock: &dyn ClockPort,
    publisher: &mut dyn TerminalPublisher,
) -> Result<DurableExecutionResult, DurableExecutionError> {
    execute_durable_model_only_inner(
        ledger,
        config,
        request,
        Some(activation),
        context,
        model,
        events,
        cancellation,
        clock,
        publisher,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn execute_durable_model_only_inner(
    ledger: &mut SqliteLedger,
    config: &DurableExecutionConfig,
    request: &AgentTurnRequest,
    activation: Option<PlannedSkillActivation>,
    context: &mut dyn ContextPort,
    model: &dyn ModelPort,
    events: &mut dyn EventSink,
    cancellation: &dyn ModelCancellation,
    clock: &dyn ClockPort,
    publisher: &mut dyn TerminalPublisher,
) -> Result<DurableExecutionResult, DurableExecutionError> {
    validate_identity(config, request)?;
    validate_ledger_watermark(ledger, config, request)?;
    let cancellation_requested = has_cancellation_request(ledger, &config.model.turn_id)?;
    let lease = ledger
        .acquire_execution_lease(&config.lease)
        .map_err(DurableExecutionError::Lease)?;
    let mut coordinator = CommitCoordinator {
        ledger,
        lease,
        session_id: config.session_id.clone(),
        turn_id: config.model.turn_id.clone(),
        version: config.expected_session_version,
        position: request.context_request.through_position,
        cancellation_requested,
        failure: None,
    };
    let effective_request = prepare_activation(&mut coordinator, request, activation)?;
    let coordinator = Mutex::new(coordinator);
    let prepared_events = Mutex::new(BTreeMap::new());
    let durable_model = DurableModelPort {
        inner: model,
        coordinator: &coordinator,
        lifecycle: &config.model,
        prepared_events: &prepared_events,
    };
    let mut gated_events = PreparedEventGate {
        downstream: events,
        prepared_events: &prepared_events,
        coordinator: &coordinator,
        lifecycle: &config.model,
    };
    let durable_cancellation = DurableCancellation {
        upstream: cancellation,
        coordinator: &coordinator,
        turn_id: &config.model.turn_id,
    };
    let report = {
        let mut ports = AgentExecutionPorts {
            context,
            model: &durable_model,
            events: &mut gated_events,
            cancellation: &durable_cancellation,
            clock,
        };
        execute_model_only(&effective_request, &mut ports).await
    };
    finish_durable_execution(coordinator, config, report, publisher)
}

fn finish_durable_execution(
    coordinator: Mutex<CommitCoordinator<'_>>,
    config: &DurableExecutionConfig,
    report: garive_core::ExecutionReport,
    publisher: &mut dyn TerminalPublisher,
) -> Result<DurableExecutionResult, DurableExecutionError> {
    let mut coordinator = coordinator
        .into_inner()
        .map_err(|_| DurableExecutionError::Coordination)?;
    if let Some(failure) = coordinator.failure.take() {
        return Err(failure);
    }
    coordinator.observe_durable_cancellation(&config.model.turn_id);
    if let Some(failure) = coordinator.failure.take() {
        return Err(failure);
    }
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: config.model.turn_id.clone(),
            execution_id: config.model.execution_id.clone(),
            recorded_at: config.model.recorded_at.clone(),
        },
        &report,
    )
    .map_err(DurableExecutionError::Command)?;
    let terminal_commit = coordinator.commit(terminal)?;
    coordinator
        .ledger
        .release_execution_lease(&coordinator.lease)
        .map_err(DurableExecutionError::Lease)?;
    let publication = publisher.publish_terminal(&report, &terminal_commit.positions);
    Ok(DurableExecutionResult {
        report,
        terminal_commit,
        publication,
    })
}

/// Runs the complete tool-capable Core loop with one coordinated durable writer.
#[allow(clippy::too_many_arguments)]
pub async fn execute_durable_agent(
    ledger: &mut SqliteLedger,
    config: &DurableExecutionConfig,
    request: &AgentTurnRequest,
    capabilities: &AgentToolCapabilities,
    context: &mut dyn ContextPort,
    model: &dyn ModelPort,
    authority: &mut dyn AuthorityPort,
    executor: &mut dyn ExecutorPort,
    events: &mut dyn EventSink,
    cancellation: &dyn ModelCancellation,
    clock: &dyn ClockPort,
    publisher: &mut dyn TerminalPublisher,
) -> Result<DurableExecutionResult, DurableExecutionError> {
    execute_durable_agent_inner(
        ledger,
        config,
        request,
        None,
        capabilities,
        context,
        model,
        authority,
        executor,
        events,
        cancellation,
        clock,
        publisher,
    )
    .await
}

/// Runs tool-capable Core after atomically committing one exact S0 activation.
#[allow(clippy::too_many_arguments)]
pub async fn execute_durable_agent_with_skill_activation(
    ledger: &mut SqliteLedger,
    config: &DurableExecutionConfig,
    request: &AgentTurnRequest,
    activation: PlannedSkillActivation,
    capabilities: &AgentToolCapabilities,
    context: &mut dyn ContextPort,
    model: &dyn ModelPort,
    authority: &mut dyn AuthorityPort,
    executor: &mut dyn ExecutorPort,
    events: &mut dyn EventSink,
    cancellation: &dyn ModelCancellation,
    clock: &dyn ClockPort,
    publisher: &mut dyn TerminalPublisher,
) -> Result<DurableExecutionResult, DurableExecutionError> {
    execute_durable_agent_inner(
        ledger,
        config,
        request,
        Some(activation),
        capabilities,
        context,
        model,
        authority,
        executor,
        events,
        cancellation,
        clock,
        publisher,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn execute_durable_agent_inner(
    ledger: &mut SqliteLedger,
    config: &DurableExecutionConfig,
    request: &AgentTurnRequest,
    activation: Option<PlannedSkillActivation>,
    capabilities: &AgentToolCapabilities,
    context: &mut dyn ContextPort,
    model: &dyn ModelPort,
    authority: &mut dyn AuthorityPort,
    executor: &mut dyn ExecutorPort,
    events: &mut dyn EventSink,
    cancellation: &dyn ModelCancellation,
    clock: &dyn ClockPort,
    publisher: &mut dyn TerminalPublisher,
) -> Result<DurableExecutionResult, DurableExecutionError> {
    validate_identity(config, request)?;
    validate_ledger_watermark(ledger, config, request)?;
    let cancellation_requested = has_cancellation_request(ledger, &config.model.turn_id)?;
    let lease = ledger
        .acquire_execution_lease(&config.lease)
        .map_err(DurableExecutionError::Lease)?;
    let mut coordinator = CommitCoordinator {
        ledger,
        lease,
        session_id: config.session_id.clone(),
        turn_id: config.model.turn_id.clone(),
        version: config.expected_session_version,
        position: request.context_request.through_position,
        cancellation_requested,
        failure: None,
    };
    let effective_request = prepare_activation(&mut coordinator, request, activation)?;
    let coordinator = Mutex::new(coordinator);
    let prepared_events = Mutex::new(BTreeMap::new());
    let durable_model = DurableModelPort {
        inner: model,
        coordinator: &coordinator,
        lifecycle: &config.model,
        prepared_events: &prepared_events,
    };
    let mut gated_events = PreparedEventGate {
        downstream: events,
        prepared_events: &prepared_events,
        coordinator: &coordinator,
        lifecycle: &config.model,
    };
    let durable_cancellation = DurableCancellation {
        upstream: cancellation,
        coordinator: &coordinator,
        turn_id: &config.model.turn_id,
    };
    let report = {
        let effect_version = coordinator
            .lock()
            .map_err(|_| DurableExecutionError::Coordination)?
            .version();
        let mut effects = SqliteGovernedEffectPort::coordinated(
            &coordinator,
            authority,
            executor,
            GovernedEffectConfig {
                session_id: config.session_id.clone(),
                expected_session_version: effect_version,
                initial_through_position: effective_request.context_request.through_position,
                turn_id: config.model.turn_id.clone(),
                execution_id: config.model.execution_id.clone(),
                recorded_at: config.model.recorded_at.clone(),
            },
        )
        .map_err(|_| DurableExecutionError::Command(RuntimeCommandError::InvalidCommand))?;
        let mut ports = AgentExecutionPorts {
            context,
            model: &durable_model,
            events: &mut gated_events,
            cancellation: &durable_cancellation,
            clock,
        };
        execute_agent(&effective_request, capabilities, &mut ports, &mut effects).await
    };
    finish_durable_execution(coordinator, config, report, publisher)
}

fn prepare_activation(
    coordinator: &mut CommitCoordinator<'_>,
    request: &AgentTurnRequest,
    activation: Option<PlannedSkillActivation>,
) -> Result<AgentTurnRequest, DurableExecutionError> {
    let mut effective = request.clone();
    if let Some(activation) = activation {
        coordinator.commit(vec![activation.fact])?;
        effective.context_request.through_position = coordinator.position();
        effective.activated_skills = activation.activated_skills;
    }
    Ok(effective)
}

pub(super) struct CommitCoordinator<'a> {
    ledger: &'a mut SqliteLedger,
    lease: ExecutionLease,
    session_id: SessionId,
    turn_id: TurnId,
    version: u64,
    position: u64,
    cancellation_requested: bool,
    failure: Option<DurableExecutionError>,
}

impl CommitCoordinator<'_> {
    pub(super) fn commit(
        &mut self,
        facts: Vec<FactDraft>,
    ) -> Result<CommitResult, DurableExecutionError> {
        let turn_id = self.turn_id.clone();
        self.observe_durable_cancellation(&turn_id);
        if self.failure.is_some() {
            return Err(DurableExecutionError::Command(
                RuntimeCommandError::ConcurrentModification,
            ));
        }
        let first = self.ledger.commit_leased(
            &self.lease,
            self.session_id.clone(),
            self.version,
            facts.clone(),
        );
        let result = match first {
            Err(SqliteLedgerError::Domain(LedgerError::ConcurrentModification)) => {
                self.observe_durable_cancellation(&turn_id);
                if self.failure.is_some() {
                    return Err(DurableExecutionError::Command(
                        RuntimeCommandError::ConcurrentModification,
                    ));
                }
                self.ledger
                    .commit_leased(&self.lease, self.session_id.clone(), self.version, facts)
                    .map_err(DurableExecutionError::Ledger)?
            }
            Err(error) => return Err(DurableExecutionError::Ledger(error)),
            Ok(result) => result,
        };
        self.version = result.session_version;
        let committed_position =
            result
                .positions
                .last()
                .copied()
                .ok_or(DurableExecutionError::Command(
                    RuntimeCommandError::InvariantViolation,
                ))?;
        self.position = self.position.max(committed_position);
        Ok(result)
    }

    pub(super) const fn version(&self) -> u64 {
        self.version
    }

    pub(super) const fn position(&self) -> u64 {
        self.position
    }

    pub(super) fn record_failure(&mut self, failure: DurableExecutionError) {
        if self.failure.is_none() {
            self.failure = Some(failure);
        }
    }

    fn observe_durable_cancellation(&mut self, turn_id: &garive_ledger::TurnId) -> bool {
        if self.failure.is_some() {
            return true;
        }
        let snapshot = match self.ledger.load_turn(turn_id) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.failure = Some(DurableExecutionError::Ledger(error));
                return true;
            }
        };
        if snapshot.session_version == self.version {
            return self.cancellation_requested;
        }
        if snapshot.session_version < self.version || snapshot.through_position <= self.position {
            self.failure = Some(DurableExecutionError::Command(
                RuntimeCommandError::InvariantViolation,
            ));
            return true;
        }
        let appended = match self.ledger.read_facts(
            &self.session_id,
            self.position,
            snapshot.through_position,
            None,
        ) {
            Ok(facts) => facts,
            Err(error) => {
                self.failure = Some(DurableExecutionError::Ledger(error));
                return true;
            }
        };
        if appended.is_empty()
            || appended.iter().any(|fact| {
                fact.kind.as_str() != "turn.cancel_requested"
                    || fact.turn_id.as_ref() != Some(turn_id)
            })
        {
            self.failure = Some(DurableExecutionError::Command(
                RuntimeCommandError::ConcurrentModification,
            ));
            return true;
        }
        self.version = snapshot.session_version;
        self.position = snapshot.through_position;
        self.cancellation_requested = true;
        true
    }

    fn append_for_model(&mut self, fact: FactDraft) -> Result<(), ModelPortFailure> {
        match self.commit(vec![fact]) {
            Ok(_) => Ok(()),
            Err(error) => {
                self.record_failure(error);
                Err(ModelPortFailure::RequiredPortFailure)
            }
        }
    }

    fn append_for_event(&mut self, fact: FactDraft) -> Result<(), PortFailure> {
        match self.commit(vec![fact]) {
            Ok(_) => Ok(()),
            Err(error) => {
                self.record_failure(error);
                Err(PortFailure::Event)
            }
        }
    }
}

struct DurableCancellation<'a, 'ledger> {
    upstream: &'a dyn ModelCancellation,
    coordinator: &'a Mutex<CommitCoordinator<'ledger>>,
    turn_id: &'a garive_ledger::TurnId,
}

impl ModelCancellation for DurableCancellation<'_, '_> {
    fn is_cancelled(&self) -> bool {
        if self.upstream.is_cancelled() {
            return true;
        }
        match self.coordinator.lock() {
            Ok(mut coordinator) => coordinator.observe_durable_cancellation(self.turn_id),
            Err(_) => true,
        }
    }
}

struct PreparedEventGate<'a, 'ledger> {
    downstream: &'a mut dyn EventSink,
    prepared_events: &'a Mutex<BTreeMap<String, String>>,
    coordinator: &'a Mutex<CommitCoordinator<'ledger>>,
    lifecycle: &'a ModelLifecycleContext,
}

impl EventSink for PreparedEventGate<'_, '_> {
    fn emit(&mut self, event: AgentEvent) -> Result<(), PortFailure> {
        if let AgentEventKind::IterationStarted { iteration } = &event.kind {
            let fact = plan_iteration_started(self.lifecycle, *iteration)?;
            self.coordinator
                .lock()
                .map_err(|_| PortFailure::Event)?
                .append_for_event(fact)?;
        }
        if let AgentEventKind::ModelRequestPrepared {
            request_id,
            target_id,
        } = &event.kind
        {
            self.prepared_events
                .lock()
                .map_err(|_| PortFailure::Event)?
                .insert(request_id.clone(), target_id.clone());
        }
        self.downstream.emit(event)
    }
}

fn plan_iteration_started(
    lifecycle: &ModelLifecycleContext,
    iteration: u32,
) -> Result<FactDraft, PortFailure> {
    if iteration == 0 {
        return Err(PortFailure::Event);
    }
    let seed = format!("{}:iteration:{iteration}", lifecycle.execution_id.as_str());
    Ok(FactDraft {
        fact_id: FactId::try_from(format!("fact-{}", digest(seed.as_bytes())).as_str())
            .map_err(|_| PortFailure::Event)?,
        turn_id: Some(lifecycle.turn_id.clone()),
        execution_id: Some(lifecycle.execution_id.clone()),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("execution.iteration_started").map_err(|_| PortFailure::Event)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({"iteration":iteration}))
            .map_err(|_| PortFailure::Event)?,
        recorded_at: lifecycle.recorded_at.clone(),
    })
}

struct DurableModelPort<'a, 'ledger> {
    inner: &'a dyn ModelPort,
    coordinator: &'a Mutex<CommitCoordinator<'ledger>>,
    lifecycle: &'a ModelLifecycleContext,
    prepared_events: &'a Mutex<BTreeMap<String, String>>,
}

impl ModelPort for DurableModelPort<'_, '_> {
    fn preflight(&self, request: &ModelRequest) -> Result<(), ModelPortFailure> {
        self.inner.preflight(request)
    }

    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        observer: &'a mut dyn ModelObserver,
        cancellation: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let target = self
                .prepared_events
                .lock()
                .map_err(|_| ModelPortFailure::RequiredPortFailure)?
                .remove(request.request_id.as_str());
            if target.as_deref() != Some(request.target_id.as_str()) {
                return Err(ModelPortFailure::RequiredPortFailure);
            }
            let prepared = plan_model_prepared(self.lifecycle, request)
                .map_err(|_| ModelPortFailure::RequiredPortFailure)?;
            self.append(prepared.fact.clone())?;
            let attempt = format!("dispatch-{}-1", request.request_id.as_str());
            self.append(
                plan_model_started(self.lifecycle, &prepared, &attempt)
                    .map_err(|_| ModelPortFailure::RequiredPortFailure)?,
            )?;
            let result = self.inner.invoke(request, observer, cancellation).await;
            let terminal = match &result {
                Ok(outcome) => plan_model_terminal(self.lifecycle, &prepared, outcome),
                Err(_) => plan_model_uncertain(
                    self.lifecycle,
                    &prepared,
                    RuntimeModelUncertainReason::ProviderStateUnknown,
                ),
            }
            .map_err(|_| ModelPortFailure::RequiredPortFailure)?;
            self.append(terminal)?;
            result
        })
    }
}

impl DurableModelPort<'_, '_> {
    fn append(&self, fact: FactDraft) -> Result<(), ModelPortFailure> {
        self.coordinator
            .lock()
            .map_err(|_| ModelPortFailure::RequiredPortFailure)?
            .append_for_model(fact)
    }
}

fn validate_identity(
    config: &DurableExecutionConfig,
    request: &AgentTurnRequest,
) -> Result<(), DurableExecutionError> {
    if config.session_id.as_str() != request.session_id.as_str()
        || config.model.turn_id.as_str() != request.turn_id.as_str()
        || config.model.execution_id.as_str() != request.execution_id.as_str()
        || config.lease.turn_id != config.model.turn_id
        || config.lease.execution_id != config.model.execution_id
    {
        Err(DurableExecutionError::Command(
            RuntimeCommandError::InvalidCommand,
        ))
    } else {
        Ok(())
    }
}

fn validate_ledger_watermark(
    ledger: &SqliteLedger,
    config: &DurableExecutionConfig,
    request: &AgentTurnRequest,
) -> Result<(), DurableExecutionError> {
    let turn = ledger
        .load_turn(&config.model.turn_id)
        .map_err(DurableExecutionError::Ledger)?;
    if turn.session_version != config.expected_session_version
        || turn.through_position != request.context_request.through_position
        || turn
            .facts
            .iter()
            .any(|fact| fact.session_id != config.session_id)
        || !turn.facts.iter().any(|fact| {
            fact.execution_id.as_ref() == Some(&config.model.execution_id)
                && fact.kind.as_str() == "execution.started"
        })
        || turn.facts.iter().any(|fact| {
            fact.execution_id.as_ref() == Some(&config.model.execution_id)
                && matches!(
                    fact.kind.as_str(),
                    "execution.abandoned"
                        | "execution.completed"
                        | "execution.suspended"
                        | "execution.stopped"
                        | "execution.failed"
                )
        })
    {
        Err(DurableExecutionError::Command(
            RuntimeCommandError::ConcurrentModification,
        ))
    } else {
        Ok(())
    }
}

fn has_cancellation_request(
    ledger: &SqliteLedger,
    turn_id: &garive_ledger::TurnId,
) -> Result<bool, DurableExecutionError> {
    Ok(ledger
        .load_turn(turn_id)
        .map_err(DurableExecutionError::Ledger)?
        .facts
        .iter()
        .any(|fact| fact.kind.as_str() == "turn.cancel_requested"))
}
