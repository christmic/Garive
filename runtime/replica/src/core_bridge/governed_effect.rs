use std::sync::Mutex;

use garive_core::{
    CommittedGovernedResult, GovernedEffectFuture, GovernedEffectPort, GovernedSuspensionBinding,
    PortFailure,
};
use garive_ledger::{
    CanonicalPayload, FactDraft, FactId, FactKind, ModelRequestId, ToolInvocationId as LedgerToolId,
};
use garive_tools::{
    reduce_preparation_failure, AuthorizationVerdict, DispatchAttemptId, EffectReceipt,
    ExecutionFact, GovernedAction, GovernedEffect, GovernedEffectFailure, GovernedFailureCode,
    GovernedObservation, GovernedToolResult, GrantId, InteractionId, InteractionRequest,
    InvocationGrant, PreparationError, PreparationErrorCode, PreparedToolCall, ReplayClass,
    SuspensionRequirement, TerminalClassification, ToolFeedback, ToolIntent, ToolInvocationId,
};
use serde_json::{json, Map, Value};

use crate::{
    plan_f0_safety_decision, plan_f0_sandbox_admission, F0EffectAdmissionContext,
    F0SafetyDecisionContext, SafetyDisposition, SafetyRequestV1, SqliteLedger,
};

use super::encoding::digest;
use super::execution::CommitCoordinator;
use super::governed_effect_types::{
    receipt, AuthorityDecision, AuthorityPort, AuthorityRequest, ExecutorDispatch,
    ExecutorDispatchError, ExecutorPort, F0GovernanceContext, GovernedEffectConfig,
    GovernedRuntimePortError, PreparedExecution, SafetyPort, SandboxAdmissionPort,
    SandboxAdmissionRequest,
};

/// SQLite-backed C6 effect composer with frozen authority and executor ports.
pub struct SqliteGovernedEffectPort<'a> {
    writer: Box<dyn EffectLedgerWriter + 'a>,
    authority: &'a mut dyn AuthorityPort,
    executor: &'a mut dyn ExecutorPort,
    config: GovernedEffectConfig,
    f0_context: Option<F0GovernanceContext>,
    safety: Option<&'a mut dyn SafetyPort>,
    sandbox: Option<&'a mut dyn SandboxAdmissionPort>,
    next_ordinal: u64,
}

impl<'a> SqliteGovernedEffectPort<'a> {
    /// Creates a single-owner effect writer for one already-started Execution.
    pub fn new(
        ledger: &'a mut SqliteLedger,
        authority: &'a mut dyn AuthorityPort,
        executor: &'a mut dyn ExecutorPort,
        config: GovernedEffectConfig,
    ) -> Result<Self, GovernedRuntimePortError> {
        chrono::DateTime::parse_from_rfc3339(&config.recorded_at)
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
        let snapshot = ledger.load_turn(&config.turn_id)?;
        if snapshot.session_version != config.expected_session_version
            || snapshot.through_position != config.initial_through_position
            || snapshot
                .facts
                .iter()
                .any(|fact| fact.session_id != config.session_id)
            || !execution_is_active(&snapshot.facts, &config.execution_id)
        {
            return Err(GovernedRuntimePortError::InvalidBinding);
        }
        let writer = DirectEffectLedger {
            ledger,
            session_id: config.session_id.clone(),
            version: config.expected_session_version,
            position: config.initial_through_position,
        };
        Ok(Self {
            writer: Box::new(writer),
            authority,
            executor,
            config,
            f0_context: None,
            safety: None,
            sandbox: None,
            next_ordinal: 0,
        })
    }

    pub(super) fn coordinated<'ledger>(
        coordinator: &'a Mutex<CommitCoordinator<'ledger>>,
        authority: &'a mut dyn AuthorityPort,
        executor: &'a mut dyn ExecutorPort,
        config: GovernedEffectConfig,
    ) -> Result<Self, GovernedRuntimePortError>
    where
        'ledger: 'a,
    {
        chrono::DateTime::parse_from_rfc3339(&config.recorded_at)
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
        {
            let current = coordinator
                .lock()
                .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
            if current.version() != config.expected_session_version
                || current.position() != config.initial_through_position
            {
                return Err(GovernedRuntimePortError::InvalidBinding);
            }
        }
        Ok(Self {
            writer: Box::new(CoordinatedEffectLedger { coordinator }),
            authority,
            executor,
            config,
            f0_context: None,
            safety: None,
            sandbox: None,
            next_ordinal: 0,
        })
    }

    /// Attaches the mandatory F0 policy and Sandbox brokers for Prepared-v3.
    pub fn with_f0_governance(
        mut self,
        safety: &'a mut dyn SafetyPort,
        sandbox: &'a mut dyn SandboxAdmissionPort,
        context: F0GovernanceContext,
    ) -> Result<Self, GovernedRuntimePortError> {
        if context.actor_authority_reference.is_empty()
            || context.effective_policy_revision.is_empty()
            || context.goal_reference.as_deref() == Some("")
            || context.plan_reference.as_deref() == Some("")
        {
            return Err(GovernedRuntimePortError::InvalidBinding);
        }
        self.f0_context = Some(context);
        self.safety = Some(safety);
        self.sandbox = Some(sandbox);
        Ok(self)
    }

    /// Returns the latest committed Session version owned by this port.
    pub fn session_version(&self) -> Result<u64, GovernedRuntimePortError> {
        self.writer.version()
    }

    async fn reject_inner(
        &mut self,
        source_model_request_id: &str,
        intent: &ToolIntent,
        error: &PreparationError,
    ) -> Result<CommittedGovernedResult, GovernedRuntimePortError> {
        let result = reduce_preparation_failure(intent, error);
        if matches!(
            result,
            GovernedToolResult::Fail(GovernedEffectFailure {
                code: GovernedFailureCode::InvalidModelOutput
            })
        ) {
            return Ok(CommittedGovernedResult {
                result,
                through_position: self.current_position()?,
                suspension_binding: None,
            });
        }
        let request_id = ModelRequestId::try_from(source_model_request_id)
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
        let code = preparation_code(error.code())?;
        let paths = error
            .failures()
            .iter()
            .map(|failure| Value::String(failure.instance_path().to_owned()))
            .collect();
        let payload = json!({
            "source_model_request_id":source_model_request_id,
            "model_call_id":intent.model_call_id(),
            "proposed_tool_name":intent.tool_name(),
            "code":code,
            "failure_paths":content_binding(&Value::Array(paths))?,
        });
        let seed = format!(
            "{}:{}:{}:{code}",
            self.config.execution_id.as_str(),
            source_model_request_id,
            intent.model_call_id()
        );
        let fact = self.fact(
            &format!("fact-{}", digest(seed.as_bytes())),
            "tool.preparation_rejected",
            Some(request_id),
            None,
            payload,
        )?;
        let position = self.commit(vec![fact])?;
        Ok(CommittedGovernedResult {
            result,
            through_position: position,
            suspension_binding: None,
        })
    }

    async fn invoke_inner(
        &mut self,
        source_model_request_id: &str,
        prepared: &PreparedToolCall,
    ) -> Result<CommittedGovernedResult, GovernedRuntimePortError> {
        ModelRequestId::try_from(source_model_request_id)
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
        if prepared.contract_version() == 3 {
            return self.invoke_f0(prepared).await;
        }
        let invocation_id = self.allocate_tool_id()?;
        let prepared_fact = self.fact_for_tool(
            &invocation_id,
            "effect.prepared",
            json!({
                "prepared_digest":prepared.input_digest(),
                "tool_name":prepared.tool_name(),
                "tool_revision":prepared.tool_revision(),
                "replay_class":replay_class(prepared.replay_class()),
                "model_call_id":prepared.model_call_id(),
            }),
        )?;
        self.commit(vec![prepared_fact])?;
        let (mut reducer, _) = GovernedEffect::new(invocation_id.clone(), prepared.clone());
        let decision = self
            .authority
            .authorize(AuthorityRequest {
                invocation_id: &invocation_id,
                prepared,
            })
            .await?;
        let (verdict, fact) = self.authority_fact(&invocation_id, prepared, decision)?;
        let action = reducer.clone().apply_authorization(verdict.clone());
        if matches!(action, GovernedAction::Fail(_)) {
            return Err(GovernedRuntimePortError::InvalidBinding);
        }
        let position = self.commit(vec![fact])?;
        let action = reducer.apply_authorization(verdict);
        match action {
            GovernedAction::Dispatch(grant) => {
                self.dispatch(invocation_id, prepared, reducer, grant, None)
                    .await
            }
            GovernedAction::Observation(observation) => {
                self.commit_observation(&invocation_id, &observation)?;
                Ok(CommittedGovernedResult {
                    result: GovernedToolResult::Observation(ToolFeedback::Governed(observation)),
                    through_position: self.current_position()?,
                    suspension_binding: None,
                })
            }
            GovernedAction::Suspend(requirement) => {
                let binding =
                    interaction_binding(&requirement, self.child_id(&invocation_id, "suspension"))?;
                Ok(CommittedGovernedResult {
                    result: GovernedToolResult::Suspend(requirement),
                    through_position: position,
                    suspension_binding: Some(binding),
                })
            }
            GovernedAction::Fail(failure) => Ok(CommittedGovernedResult {
                result: GovernedToolResult::Fail(failure),
                through_position: position,
                suspension_binding: None,
            }),
            GovernedAction::Authorize | GovernedAction::None => {
                Err(GovernedRuntimePortError::InvalidBinding)
            }
        }
    }

    async fn invoke_f0(
        &mut self,
        prepared: &PreparedToolCall,
    ) -> Result<CommittedGovernedResult, GovernedRuntimePortError> {
        let invocation_id = self.allocate_tool_id()?;
        let context = self
            .f0_context
            .clone()
            .ok_or(GovernedRuntimePortError::InvalidBinding)?;
        let request = SafetyRequestV1::new(
            self.child_id(&invocation_id, "safety-request"),
            invocation_id.clone(),
            prepared,
            context.actor_authority_reference,
            context.goal_reference,
            context.plan_reference,
            context.effective_policy_revision,
        )
        .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
        let evaluation = self
            .safety
            .as_deref_mut()
            .ok_or(GovernedRuntimePortError::InvalidBinding)?
            .decide(&request)
            .await?;
        let decision_facts = plan_f0_safety_decision(
            &F0SafetyDecisionContext {
                turn_id: self.config.turn_id.clone(),
                execution_id: self.config.execution_id.clone(),
                recorded_at: self.config.recorded_at.clone(),
            },
            &request,
            prepared,
            &evaluation.decision,
        )
        .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
        self.commit(decision_facts)?;
        let (mut reducer, _) = GovernedEffect::new(invocation_id.clone(), prepared.clone());
        match evaluation.decision.disposition() {
            SafetyDisposition::Deny => {
                if evaluation.granted_requirements.is_some() || evaluation.interaction.is_some() {
                    return Err(GovernedRuntimePortError::InvalidBinding);
                }
                let action = reducer.apply_authorization(AuthorizationVerdict::Deny {
                    code: "safety_denied".into(),
                    details: None,
                });
                let denied = self.fact_for_tool(
                    &invocation_id,
                    "effect.denied",
                    json!({"prepared_digest":prepared.input_digest(),"code":"safety_denied"}),
                )?;
                self.commit(vec![denied])?;
                let GovernedAction::Observation(observation) = action else {
                    return Err(GovernedRuntimePortError::InvalidBinding);
                };
                self.commit_observation(&invocation_id, &observation)?;
                Ok(CommittedGovernedResult {
                    result: GovernedToolResult::Observation(ToolFeedback::Governed(observation)),
                    through_position: self.current_position()?,
                    suspension_binding: None,
                })
            }
            SafetyDisposition::InteractionRequired => {
                let interaction = evaluation
                    .interaction
                    .ok_or(GovernedRuntimePortError::InvalidBinding)?;
                if evaluation.granted_requirements.is_some() {
                    return Err(GovernedRuntimePortError::InvalidBinding);
                }
                let (verdict, fact) = self.authority_fact(
                    &invocation_id,
                    prepared,
                    AuthorityDecision::InteractionRequired {
                        kind: interaction.kind,
                        prompt: interaction.prompt,
                        response_schema: interaction.response_schema,
                        expiry_code: interaction.expiry_code,
                    },
                )?;
                let action = reducer.apply_authorization(verdict);
                let position = self.commit(vec![fact])?;
                let GovernedAction::Suspend(requirement) = action else {
                    return Err(GovernedRuntimePortError::InvalidBinding);
                };
                let binding =
                    interaction_binding(&requirement, self.child_id(&invocation_id, "suspension"))?;
                Ok(CommittedGovernedResult {
                    result: GovernedToolResult::Suspend(requirement),
                    through_position: position,
                    suspension_binding: Some(binding),
                })
            }
            SafetyDisposition::Allow => {
                if evaluation.interaction.is_some() {
                    return Err(GovernedRuntimePortError::InvalidBinding);
                }
                let requirements = evaluation
                    .granted_requirements
                    .ok_or(GovernedRuntimePortError::InvalidBinding)?;
                let grant = InvocationGrant::new(
                    GrantId::new(self.child_id(&invocation_id, "grant"))
                        .map_err(|_| GovernedRuntimePortError::InvalidBinding)?,
                    invocation_id.clone(),
                    prepared.input_digest(),
                    prepared.tool_name(),
                    prepared.tool_revision(),
                    requirements,
                    evaluation
                        .decision
                        .constraints_digest()
                        .ok_or(GovernedRuntimePortError::InvalidBinding)?,
                    evaluation.decision.policy_revision(),
                )
                .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
                let admission = self
                    .sandbox
                    .as_deref_mut()
                    .ok_or(GovernedRuntimePortError::InvalidBinding)?
                    .admit(SandboxAdmissionRequest {
                        safety_request: &request,
                        decision: &evaluation.decision,
                        grant: &grant,
                    })?;
                let planned = plan_f0_sandbox_admission(
                    &F0EffectAdmissionContext {
                        turn_id: self.config.turn_id.clone(),
                        execution_id: self.config.execution_id.clone(),
                        preflight_id: admission.preflight_id,
                        effective_limits_digest: admission.effective_limits_digest,
                        recorded_at: self.config.recorded_at.clone(),
                    },
                    &request,
                    prepared,
                    &grant,
                    &evaluation.decision,
                    &admission.binding,
                    &admission.dispatch_attempt_id,
                )
                .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
                self.commit(planned.facts)?;
                let action =
                    reducer.apply_authorization(AuthorizationVerdict::Approve(grant.clone()));
                if !matches!(action, GovernedAction::Dispatch(_)) {
                    return Err(GovernedRuntimePortError::InvalidBinding);
                }
                self.dispatch(
                    invocation_id,
                    prepared,
                    reducer,
                    grant,
                    Some(planned.execution),
                )
                .await
            }
        }
    }

    fn authority_fact(
        &self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        decision: AuthorityDecision,
    ) -> Result<(AuthorizationVerdict, FactDraft), GovernedRuntimePortError> {
        match decision {
            AuthorityDecision::Approve {
                granted_requirements,
                constraints_digest,
                authority_revision,
            } => {
                let grant_id = GrantId::new(self.child_id(invocation_id, "grant"))
                    .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
                let grant = InvocationGrant::new(
                    grant_id.clone(),
                    invocation_id.clone(),
                    prepared.input_digest(),
                    prepared.tool_name(),
                    prepared.tool_revision(),
                    granted_requirements.clone(),
                    constraints_digest,
                    authority_revision.clone(),
                )
                .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
                let fact = self.fact_for_tool(
                    invocation_id,
                    "effect.authorized",
                    json!({
                        "prepared_digest":prepared.input_digest(),
                        "grant_id":grant_id.as_str(),
                        "authority_revision":authority_revision,
                        "granted_requirements":content_binding(&serde_json::to_value(granted_requirements).map_err(|_| GovernedRuntimePortError::InvalidBinding)?)?,
                    }),
                )?;
                Ok((AuthorizationVerdict::Approve(grant), fact))
            }
            AuthorityDecision::Deny { safe_details } => {
                let mut payload = Map::from_iter([
                    ("prepared_digest".into(), json!(prepared.input_digest())),
                    ("code".into(), json!("authorization_denied")),
                ]);
                if let Some(details) = &safe_details {
                    payload.insert(
                        "safe_details".into(),
                        content_binding(&Value::String(details.clone()))?,
                    );
                }
                Ok((
                    AuthorizationVerdict::Deny {
                        code: "authorization_denied".into(),
                        details: safe_details,
                    },
                    self.fact_for_tool(
                        invocation_id,
                        "effect.denied",
                        Value::Object(payload),
                    )?,
                ))
            }
            AuthorityDecision::ReplacementRequired => Ok((
                AuthorizationVerdict::ReplacementRequired,
                self.fact_for_tool(
                    invocation_id,
                    "effect.denied",
                    json!({"prepared_digest":prepared.input_digest(),"code":"replacement_required"}),
                )?,
            )),
            AuthorityDecision::InteractionRequired {
                kind,
                prompt,
                response_schema,
                expiry_code,
            } => {
                let interaction_id = InteractionId::new(self.child_id(invocation_id, "interaction"))
                    .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
                let suspension_id = self.child_id(invocation_id, "suspension");
                let request = InteractionRequest {
                    interaction_id: interaction_id.clone(),
                    invocation_id: invocation_id.clone(),
                    prepared_digest: prepared.input_digest().to_owned(),
                    kind,
                    prompt: prompt.clone(),
                    response_schema: response_schema.clone(),
                    expiry_policy: expiry_code.clone(),
                };
                let schema = CanonicalPayload::from_value(&response_schema)
                    .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
                let fact = self.fact_for_tool(
                    invocation_id,
                    "interaction.requested",
                    json!({
                        "interaction_id":interaction_id.as_str(),
                        "suspension_id":suspension_id,
                        "prepared_digest":prepared.input_digest(),
                        "kind":interaction_kind(kind),
                        "prompt":content_binding(&prompt)?,
                        "response_schema":{"digest":schema.sha256(),"inline_utf8":schema.as_json()},
                        "response_schema_digest":schema.sha256(),
                        "expiry_code":expiry_code,
                    }),
                )?;
                Ok((AuthorizationVerdict::InteractionRequired(request), fact))
            }
        }
    }

    async fn dispatch(
        &mut self,
        invocation_id: ToolInvocationId,
        prepared: &PreparedToolCall,
        mut reducer: GovernedEffect,
        grant: InvocationGrant,
        admitted_execution: Option<PreparedExecution>,
    ) -> Result<CommittedGovernedResult, GovernedRuntimePortError> {
        let prepared_execution = admitted_execution
            .map(Ok)
            .unwrap_or_else(|| self.executor.prepare(&invocation_id, prepared, &grant));
        let execution = match prepared_execution {
            Ok(value) => value,
            Err(requirement) => {
                let failure = reducer.apply_execution(ExecutionFact::Unsupported {
                    requirement: requirement.clone(),
                });
                let fact = self.fact_for_tool(
                    &invocation_id,
                    "effect.failed",
                    json!({"prepared_digest":prepared.input_digest(),"code":"requirement_unsupported","evidence":content_binding(&Value::String(requirement))?}),
                )?;
                let position = self.commit(vec![fact])?;
                return Ok(CommittedGovernedResult {
                    result: action_result(failure),
                    through_position: position,
                    suspension_binding: None,
                });
            }
        };
        validate_execution(&execution)?;
        let dispatch_id = DispatchAttemptId::new(execution.dispatch_attempt_id.as_str())
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
        let started = self.fact_for_tool(
            &invocation_id,
            "effect.started",
            json!({
                "prepared_digest":prepared.input_digest(),
                "grant_id":grant.grant_id.as_str(),
                "executor_id":execution.executor_id,
                "executor_revision":execution.executor_revision,
                "dispatch_attempt_id":execution.dispatch_attempt_id,
            }),
        )?;
        self.commit(vec![started])?;
        if !matches!(
            reducer.apply_execution(ExecutionFact::Started(dispatch_id)),
            GovernedAction::None
        ) {
            return Err(GovernedRuntimePortError::InvalidBinding);
        }
        let receipt_id = self.child_id(&invocation_id, "receipt");
        let terminal = self
            .executor
            .dispatch(ExecutorDispatch {
                invocation_id: &invocation_id,
                prepared,
                grant: &grant,
                execution: &execution,
                receipt_id: &receipt_id,
            })
            .await;
        let fact = match terminal {
            Ok(value) => value,
            Err(error) => {
                return self.commit_uncertain(&invocation_id, prepared, &mut reducer, error)
            }
        };
        let binding_valid = validate_terminal_binding(
            &fact,
            &invocation_id,
            prepared,
            &grant,
            &execution,
            &receipt_id,
        );
        let validation = reducer.clone().apply_execution(fact.clone());
        let artifact = artifact_commit_payload(&fact, receipt(&fact));
        if binding_valid.is_err()
            || matches!(validation, GovernedAction::Fail(_))
            || receipt(&fact).is_none()
            || artifact.is_err()
        {
            return self.commit_uncertain(
                &invocation_id,
                prepared,
                &mut reducer,
                ExecutorDispatchError::ReceiptInvalid,
            );
        }
        self.commit_receipt(
            &invocation_id,
            prepared,
            receipt(&fact).expect("checked receipt"),
            &fact,
        )?;
        self.commit_terminal(
            &invocation_id,
            prepared,
            &fact,
            artifact.expect("checked artifact"),
        )?;
        let action = reducer.apply_execution(fact);
        let GovernedAction::Observation(observation) = action else {
            return Err(GovernedRuntimePortError::InvalidBinding);
        };
        let position = self.commit_observation(&invocation_id, &observation)?;
        Ok(CommittedGovernedResult {
            result: GovernedToolResult::Observation(ToolFeedback::Governed(observation)),
            through_position: position,
            suspension_binding: None,
        })
    }

    fn commit_receipt(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        receipt: &EffectReceipt,
        terminal: &ExecutionFact,
    ) -> Result<u64, GovernedRuntimePortError> {
        receipt
            .validate()
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
        let evidence = terminal_evidence(terminal)?;
        let fact = self.fact_for_tool(
            invocation_id,
            "effect.receipt",
            json!({
                "receipt_id":receipt.receipt_id.as_str(),
                "prepared_digest":prepared.input_digest(),
                "grant_id":receipt.grant_id.as_str(),
                "executor_id":receipt.executor_id,
                "executor_revision":receipt.executor_revision,
                "classification":terminal_class(receipt.terminal_classification),
                "result_or_evidence":content_binding(&evidence)?,
            }),
        )?;
        self.commit(vec![fact])
    }

    fn commit_terminal(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        fact: &ExecutionFact,
        artifact: Option<Value>,
    ) -> Result<u64, GovernedRuntimePortError> {
        let receipt = receipt(fact).ok_or(GovernedRuntimePortError::InvalidBinding)?;
        let (kind, payload) = match fact {
            ExecutionFact::Completed { content, .. } => (
                "effect.completed",
                json!({"prepared_digest":prepared.input_digest(),"receipt_id":receipt.receipt_id.as_str(),"result":content_binding(content)?}),
            ),
            ExecutionFact::Failed {
                code,
                details,
                partial,
                ..
            } => {
                let mut payload = Map::from_iter([
                    ("prepared_digest".into(), json!(prepared.input_digest())),
                    ("receipt_id".into(), json!(receipt.receipt_id.as_str())),
                    ("code".into(), json!(failure_code(code))),
                ]);
                if details.is_some() || partial.is_some() {
                    payload.insert(
                        "evidence".into(),
                        content_binding(&json!({"details":details,"partial":partial}))?,
                    );
                }
                ("effect.failed", Value::Object(payload))
            }
            _ => return Err(GovernedRuntimePortError::InvalidBinding),
        };
        let durable = self.fact_for_tool(invocation_id, kind, payload)?;
        let mut facts = vec![durable];
        if let Some(payload) = artifact {
            facts.push(self.fact_for_tool(invocation_id, "artifact.committed", payload)?);
        }
        self.commit(facts)
    }

    fn commit_uncertain(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        reducer: &mut GovernedEffect,
        error: ExecutorDispatchError,
    ) -> Result<CommittedGovernedResult, GovernedRuntimePortError> {
        let reason = match error {
            ExecutorDispatchError::StartedWithoutReceipt => "started_without_receipt",
            ExecutorDispatchError::ExecutorStateUnknown => "executor_state_unknown",
            ExecutorDispatchError::ReceiptInvalid => "receipt_invalid",
        };
        let evidence = reason.to_owned();
        let fact = self.fact_for_tool(
            invocation_id,
            "effect.uncertain",
            json!({"prepared_digest":prepared.input_digest(),"reason":reason,"evidence":content_binding(&Value::String(evidence.clone()))?}),
        )?;
        let position = self.commit(vec![fact])?;
        let action = reducer.apply_execution(ExecutionFact::Uncertain { evidence });
        Ok(CommittedGovernedResult {
            result: action_result(action),
            through_position: position,
            suspension_binding: Some(GovernedSuspensionBinding::OperatorReconciliation {
                suspension_id: self.child_id(invocation_id, "suspension"),
                invocation_id: invocation_id.as_str().to_owned(),
                prepared_digest: prepared.input_digest().to_owned(),
            }),
        })
    }

    fn commit_observation(
        &mut self,
        invocation_id: &ToolInvocationId,
        observation: &GovernedObservation,
    ) -> Result<u64, GovernedRuntimePortError> {
        let fact = self.fact_for_tool(
            invocation_id,
            "effect.observation",
            json!({
                "prepared_digest":observation.prepared_digest,
                "model_call_id":observation.model_call_id,
                "observation":content_binding(&observation.model_envelope())?,
            }),
        )?;
        self.commit(vec![fact])
    }

    fn allocate_tool_id(&mut self) -> Result<ToolInvocationId, GovernedRuntimePortError> {
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or(GovernedRuntimePortError::InvalidBinding)?;
        let seed = format!(
            "{}:{}",
            self.config.execution_id.as_str(),
            self.next_ordinal
        );
        ToolInvocationId::new(format!("invocation-{}", digest(seed.as_bytes())))
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)
    }

    fn child_id(&self, invocation_id: &ToolInvocationId, kind: &str) -> String {
        let seed = format!("{}:{kind}", invocation_id.as_str());
        format!("{kind}-{}", digest(seed.as_bytes()))
    }

    fn fact_for_tool(
        &self,
        invocation_id: &ToolInvocationId,
        kind: &str,
        payload: Value,
    ) -> Result<FactDraft, GovernedRuntimePortError> {
        let fact_id = self.child_id(invocation_id, kind);
        self.fact(
            &fact_id,
            kind,
            None,
            Some(ledger_tool_id(invocation_id)?),
            payload,
        )
    }

    fn fact(
        &self,
        fact_id: &str,
        kind: &str,
        model_request_id: Option<ModelRequestId>,
        tool_invocation_id: Option<LedgerToolId>,
        payload: Value,
    ) -> Result<FactDraft, GovernedRuntimePortError> {
        Ok(FactDraft {
            fact_id: FactId::try_from(fact_id)
                .map_err(|_| GovernedRuntimePortError::InvalidBinding)?,
            turn_id: Some(self.config.turn_id.clone()),
            execution_id: Some(self.config.execution_id.clone()),
            model_request_id,
            tool_invocation_id,
            kind: FactKind::new(kind).map_err(|_| GovernedRuntimePortError::InvalidBinding)?,
            schema_version: 1,
            payload: CanonicalPayload::from_value(&payload)
                .map_err(|_| GovernedRuntimePortError::InvalidBinding)?,
            recorded_at: self.config.recorded_at.clone(),
        })
    }

    fn commit(&mut self, facts: Vec<FactDraft>) -> Result<u64, GovernedRuntimePortError> {
        self.writer.append(facts)
    }

    fn current_position(&self) -> Result<u64, GovernedRuntimePortError> {
        self.writer.position()
    }
}

trait EffectLedgerWriter: Send {
    fn append(&mut self, facts: Vec<FactDraft>) -> Result<u64, GovernedRuntimePortError>;
    fn version(&self) -> Result<u64, GovernedRuntimePortError>;
    fn position(&self) -> Result<u64, GovernedRuntimePortError>;
}

struct DirectEffectLedger<'a> {
    ledger: &'a mut SqliteLedger,
    session_id: garive_ledger::SessionId,
    version: u64,
    position: u64,
}

impl EffectLedgerWriter for DirectEffectLedger<'_> {
    fn append(&mut self, facts: Vec<FactDraft>) -> Result<u64, GovernedRuntimePortError> {
        let result = self
            .ledger
            .commit(self.session_id.clone(), self.version, facts)?;
        self.version = result.session_version;
        self.position = result
            .positions
            .last()
            .copied()
            .ok_or(GovernedRuntimePortError::InvalidBinding)?;
        Ok(self.position)
    }

    fn version(&self) -> Result<u64, GovernedRuntimePortError> {
        Ok(self.version)
    }

    fn position(&self) -> Result<u64, GovernedRuntimePortError> {
        Ok(self.position)
    }
}

struct CoordinatedEffectLedger<'a, 'ledger> {
    coordinator: &'a Mutex<CommitCoordinator<'ledger>>,
}

impl EffectLedgerWriter for CoordinatedEffectLedger<'_, '_> {
    fn append(&mut self, facts: Vec<FactDraft>) -> Result<u64, GovernedRuntimePortError> {
        let mut coordinator = self
            .coordinator
            .lock()
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
        match coordinator.commit(facts) {
            Ok(result) => result
                .positions
                .last()
                .copied()
                .ok_or(GovernedRuntimePortError::InvalidBinding),
            Err(error) => {
                coordinator.record_failure(error);
                Err(GovernedRuntimePortError::InvalidBinding)
            }
        }
    }

    fn version(&self) -> Result<u64, GovernedRuntimePortError> {
        self.coordinator
            .lock()
            .map(|coordinator| coordinator.version())
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)
    }

    fn position(&self) -> Result<u64, GovernedRuntimePortError> {
        self.coordinator
            .lock()
            .map(|coordinator| coordinator.position())
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)
    }
}

impl GovernedEffectPort for SqliteGovernedEffectPort<'_> {
    fn reject<'a>(
        &'a mut self,
        source_model_request_id: &'a str,
        intent: &'a ToolIntent,
        error: &'a PreparationError,
    ) -> GovernedEffectFuture<'a> {
        Box::pin(async move {
            self.reject_inner(source_model_request_id, intent, error)
                .await
                .map_err(|_| PortFailure::Tool)
        })
    }

    fn invoke<'a>(
        &'a mut self,
        source_model_request_id: &'a str,
        prepared: &'a PreparedToolCall,
    ) -> GovernedEffectFuture<'a> {
        Box::pin(async move {
            self.invoke_inner(source_model_request_id, prepared)
                .await
                .map_err(|_| PortFailure::Tool)
        })
    }
}

fn ledger_tool_id(value: &ToolInvocationId) -> Result<LedgerToolId, GovernedRuntimePortError> {
    LedgerToolId::try_from(value.as_str()).map_err(|_| GovernedRuntimePortError::InvalidBinding)
}

fn content_binding(value: &Value) -> Result<Value, GovernedRuntimePortError> {
    let canonical = CanonicalPayload::from_value(value)
        .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
    Ok(json!({"digest":canonical.sha256(),"inline_utf8":canonical.as_json()}))
}

fn artifact_commit_payload(
    fact: &ExecutionFact,
    receipt: Option<&EffectReceipt>,
) -> Result<Option<Value>, GovernedRuntimePortError> {
    let ExecutionFact::Completed { content, .. } = fact else {
        return Ok(None);
    };
    let Some(object) = content.as_object() else {
        return Ok(None);
    };
    if object.get("artifact_contract").and_then(Value::as_str) != Some("garive.artifact.v1") {
        return Ok(None);
    }
    let required = [
        "artifact_contract",
        "artifact_id",
        "artifact_revision",
        "workspace_id",
        "grant_revision",
        "display_name",
        "byte_size",
        "content_digest",
        "kind",
        "mime_type",
        "verification",
        "preview",
        "revealable",
        "exportable",
    ];
    if object.len() != required.len() || required.iter().any(|key| !object.contains_key(*key)) {
        return Err(GovernedRuntimePortError::InvalidBinding);
    }
    let text = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(GovernedRuntimePortError::InvalidBinding)
    };
    let artifact_id = text("artifact_id")?;
    let workspace_id = text("workspace_id")?;
    let display_name = text("display_name")?;
    let content_digest = text("content_digest")?;
    let kind = text("kind")?;
    let mime_type = text("mime_type")?;
    let verification = text("verification")?;
    let preview = text("preview")?;
    let revision = object
        .get("artifact_revision")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or(GovernedRuntimePortError::InvalidBinding)?;
    let byte_size = object
        .get("byte_size")
        .and_then(Value::as_u64)
        .filter(|value| *value <= 256 * 1_024)
        .ok_or(GovernedRuntimePortError::InvalidBinding)?;
    if !artifact_id.starts_with("artifact-")
        || !workspace_id.starts_with("workspace-")
        || display_name.len() > 128
        || content_digest.len() != 64
        || !content_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || !matches!(kind, "text" | "file")
        || !matches!(verification, "not_run" | "passed" | "failed" | "partial")
        || !matches!(preview, "unavailable" | "text")
        || !object.get("revealable").is_some_and(Value::is_boolean)
        || !object.get("exportable").is_some_and(Value::is_boolean)
    {
        return Err(GovernedRuntimePortError::InvalidBinding);
    }
    let receipt = receipt.ok_or(GovernedRuntimePortError::InvalidBinding)?;
    Ok(Some(json!({
        "artifact_id":artifact_id,
        "revision":revision,
        "receipt_id":receipt.receipt_id.as_str(),
        "display_name":display_name,
        "kind":kind,
        "mime_type":mime_type,
        "byte_size":byte_size,
        "content_digest":content_digest,
        "verification":verification,
        "preview":preview,
        "workspace_id":workspace_id,
        "revealable":object["revealable"],
        "exportable":object["exportable"],
    })))
}

fn action_result(action: GovernedAction) -> GovernedToolResult {
    match action {
        GovernedAction::Observation(value) => {
            GovernedToolResult::Observation(ToolFeedback::Governed(value))
        }
        GovernedAction::Suspend(value) => GovernedToolResult::Suspend(value),
        GovernedAction::Fail(value) => GovernedToolResult::Fail(value),
        _ => GovernedToolResult::Fail(GovernedEffectFailure {
            code: GovernedFailureCode::CorruptRecoveryState,
        }),
    }
}

fn interaction_binding(
    requirement: &SuspensionRequirement,
    suspension_id: String,
) -> Result<GovernedSuspensionBinding, GovernedRuntimePortError> {
    match requirement {
        SuspensionRequirement::Interaction(request) => Ok(GovernedSuspensionBinding::Interaction {
            suspension_id,
            interaction_id: request.interaction_id.as_str().to_owned(),
            invocation_id: request.invocation_id.as_str().to_owned(),
            prepared_digest: request.prepared_digest.clone(),
        }),
        SuspensionRequirement::OperatorReconciliation { .. } => {
            Err(GovernedRuntimePortError::InvalidBinding)
        }
    }
}

fn preparation_code(code: PreparationErrorCode) -> Result<&'static str, GovernedRuntimePortError> {
    match code {
        PreparationErrorCode::InvalidToolName => Ok("invalid_tool_name"),
        PreparationErrorCode::ToolNotAdmitted => Ok("tool_not_admitted"),
        PreparationErrorCode::InvalidArgumentsJson => Ok("invalid_arguments_json"),
        PreparationErrorCode::ArgumentsSchemaMismatch => Ok("arguments_schema_mismatch"),
        PreparationErrorCode::NonCanonicalValue => Ok("non_canonical_value"),
        _ => Err(GovernedRuntimePortError::InvalidBinding),
    }
}

const fn replay_class(value: ReplayClass) -> &'static str {
    match value {
        ReplayClass::ReadOnly => "read_only",
        ReplayClass::Idempotent => "idempotent",
        ReplayClass::ReceiptRecoverable => "receipt_recoverable",
        ReplayClass::NeverReplay => "never_replay",
    }
}

const fn interaction_kind(value: garive_tools::InteractionKind) -> &'static str {
    match value {
        garive_tools::InteractionKind::Approval => "approval",
        garive_tools::InteractionKind::ExternalInput => "external_input",
    }
}

const fn terminal_class(value: TerminalClassification) -> &'static str {
    match value {
        TerminalClassification::Completed => "completed",
        TerminalClassification::Failed => "failed",
    }
}

fn failure_code(value: &str) -> &'static str {
    match value {
        "timeout" => "timeout",
        "cancelled" => "cancelled",
        "executor_unavailable" => "executor_unavailable",
        _ => "tool_failure",
    }
}

fn validate_execution(
    value: &super::governed_effect_types::PreparedExecution,
) -> Result<(), GovernedRuntimePortError> {
    if value.executor_id.is_empty()
        || value.executor_revision.is_empty()
        || value.dispatch_attempt_id.is_empty()
    {
        Err(GovernedRuntimePortError::InvalidBinding)
    } else {
        Ok(())
    }
}

fn execution_is_active(
    facts: &[garive_ledger::DurableFact],
    execution_id: &garive_ledger::ExecutionId,
) -> bool {
    let mut started = false;
    for fact in facts
        .iter()
        .filter(|fact| fact.execution_id.as_ref() == Some(execution_id))
    {
        match fact.kind.as_str() {
            "execution.started" => started = true,
            "execution.abandoned"
            | "execution.completed"
            | "execution.suspended"
            | "execution.stopped"
            | "execution.failed" => return false,
            _ => {}
        }
    }
    started
}

fn validate_terminal_binding(
    fact: &ExecutionFact,
    invocation_id: &ToolInvocationId,
    prepared: &PreparedToolCall,
    grant: &InvocationGrant,
    execution: &super::governed_effect_types::PreparedExecution,
    expected_receipt_id: &str,
) -> Result<(), GovernedRuntimePortError> {
    let receipt = receipt(fact).ok_or(GovernedRuntimePortError::InvalidBinding)?;
    receipt
        .validate()
        .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
    let evidence = terminal_evidence(fact)?;
    let canonical = CanonicalPayload::from_value(&evidence)
        .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
    if receipt.receipt_id.as_str() != expected_receipt_id
        || &receipt.invocation_id != invocation_id
        || receipt.prepared_digest != prepared.input_digest()
        || receipt.grant_id != grant.grant_id
        || receipt.executor_id != execution.executor_id
        || receipt.executor_revision != execution.executor_revision
        || receipt.result_digest != canonical.sha256()
    {
        return Err(GovernedRuntimePortError::InvalidBinding);
    }
    let classification_matches = matches!(
        (fact, receipt.terminal_classification),
        (
            ExecutionFact::Completed { .. },
            TerminalClassification::Completed
        ) | (ExecutionFact::Failed { .. }, TerminalClassification::Failed)
    );
    if classification_matches {
        Ok(())
    } else {
        Err(GovernedRuntimePortError::InvalidBinding)
    }
}

fn terminal_evidence(fact: &ExecutionFact) -> Result<Value, GovernedRuntimePortError> {
    match fact {
        ExecutionFact::Completed { content, .. } => Ok(content.clone()),
        ExecutionFact::Failed {
            code,
            details,
            partial,
            ..
        } => Ok(json!({"code":code,"details":details,"partial":partial})),
        _ => Err(GovernedRuntimePortError::InvalidBinding),
    }
}
