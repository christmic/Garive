use std::{collections::BTreeMap, sync::Mutex};

use garive_core::{
    execute_model_only, AgentEvent, AgentEventKind, AgentExecutionPorts, AgentTurnRequest,
    ClockPort, ContextPort, EventSink, PortFailure,
};
use garive_ledger::{CommitResult, FactDraft, SessionId};
use garive_llm::{
    ModelCancellation, ModelFuture, ModelObserver, ModelPort, ModelPortFailure, ModelRequest,
};

use crate::{RuntimeCommandError, SqliteLedger};

use super::{
    plan_core_terminal, plan_model_prepared, plan_model_started, plan_model_terminal,
    plan_model_uncertain, CoreTerminalContext, ModelLifecycleContext, RuntimeModelUncertainReason,
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
    validate_identity(config, request)?;
    let coordinator = Mutex::new(CommitCoordinator {
        ledger,
        session_id: config.session_id.clone(),
        version: config.expected_session_version,
        failure: None,
    });
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
    };
    let report = {
        let mut ports = AgentExecutionPorts {
            context,
            model: &durable_model,
            events: &mut gated_events,
            cancellation,
            clock,
        };
        execute_model_only(request, &mut ports).await
    };
    let mut coordinator = coordinator
        .into_inner()
        .map_err(|_| DurableExecutionError::Coordination)?;
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
    let publication = publisher.publish_terminal(&report, &terminal_commit.positions);
    Ok(DurableExecutionResult {
        report,
        terminal_commit,
        publication,
    })
}

struct CommitCoordinator<'a> {
    ledger: &'a mut SqliteLedger,
    session_id: SessionId,
    version: u64,
    failure: Option<DurableExecutionError>,
}

impl CommitCoordinator<'_> {
    fn commit(&mut self, facts: Vec<FactDraft>) -> Result<CommitResult, DurableExecutionError> {
        let result = self
            .ledger
            .commit(self.session_id.clone(), self.version, facts)
            .map_err(DurableExecutionError::Ledger)?;
        self.version = result.session_version;
        Ok(result)
    }

    fn append_for_model(&mut self, fact: FactDraft) -> Result<(), ModelPortFailure> {
        match self.commit(vec![fact]) {
            Ok(_) => Ok(()),
            Err(error) => {
                self.failure = Some(error);
                Err(ModelPortFailure::RequiredPortFailure)
            }
        }
    }
}

struct PreparedEventGate<'a> {
    downstream: &'a mut dyn EventSink,
    prepared_events: &'a Mutex<BTreeMap<String, String>>,
}

impl EventSink for PreparedEventGate<'_> {
    fn emit(&mut self, event: AgentEvent) -> Result<(), PortFailure> {
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

struct DurableModelPort<'a, 'ledger> {
    inner: &'a dyn ModelPort,
    coordinator: &'a Mutex<CommitCoordinator<'ledger>>,
    lifecycle: &'a ModelLifecycleContext,
    prepared_events: &'a Mutex<BTreeMap<String, String>>,
}

impl ModelPort for DurableModelPort<'_, '_> {
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
    {
        Err(DurableExecutionError::Command(
            RuntimeCommandError::InvalidCommand,
        ))
    } else {
        Ok(())
    }
}
