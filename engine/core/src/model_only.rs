use garive_llm::{InterruptionKind, InvokeOutcome, ModelItem, ModelPortFailure, RejectionKind};

use crate::{
    AgentEventKind, AgentExecutionPorts, AgentFailureReason, AgentOutcome, AgentTurnRequest,
    BeginIteration, ExecutionReport, OutputLimitAction, StopReason, SuspensionReason,
    TerminalRecoveryAction,
};

use crate::model_only_support::{
    account_or_limit, build_model_request, deadline_reached, emit, finish, finish_recovery,
    prepare_control, ForwardObserver, UsageAccumulator,
};

pub async fn execute_model_only(
    request: &AgentTurnRequest,
    ports: &mut AgentExecutionPorts<'_>,
) -> ExecutionReport {
    let mut control = match prepare_control(request) {
        Ok(control) => control,
        Err(report) => return report,
    };
    let mut usage = UsageAccumulator::default();
    let mut rebuild_attempt = 0;
    let mut output_retries = 0;
    let mut target_index = 0;

    if emit(ports, request, AgentEventKind::ExecutionStarted).is_err() {
        return finish(
            request,
            ports,
            &mut control,
            AgentOutcome::Failed {
                reason: AgentFailureReason::PortFailure,
            },
        );
    }

    loop {
        if ports.cancellation.is_cancelled() {
            return finish(
                request,
                ports,
                &mut control,
                AgentOutcome::Stopped {
                    reason: StopReason::Cancelled,
                },
            );
        }
        match deadline_reached(request, ports) {
            Ok(true) => {
                return finish(
                    request,
                    ports,
                    &mut control,
                    AgentOutcome::Stopped {
                        reason: StopReason::Deadline,
                    },
                );
            }
            Err(()) => {
                return finish(
                    request,
                    ports,
                    &mut control,
                    AgentOutcome::Failed {
                        reason: AgentFailureReason::PortFailure,
                    },
                );
            }
            Ok(false) => {}
        }
        let iteration = match control.begin_iteration() {
            Ok(BeginIteration::Started { iteration }) => iteration.get(),
            Ok(BeginIteration::IterationLimitReached) => {
                return finish(
                    request,
                    ports,
                    &mut control,
                    AgentOutcome::Stopped {
                        reason: StopReason::IterationLimit,
                    },
                );
            }
            Err(_) => {
                return finish(
                    request,
                    ports,
                    &mut control,
                    AgentOutcome::Failed {
                        reason: AgentFailureReason::InvariantViolation,
                    },
                );
            }
        };
        if emit(
            ports,
            request,
            AgentEventKind::IterationStarted { iteration },
        )
        .is_err()
        {
            return finish(
                request,
                ports,
                &mut control,
                AgentOutcome::Failed {
                    reason: AgentFailureReason::PortFailure,
                },
            );
        }
        let surface = match ports
            .context
            .derive(&request.context_request, rebuild_attempt)
        {
            Ok(surface) => surface,
            Err(crate::ContextPortError::RequiredFactsExceedBudget) => {
                return finish(
                    request,
                    ports,
                    &mut control,
                    AgentOutcome::Stopped {
                        reason: StopReason::TokenLimit,
                    },
                );
            }
            Err(crate::ContextPortError::PortFailure) => {
                return finish(
                    request,
                    ports,
                    &mut control,
                    AgentOutcome::Failed {
                        reason: AgentFailureReason::PortFailure,
                    },
                );
            }
        };
        if emit(
            ports,
            request,
            AgentEventKind::ContextDerived {
                item_count: surface.item_count,
                utf8_bytes: surface.utf8_bytes,
            },
        )
        .is_err()
        {
            return finish(
                request,
                ports,
                &mut control,
                AgentOutcome::Failed {
                    reason: AgentFailureReason::PortFailure,
                },
            );
        }
        if ports.cancellation.is_cancelled() {
            return finish(
                request,
                ports,
                &mut control,
                AgentOutcome::Stopped {
                    reason: StopReason::Cancelled,
                },
            );
        }
        let (model_request, request_id) =
            match build_model_request(request, surface, iteration, target_index) {
                Ok(value) => value,
                Err(()) => {
                    return finish(
                        request,
                        ports,
                        &mut control,
                        AgentOutcome::Failed {
                            reason: AgentFailureReason::InvalidInput,
                        },
                    );
                }
            };
        if emit(
            ports,
            request,
            AgentEventKind::ModelRequestPrepared {
                request_id,
                target_id: model_request.target_id.as_str().into(),
            },
        )
        .is_err()
        {
            return finish(
                request,
                ports,
                &mut control,
                AgentOutcome::Failed {
                    reason: AgentFailureReason::PortFailure,
                },
            );
        }

        let (result, observer_failed) = {
            let mut observer = ForwardObserver::new(request, ports.events, ports.cancellation);
            let result = ports
                .model
                .invoke(&model_request, &mut observer, ports.cancellation)
                .await;
            (result, observer.failed())
        };
        if observer_failed {
            return finish(
                request,
                ports,
                &mut control,
                AgentOutcome::Failed {
                    reason: AgentFailureReason::PortFailure,
                },
            );
        }
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(failure) => {
                let reason = match failure {
                    ModelPortFailure::InvalidRequest => AgentFailureReason::InvalidInput,
                    ModelPortFailure::UnsupportedCapability => {
                        AgentFailureReason::RequiredCapabilityUnavailable
                    }
                    ModelPortFailure::AdapterInvariant => AgentFailureReason::InvalidModelOutput,
                    ModelPortFailure::RequiredPortFailure => AgentFailureReason::PortFailure,
                };
                return finish(
                    request,
                    ports,
                    &mut control,
                    AgentOutcome::Failed { reason },
                );
            }
        };

        match outcome {
            InvokeOutcome::Completed {
                items,
                usage: model_usage,
                ..
            } => {
                if let Some(terminal) = account_or_limit(
                    &mut usage,
                    model_usage,
                    request.recovery_policy,
                    request.limits.max_total_tokens,
                ) {
                    return finish(request, ports, &mut control, terminal);
                }
                if items
                    .iter()
                    .any(|item| matches!(item, ModelItem::ToolIntent { .. }))
                {
                    return finish(
                        request,
                        ports,
                        &mut control,
                        AgentOutcome::Failed {
                            reason: AgentFailureReason::RequiredCapabilityUnavailable,
                        },
                    );
                }
                if items
                    .iter()
                    .any(|item| matches!(item, ModelItem::ToolObservation { .. }))
                {
                    return finish(
                        request,
                        ports,
                        &mut control,
                        AgentOutcome::Failed {
                            reason: AgentFailureReason::InvalidModelOutput,
                        },
                    );
                }
                return finish(
                    request,
                    ports,
                    &mut control,
                    AgentOutcome::Completed {
                        response_items: items,
                        usage: usage.summary(),
                    },
                );
            }
            InvokeOutcome::Rejected {
                kind: RejectionKind::ContextOverflow,
                ..
            } if rebuild_attempt < request.recovery_policy.max_context_rebuilds => {
                rebuild_attempt += 1;
            }
            InvokeOutcome::Rejected { .. } => {
                return finish(
                    request,
                    ports,
                    &mut control,
                    AgentOutcome::Failed {
                        reason: AgentFailureReason::InvalidModelOutput,
                    },
                );
            }
            InvokeOutcome::Interrupted {
                kind,
                partial_items,
                usage: model_usage,
            } => {
                if let Some(terminal) = account_or_limit(
                    &mut usage,
                    model_usage,
                    request.recovery_policy,
                    request.limits.max_total_tokens,
                ) {
                    return finish(request, ports, &mut control, terminal);
                }
                match kind {
                    InterruptionKind::Cancelled => {
                        return finish(
                            request,
                            ports,
                            &mut control,
                            AgentOutcome::Stopped {
                                reason: StopReason::Cancelled,
                            },
                        );
                    }
                    InterruptionKind::OutputLimit => match request.recovery_policy.output_limit {
                        OutputLimitAction::CompletePartial => {
                            if partial_items
                                .iter()
                                .any(|item| matches!(item, ModelItem::ToolIntent { .. }))
                            {
                                return finish(
                                    request,
                                    ports,
                                    &mut control,
                                    AgentOutcome::Failed {
                                        reason: AgentFailureReason::RequiredCapabilityUnavailable,
                                    },
                                );
                            }
                            return finish(
                                request,
                                ports,
                                &mut control,
                                AgentOutcome::Completed {
                                    response_items: partial_items,
                                    usage: usage.summary(),
                                },
                            );
                        }
                        OutputLimitAction::Retry { max_retries }
                            if output_retries < max_retries =>
                        {
                            output_retries += 1;
                        }
                        OutputLimitAction::Suspend | OutputLimitAction::Retry { .. } => {
                            return finish(
                                request,
                                ports,
                                &mut control,
                                AgentOutcome::Suspended {
                                    reason: SuspensionReason::PartialOutput,
                                    partial_items,
                                    last_durable_position: request.cursor.last_durable_position,
                                },
                            );
                        }
                        OutputLimitAction::Stop => {
                            return finish(
                                request,
                                ports,
                                &mut control,
                                AgentOutcome::Stopped {
                                    reason: StopReason::TokenLimit,
                                },
                            );
                        }
                        OutputLimitAction::Fail => {
                            return finish(
                                request,
                                ports,
                                &mut control,
                                AgentOutcome::Failed {
                                    reason: AgentFailureReason::InvalidModelOutput,
                                },
                            );
                        }
                    },
                    InterruptionKind::Transport => {
                        return finish_recovery(
                            request,
                            ports,
                            &mut control,
                            request.recovery_policy.transport,
                        );
                    }
                }
            }
            InvokeOutcome::Unavailable { .. } => {
                if request.recovery_policy.unavailable
                    == TerminalRecoveryAction::AlternateThenSuspend
                    && target_index + 1 < request.model_targets.len()
                {
                    target_index += 1;
                    continue;
                }
                return finish_recovery(
                    request,
                    ports,
                    &mut control,
                    request.recovery_policy.unavailable,
                );
            }
        }
    }
}
