use garive_llm::{
    ModelCancellation, ModelObserver, ModelRequest, ModelRequestId, ModelStreamEvent,
    ObserverDecision, TokenCount,
};

use crate::{
    AgentEvent, AgentEventKind, AgentExecutionPorts, AgentFailureReason, AgentOutcome,
    AgentTurnRequest, ContextItem, ContextSurface, EventSink, ExecutionControl,
    ExecutionOutcomeKind, ExecutionReport, ExecutionStatus, MissingUsagePolicy, ModelRecoveryPolicy,
    StopReason, SuspensionReason, TerminalRecoveryAction, UsageSummary,
};

pub(super) fn build_model_request(
    request: &AgentTurnRequest,
    surface: ContextSurface,
    iteration: u32,
    target_index: usize,
) -> Result<(ModelRequest, String), ()> {
    let request_id = format!("{}:{iteration}", request.execution_id.as_str());
    let target = request.model_targets[target_index].clone();
    let value = ModelRequest {
        request_id: ModelRequestId::new(request_id.clone()),
        target_id: target,
        required_capabilities: request.required_capabilities.clone(),
        input_items: surface
            .items
            .into_iter()
            .filter_map(|value| match value {
                ContextItem::Input { item, .. } => Some(item),
                ContextItem::RedactedItem { .. } => None,
            })
            .collect(),
        tools: vec![],
        output: request.model_output.clone(),
        trace_metadata: vec![
            ("turn_id".into(), request.turn_id.as_str().into()),
            (
                "execution_id".into(),
                request.execution_id.as_str().into(),
            ),
        ],
    };
    value.validate().map_err(|_| ())?;
    Ok((value, request_id))
}

pub(super) fn prepare_control(
    request: &AgentTurnRequest,
) -> Result<ExecutionControl, ExecutionReport> {
    if request.validate().is_err() {
        return Err(invalid_report(request));
    }
    ExecutionControl::new(
        request.turn_id.clone(),
        request.execution_id.clone(),
        request.cursor.completed_iterations,
        request.limits.execution,
    )
    .map_err(|_| invalid_report(request))
}

fn invalid_report(request: &AgentTurnRequest) -> ExecutionReport {
    ExecutionReport {
        outcome: AgentOutcome::Failed {
            reason: AgentFailureReason::InvalidInput,
        },
        completed_iterations: request.cursor.completed_iterations,
    }
}

#[derive(Default)]
pub(super) struct UsageAccumulator {
    input: u64,
    output: u64,
    estimated: bool,
}

impl UsageAccumulator {
    fn add(
        &mut self,
        model_usage: garive_llm::ModelUsage,
        policy: MissingUsagePolicy,
    ) -> Result<(), UsageError> {
        let (input, input_estimated) = known_or_estimate(model_usage.input_tokens, policy, true)?;
        let (output, output_estimated) =
            known_or_estimate(model_usage.output_tokens, policy, false)?;
        self.input = self.input.checked_add(input).ok_or(UsageError::Overflow)?;
        self.output = self.output.checked_add(output).ok_or(UsageError::Overflow)?;
        self.estimated |= input_estimated || output_estimated;
        Ok(())
    }

    fn total(&self) -> Option<u64> {
        self.input.checked_add(self.output)
    }

    pub(super) const fn summary(&self) -> UsageSummary {
        UsageSummary {
            input_tokens: self.input,
            output_tokens: self.output,
            estimated: self.estimated,
        }
    }
}

enum UsageError {
    Missing,
    Overflow,
}

fn known_or_estimate(
    count: TokenCount,
    policy: MissingUsagePolicy,
    input: bool,
) -> Result<(u64, bool), UsageError> {
    match (count, policy) {
        (TokenCount::Known(value), _) => Ok((value, false)),
        (TokenCount::Unknown, MissingUsagePolicy::Stop) => Err(UsageError::Missing),
        (
            TokenCount::Unknown,
            MissingUsagePolicy::Estimate {
                input_tokens,
                output_tokens,
            },
        ) => Ok((if input { input_tokens } else { output_tokens }, true)),
    }
}

pub(super) fn account_or_limit(
    usage: &mut UsageAccumulator,
    model_usage: garive_llm::ModelUsage,
    policy: ModelRecoveryPolicy,
    maximum: Option<u64>,
) -> Option<AgentOutcome> {
    if usage.add(model_usage, policy.missing_usage).is_err() {
        return Some(AgentOutcome::Stopped {
            reason: StopReason::TokenLimit,
        });
    }
    let Some(total) = usage.total() else {
        return Some(AgentOutcome::Failed {
            reason: AgentFailureReason::InvariantViolation,
        });
    };
    maximum
        .is_some_and(|limit| total > limit)
        .then_some(AgentOutcome::Stopped {
            reason: StopReason::TokenLimit,
        })
}

pub(super) fn finish_recovery(
    request: &AgentTurnRequest,
    ports: &mut AgentExecutionPorts<'_>,
    control: &mut ExecutionControl,
    action: TerminalRecoveryAction,
) -> ExecutionReport {
    let outcome = match action {
        TerminalRecoveryAction::Suspend | TerminalRecoveryAction::AlternateThenSuspend => {
            AgentOutcome::Suspended {
                reason: SuspensionReason::ResourceUnavailable,
                partial_items: vec![],
                last_durable_position: request.cursor.last_durable_position,
            }
        }
        TerminalRecoveryAction::Stop => AgentOutcome::Stopped {
            reason: StopReason::ResourceUnavailable,
        },
        TerminalRecoveryAction::Fail => AgentOutcome::Failed {
            reason: AgentFailureReason::PortFailure,
        },
    };
    finish(request, ports, control, outcome)
}

pub(super) fn deadline_reached(
    request: &AgentTurnRequest,
    ports: &AgentExecutionPorts<'_>,
) -> Result<bool, ()> {
    request.limits.deadline_tick.map_or(Ok(false), |deadline| {
        ports
            .clock
            .now_tick()
            .map(|now| now >= deadline)
            .map_err(|_| ())
    })
}

pub(super) fn emit(
    ports: &mut AgentExecutionPorts<'_>,
    request: &AgentTurnRequest,
    kind: AgentEventKind,
) -> Result<(), ()> {
    ports
        .events
        .emit(AgentEvent {
            session_id: request.session_id.clone(),
            turn_id: request.turn_id.clone(),
            execution_id: request.execution_id.clone(),
            kind,
        })
        .map_err(|_| ())
}

pub(super) fn finish(
    request: &AgentTurnRequest,
    ports: &mut AgentExecutionPorts<'_>,
    control: &mut ExecutionControl,
    mut outcome: AgentOutcome,
) -> ExecutionReport {
    if emit(ports, request, AgentEventKind::OutcomeProposed).is_err() {
        outcome = AgentOutcome::Failed {
            reason: AgentFailureReason::PortFailure,
        };
    }
    if control.status() == ExecutionStatus::Active {
        let kind = match outcome {
            AgentOutcome::Completed { .. } => ExecutionOutcomeKind::Completed,
            AgentOutcome::Suspended { .. } => ExecutionOutcomeKind::Suspended,
            AgentOutcome::Stopped { .. } => ExecutionOutcomeKind::Stopped,
            AgentOutcome::Failed { .. } => ExecutionOutcomeKind::Failed,
        };
        if control.close(kind).is_err() {
            outcome = AgentOutcome::Failed {
                reason: AgentFailureReason::InvariantViolation,
            };
        }
    }
    ExecutionReport {
        outcome,
        completed_iterations: control.completed_iterations(),
    }
}

pub(super) struct ForwardObserver<'a> {
    request: &'a AgentTurnRequest,
    events: &'a mut dyn EventSink,
    cancellation: &'a dyn ModelCancellation,
    failed: bool,
}

impl<'a> ForwardObserver<'a> {
    pub(super) fn new(
        request: &'a AgentTurnRequest,
        events: &'a mut dyn EventSink,
        cancellation: &'a dyn ModelCancellation,
    ) -> Self {
        Self {
            request,
            events,
            cancellation,
            failed: false,
        }
    }

    pub(super) const fn failed(&self) -> bool {
        self.failed
    }
}

impl ModelObserver for ForwardObserver<'_> {
    fn observe(&mut self, event: &ModelStreamEvent) -> ObserverDecision {
        if self.cancellation.is_cancelled() {
            return ObserverDecision::Cancel;
        }
        if self
            .events
            .emit(AgentEvent {
                session_id: self.request.session_id.clone(),
                turn_id: self.request.turn_id.clone(),
                execution_id: self.request.execution_id.clone(),
                kind: AgentEventKind::ModelStream(event.clone()),
            })
            .is_err()
        {
            self.failed = true;
            ObserverDecision::Cancel
        } else {
            ObserverDecision::Continue
        }
    }
}
