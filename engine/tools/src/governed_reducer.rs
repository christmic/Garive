//! Deterministic C5 authority, interaction, execution, and recovery reduction.

use crate::schema::{validate_arguments, validate_value_definition};
use crate::{
    EffectReceipt, EffectState, ExecutionFact, GovernedAction, GovernedEffectFailure,
    GovernedFailureCode, GovernedObservation, InteractionRequest, InteractionResolution,
    InvocationGrant, ObservationOutcome, PreparedToolCall, RecoveryDecision, RecoveryPosition,
    ReplayClass, SuspensionRequirement, TerminalClassification, ToolInvocationId,
};

/// Durable authorization verdict for the exact invocation and Prepared Call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorizationVerdict {
    /// Exact authority grant.
    Approve(InvocationGrant),
    /// Stable safe denial.
    Deny {
        /// Stable denial code.
        code: String,
        /// Optional redacted detail.
        details: Option<String>,
    },
    /// Original invocation terminates without executable authority.
    ReplacementRequired,
    /// A committed interaction must suspend the current Execution.
    InteractionRequired(InteractionRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReducerState {
    Prepared,
    AwaitingInteraction(InteractionRequest),
    Authorized(InvocationGrant),
    Started(InvocationGrant),
    Denied,
    Replaced,
    Completed,
    Failed,
    Uncertain,
}

/// One sequential governed invocation reducer carrying no Runtime I/O.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernedEffect {
    invocation_id: ToolInvocationId,
    prepared: PreparedToolCall,
    state: ReducerState,
    last_interaction_resolution: Option<InteractionResolution>,
}

impl GovernedEffect {
    /// Starts at Prepared and asks Runtime to authorize the exact call.
    pub fn new(
        invocation_id: ToolInvocationId,
        prepared: PreparedToolCall,
    ) -> (Self, GovernedAction) {
        (
            Self {
                invocation_id,
                prepared,
                state: ReducerState::Prepared,
                last_interaction_resolution: None,
            },
            GovernedAction::Authorize,
        )
    }

    /// Returns the portable lifecycle state.
    pub const fn state(&self) -> EffectState {
        match self.state {
            ReducerState::Prepared => EffectState::Prepared,
            ReducerState::AwaitingInteraction(_) => EffectState::AwaitingInteraction,
            ReducerState::Authorized(_) => EffectState::Authorized,
            ReducerState::Started(_) => EffectState::Started,
            ReducerState::Denied => EffectState::Denied,
            ReducerState::Replaced => EffectState::Replaced,
            ReducerState::Completed => EffectState::Completed,
            ReducerState::Failed => EffectState::Failed,
            ReducerState::Uncertain => EffectState::Uncertain,
        }
    }

    /// Reduces one durably committed authorization verdict.
    pub fn apply_authorization(&mut self, verdict: AuthorizationVerdict) -> GovernedAction {
        if let (ReducerState::Authorized(existing), AuthorizationVerdict::Approve(candidate)) =
            (&self.state, &verdict)
        {
            return if existing == candidate {
                GovernedAction::None
            } else {
                self.fail(GovernedFailureCode::InvocationConflict)
            };
        }
        if self.state != ReducerState::Prepared {
            return self.fail(GovernedFailureCode::InvocationConflict);
        }
        match verdict {
            AuthorizationVerdict::Approve(grant) => {
                if !self.grant_binds(&grant) {
                    return self.fail(GovernedFailureCode::GrantMismatch);
                }
                self.state = ReducerState::Authorized(grant.clone());
                GovernedAction::Dispatch(grant)
            }
            AuthorizationVerdict::Deny { code, details } => {
                if code.is_empty() {
                    return self.fail(GovernedFailureCode::InvocationConflict);
                }
                self.state = ReducerState::Denied;
                GovernedAction::Observation(
                    self.observation(ObservationOutcome::Rejected { code, details }),
                )
            }
            AuthorizationVerdict::ReplacementRequired => {
                self.state = ReducerState::Replaced;
                GovernedAction::Observation(self.observation(ObservationOutcome::Rejected {
                    code: "replacement_required".to_owned(),
                    details: None,
                }))
            }
            AuthorizationVerdict::InteractionRequired(request) => {
                if request.validate().is_err()
                    || validate_value_definition(&request.response_schema).is_err()
                    || request.invocation_id != self.invocation_id
                    || request.prepared_digest != self.prepared.input_digest()
                {
                    return self.fail(GovernedFailureCode::InteractionConflict);
                }
                self.state = ReducerState::AwaitingInteraction(request.clone());
                self.last_interaction_resolution = None;
                GovernedAction::Suspend(SuspensionRequirement::Interaction(request))
            }
        }
    }

    /// Reduces a committed interaction continuation without inventing authority.
    pub fn apply_interaction(&mut self, resolution: InteractionResolution) -> GovernedAction {
        if let Some(existing) = &self.last_interaction_resolution {
            return if existing == &resolution {
                GovernedAction::None
            } else {
                self.fail(GovernedFailureCode::InteractionConflict)
            };
        }
        let ReducerState::AwaitingInteraction(request) = &self.state else {
            return self.fail(GovernedFailureCode::InteractionConflict);
        };
        let matches = match &resolution {
            InteractionResolution::Resolved {
                interaction_id,
                invocation_id,
                prepared_digest,
                response,
            } => {
                interaction_id == &request.interaction_id
                    && invocation_id == &request.invocation_id
                    && prepared_digest == &request.prepared_digest
                    && validate_arguments(&request.response_schema, response).is_empty()
            }
            InteractionResolution::Cancelled {
                interaction_id,
                invocation_id,
                prepared_digest,
            } => {
                interaction_id == &request.interaction_id
                    && invocation_id == &request.invocation_id
                    && prepared_digest == &request.prepared_digest
            }
        };
        if !matches {
            return self.fail(GovernedFailureCode::InteractionConflict);
        }
        match resolution {
            InteractionResolution::Resolved { .. } => {
                self.last_interaction_resolution = Some(resolution);
                self.state = ReducerState::Prepared;
                GovernedAction::Authorize
            }
            InteractionResolution::Cancelled { .. } => {
                self.last_interaction_resolution = Some(resolution);
                self.state = ReducerState::Denied;
                GovernedAction::Observation(self.observation(ObservationOutcome::Rejected {
                    code: "interaction_cancelled".to_owned(),
                    details: None,
                }))
            }
        }
    }

    /// Reduces one Runtime execution fact after its required durable commit.
    pub fn apply_execution(&mut self, fact: ExecutionFact) -> GovernedAction {
        match (&self.state, fact) {
            (ReducerState::Authorized(grant), ExecutionFact::Started(_)) => {
                self.state = ReducerState::Started(grant.clone());
                GovernedAction::None
            }
            (ReducerState::Authorized(_), ExecutionFact::Unsupported { requirement }) => {
                if requirement.is_empty() {
                    return self.fail(GovernedFailureCode::CorruptRecoveryState);
                }
                self.fail(GovernedFailureCode::RequirementUnsupported)
            }
            (
                ReducerState::Started(grant),
                ExecutionFact::Completed {
                    receipt,
                    content,
                    truncated,
                },
            ) => {
                let Some(receipt) = receipt else {
                    return self.fail(GovernedFailureCode::CorruptRecoveryState);
                };
                if !self.receipt_binds(&receipt, grant, TerminalClassification::Completed)
                    || serde_json::to_vec(&content).map_or(true, |bytes| {
                        bytes.len() as u64 > grant.granted_requirements.max_output_bytes()
                    })
                {
                    return self.fail(GovernedFailureCode::CorruptRecoveryState);
                }
                self.state = ReducerState::Completed;
                GovernedAction::Observation(
                    self.observation(ObservationOutcome::Succeeded { content, truncated }),
                )
            }
            (
                ReducerState::Started(grant),
                ExecutionFact::Failed {
                    receipt,
                    code,
                    details,
                    partial,
                },
            ) => {
                let Some(receipt) = receipt else {
                    return self.fail(GovernedFailureCode::CorruptRecoveryState);
                };
                if code.is_empty()
                    || !self.receipt_binds(&receipt, grant, TerminalClassification::Failed)
                    || partial.as_ref().is_some_and(|value| {
                        serde_json::to_vec(value).map_or(true, |bytes| {
                            bytes.len() as u64 > grant.granted_requirements.max_output_bytes()
                        })
                    })
                {
                    return self.fail(GovernedFailureCode::CorruptRecoveryState);
                }
                self.state = ReducerState::Failed;
                GovernedAction::Observation(self.observation(ObservationOutcome::Failed {
                    code,
                    details,
                    partial,
                }))
            }
            (ReducerState::Started(_), ExecutionFact::Uncertain { evidence }) => {
                if evidence.is_empty() {
                    return self.fail(GovernedFailureCode::CorruptRecoveryState);
                }
                self.state = ReducerState::Uncertain;
                GovernedAction::Suspend(SuspensionRequirement::OperatorReconciliation { evidence })
            }
            _ => self.fail(GovernedFailureCode::InvocationConflict),
        }
    }

    fn grant_binds(&self, grant: &InvocationGrant) -> bool {
        grant.invocation_id == self.invocation_id
            && grant.prepared_digest == self.prepared.input_digest()
            && grant.tool_name == self.prepared.tool_name()
            && grant.tool_revision == self.prepared.tool_revision()
            && grant.granted_requirements.max_duration_ms()
                <= self.prepared.requirements().max_duration_ms()
            && grant.granted_requirements.max_output_bytes()
                <= self.prepared.requirements().max_output_bytes()
            && grant.granted_requirements.capabilities().all(|capability| {
                self.prepared
                    .requirements()
                    .capabilities()
                    .any(|requested| requested == capability)
            })
    }

    fn receipt_binds(
        &self,
        receipt: &EffectReceipt,
        grant: &InvocationGrant,
        terminal: TerminalClassification,
    ) -> bool {
        receipt.validate().is_ok()
            && receipt.invocation_id == self.invocation_id
            && receipt.prepared_digest == self.prepared.input_digest()
            && receipt.grant_id == grant.grant_id
            && receipt.terminal_classification == terminal
    }

    fn observation(&self, outcome: ObservationOutcome) -> GovernedObservation {
        GovernedObservation {
            invocation_id: self.invocation_id.clone(),
            prepared_digest: self.prepared.input_digest().to_owned(),
            model_call_id: self.prepared.model_call_id().to_owned(),
            tool_name: self.prepared.tool_name().to_owned(),
            outcome,
        }
    }

    fn fail(&mut self, code: GovernedFailureCode) -> GovernedAction {
        self.state = ReducerState::Failed;
        GovernedAction::Fail(GovernedEffectFailure { code })
    }
}

/// Selects recovery without treating a replay declaration as executor proof.
pub const fn recover_effect(
    position: RecoveryPosition,
    replay_class: ReplayClass,
    executor_proves_replay: bool,
) -> RecoveryDecision {
    match position {
        RecoveryPosition::Authorized => RecoveryDecision::RevalidateGrant,
        RecoveryPosition::ReceiptNoResult => RecoveryDecision::ReconstructFromReceipt,
        RecoveryPosition::Terminal => RecoveryDecision::ReturnTerminal,
        RecoveryPosition::StartedNoReceipt => match replay_class {
            ReplayClass::ReceiptRecoverable if executor_proves_replay => {
                RecoveryDecision::RecoverExecutorReceipt
            }
            ReplayClass::ReadOnly | ReplayClass::Idempotent if executor_proves_replay => {
                RecoveryDecision::RetrySameInvocation
            }
            _ => RecoveryDecision::ReconcileOperator,
        },
    }
}
