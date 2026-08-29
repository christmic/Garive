use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelCancellation, ModelItem, ModelPortFailure, RejectionKind,
    ToolDescriptor,
};
use garive_tools::{
    GovernedFailureCode, GovernedToolResult, InteractionKind, SuspensionRequirement, ToolCatalog,
    ToolDefinition, ToolIntent,
};

use crate::{
    AgentEventKind, AgentExecutionPorts, AgentFailureReason, AgentOutcome, AgentTurnRequest,
    BeginIteration, ExecutionReport, OutputLimitAction, StopReason, SuspensionReason,
    TerminalRecoveryAction,
};

use crate::model_only_support::{
    account_or_limit, build_model_request, deadline_reached, emit, finish, finish_recovery,
    prepare_control, ForwardObserver, UsageAccumulator,
};

/// Runs one bounded model-only kernel execution against frozen ports.
///
/// The driver validates the immutable request, checks cancellation and limits at
/// defined boundaries, forwards normalized model events, and returns exactly one
/// terminal proposal. It never persists state or executes tool intents.
pub async fn execute_model_only(
    request: &AgentTurnRequest,
    ports: &mut AgentExecutionPorts<'_>,
) -> ExecutionReport {
    execute_kernel(request, ports, &[], None).await
}

pub(crate) async fn execute_with_tools(
    request: &AgentTurnRequest,
    ports: &mut AgentExecutionPorts<'_>,
    definitions: &[ToolDefinition],
    effects: &mut dyn crate::GovernedEffectPort,
) -> ExecutionReport {
    execute_kernel(request, ports, definitions, Some(effects)).await
}

async fn execute_kernel(
    request: &AgentTurnRequest,
    ports: &mut AgentExecutionPorts<'_>,
    definitions: &[ToolDefinition],
    mut effects: Option<&mut dyn crate::GovernedEffectPort>,
) -> ExecutionReport {
    let mut control = match prepare_control(request) {
        Ok(control) => control,
        Err(report) => return *report,
    };
    let mut usage = UsageAccumulator::default();
    let mut rebuild_attempt = 0;
    let mut output_retries = 0;
    let mut target_index = 0;
    let mut request_ordinal = 0u32;
    let mut through_position = request.context_request.through_position;
    let catalog = match ToolCatalog::new(definitions.iter().cloned()) {
        Ok(value) => value,
        Err(_) => return invalid_tool_setup(request, ports, &mut control, &usage),
    };
    let tool_descriptors: Vec<ToolDescriptor> =
        match definitions.iter().map(tool_descriptor).collect() {
            Ok(value) => value,
            Err(()) => return invalid_tool_setup(request, ports, &mut control, &usage),
        };

    if emit(ports, request, AgentEventKind::ExecutionStarted).is_err() {
        return finish(
            request,
            ports,
            &mut control,
            &usage,
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
                &usage,
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
                    &usage,
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
                    &usage,
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
                    &usage,
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
                    &usage,
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
                &usage,
                AgentOutcome::Failed {
                    reason: AgentFailureReason::PortFailure,
                },
            );
        }
        let mut context_request = request.context_request.clone();
        context_request.through_position = through_position;
        let surface = match ports.context.derive(&context_request, rebuild_attempt) {
            Ok(surface) => surface,
            Err(crate::ContextPortError::RequiredFactsExceedBudget) => {
                return finish(
                    request,
                    ports,
                    &mut control,
                    &usage,
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
                    &usage,
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
                &usage,
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
                &usage,
                AgentOutcome::Stopped {
                    reason: StopReason::Cancelled,
                },
            );
        }
        request_ordinal = match request_ordinal.checked_add(1) {
            Some(value) => value,
            None => {
                return finish(
                    request,
                    ports,
                    &mut control,
                    &usage,
                    AgentOutcome::Failed {
                        reason: AgentFailureReason::InvariantViolation,
                    },
                );
            }
        };
        let (model_request, request_id) = match build_model_request(
            request,
            surface,
            iteration,
            request_ordinal,
            target_index,
            tool_descriptors.clone(),
        ) {
            Ok(value) => value,
            Err(()) => {
                return finish(
                    request,
                    ports,
                    &mut control,
                    &usage,
                    AgentOutcome::Failed {
                        reason: AgentFailureReason::InvalidInput,
                    },
                );
            }
        };
        if let Err(failure) = ports.model.preflight(&model_request) {
            return finish(
                request,
                ports,
                &mut control,
                &usage,
                AgentOutcome::Failed {
                    reason: model_failure(failure),
                },
            );
        }
        if emit(
            ports,
            request,
            AgentEventKind::ModelRequestPrepared {
                request_id: request_id.clone(),
                target_id: model_request.target_id.as_str().into(),
            },
        )
        .is_err()
        {
            return finish(
                request,
                ports,
                &mut control,
                &usage,
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
                &usage,
                AgentOutcome::Failed {
                    reason: AgentFailureReason::PortFailure,
                },
            );
        }
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(failure) => {
                return finish(
                    request,
                    ports,
                    &mut control,
                    &usage,
                    AgentOutcome::Failed {
                        reason: model_failure(failure),
                    },
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
                    return finish(request, ports, &mut control, &usage, terminal);
                }
                if items
                    .iter()
                    .any(|item| matches!(item, ModelItem::ToolObservation { .. }))
                {
                    return finish(
                        request,
                        ports,
                        &mut control,
                        &usage,
                        AgentOutcome::Failed {
                            reason: AgentFailureReason::InvalidModelOutput,
                        },
                    );
                }
                if items
                    .iter()
                    .any(|item| matches!(item, ModelItem::ToolIntent { .. }))
                {
                    let Some(effects) = effects.as_deref_mut() else {
                        return finish(
                            request,
                            ports,
                            &mut control,
                            &usage,
                            AgentOutcome::Failed {
                                reason: AgentFailureReason::RequiredCapabilityUnavailable,
                            },
                        );
                    };
                    match govern_tool_intents(
                        &items,
                        &catalog,
                        effects,
                        &request_id,
                        ports.cancellation,
                        through_position,
                    )
                    .await
                    {
                        ToolStep::Continue { position } => {
                            through_position = position;
                            continue;
                        }
                        ToolStep::Terminal(outcome) => {
                            return finish(request, ports, &mut control, &usage, outcome);
                        }
                    }
                }
                return finish(
                    request,
                    ports,
                    &mut control,
                    &usage,
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
                    &usage,
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
                    return finish(request, ports, &mut control, &usage, terminal);
                }
                match kind {
                    InterruptionKind::Cancelled => {
                        return finish(
                            request,
                            ports,
                            &mut control,
                            &usage,
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
                                    &usage,
                                    AgentOutcome::Failed {
                                        reason: AgentFailureReason::RequiredCapabilityUnavailable,
                                    },
                                );
                            }
                            return finish(
                                request,
                                ports,
                                &mut control,
                                &usage,
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
                                &usage,
                                AgentOutcome::Suspended {
                                    reason: SuspensionReason::PartialOutput,
                                    partial_items,
                                    last_durable_position: through_position,
                                    governed_binding: None,
                                },
                            );
                        }
                        OutputLimitAction::Stop => {
                            return finish(
                                request,
                                ports,
                                &mut control,
                                &usage,
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
                                &usage,
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
                            &usage,
                            request.recovery_policy.transport,
                            through_position,
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
                    &usage,
                    request.recovery_policy.unavailable,
                    through_position,
                );
            }
        }
    }
}

const fn model_failure(failure: ModelPortFailure) -> AgentFailureReason {
    match failure {
        ModelPortFailure::InvalidRequest => AgentFailureReason::InvalidInput,
        ModelPortFailure::UnsupportedCapability => {
            AgentFailureReason::RequiredCapabilityUnavailable
        }
        ModelPortFailure::AdapterInvariant => AgentFailureReason::InvalidModelOutput,
        ModelPortFailure::RequiredPortFailure => AgentFailureReason::PortFailure,
    }
}

enum ToolStep {
    Continue { position: u64 },
    Terminal(AgentOutcome),
}

async fn govern_tool_intents(
    items: &[ModelItem],
    catalog: &ToolCatalog,
    effects: &mut dyn crate::GovernedEffectPort,
    source_model_request_id: &str,
    cancellation: &dyn ModelCancellation,
    mut position: u64,
) -> ToolStep {
    for item in items {
        let ModelItem::ToolIntent {
            model_call_id,
            tool_name,
            arguments_json,
        } = item
        else {
            continue;
        };
        if cancellation.is_cancelled() {
            return ToolStep::Terminal(AgentOutcome::Stopped {
                reason: StopReason::Cancelled,
            });
        }
        let intent = ToolIntent::new(model_call_id, tool_name, arguments_json);
        let committed = match catalog.prepare(&intent) {
            Ok(prepared) => effects.invoke(source_model_request_id, &prepared).await,
            Err(error) => {
                effects
                    .reject(source_model_request_id, &intent, &error)
                    .await
            }
        };
        let Ok(committed) = committed else {
            return ToolStep::Terminal(AgentOutcome::Failed {
                reason: AgentFailureReason::PortFailure,
            });
        };
        if committed.through_position < position {
            return ToolStep::Terminal(AgentOutcome::Failed {
                reason: AgentFailureReason::InvariantViolation,
            });
        }
        position = committed.through_position;
        match committed.result {
            GovernedToolResult::Observation(_) => {}
            GovernedToolResult::Suspend(requirement) => {
                let Some(binding) = committed.suspension_binding else {
                    return ToolStep::Terminal(AgentOutcome::Failed {
                        reason: AgentFailureReason::InvariantViolation,
                    });
                };
                if !suspension_binds(&requirement, &binding) {
                    return ToolStep::Terminal(AgentOutcome::Failed {
                        reason: AgentFailureReason::InvariantViolation,
                    });
                }
                let reason = match requirement {
                    SuspensionRequirement::Interaction(request) => match request.kind {
                        InteractionKind::Approval => SuspensionReason::ApprovalRequired,
                        InteractionKind::ExternalInput => SuspensionReason::ExternalInputRequired,
                    },
                    SuspensionRequirement::OperatorReconciliation { .. } => {
                        SuspensionReason::OperatorReconciliation
                    }
                };
                return ToolStep::Terminal(AgentOutcome::Suspended {
                    reason,
                    partial_items: items.to_vec(),
                    last_durable_position: position,
                    governed_binding: Some(binding),
                });
            }
            GovernedToolResult::Fail(failure) => {
                let reason = match failure.code {
                    GovernedFailureCode::InvalidModelOutput => {
                        AgentFailureReason::InvalidModelOutput
                    }
                    _ => AgentFailureReason::InvariantViolation,
                };
                return ToolStep::Terminal(AgentOutcome::Failed { reason });
            }
        }
    }
    ToolStep::Continue { position }
}

fn suspension_binds(
    requirement: &SuspensionRequirement,
    binding: &crate::GovernedSuspensionBinding,
) -> bool {
    match (requirement, binding) {
        (
            SuspensionRequirement::Interaction(request),
            crate::GovernedSuspensionBinding::Interaction {
                suspension_id,
                interaction_id,
                invocation_id,
                prepared_digest,
            },
        ) => {
            !suspension_id.is_empty()
                && interaction_id == request.interaction_id.as_str()
                && invocation_id == request.invocation_id.as_str()
                && prepared_digest == &request.prepared_digest
        }
        (
            SuspensionRequirement::OperatorReconciliation { .. },
            crate::GovernedSuspensionBinding::OperatorReconciliation {
                suspension_id,
                invocation_id,
                prepared_digest,
            },
        ) => !suspension_id.is_empty() && !invocation_id.is_empty() && !prepared_digest.is_empty(),
        _ => false,
    }
}

fn tool_descriptor(definition: &ToolDefinition) -> Result<ToolDescriptor, ()> {
    Ok(ToolDescriptor {
        name: definition.name().to_owned(),
        description: definition.description().to_owned(),
        definition_revision: definition.revision().to_owned(),
        input_schema_json: serde_json::to_string(definition.input_schema()).map_err(|_| ())?,
        strict: true,
    })
}

fn invalid_tool_setup(
    request: &AgentTurnRequest,
    ports: &mut AgentExecutionPorts<'_>,
    control: &mut crate::ExecutionControl,
    usage: &UsageAccumulator,
) -> ExecutionReport {
    finish(
        request,
        ports,
        control,
        usage,
        AgentOutcome::Failed {
            reason: AgentFailureReason::InvalidInput,
        },
    )
}
