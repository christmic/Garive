use std::{collections::VecDeque, num::NonZeroU32, sync::Mutex};

use futures::executor::block_on;
use garive_core::{
    execute_agent, AgentCursor, AgentDefinitionId, AgentDefinitionRevision, AgentEntry, AgentEvent,
    AgentExecutionPorts, AgentInstanceId, AgentOutcome, AgentToolCapabilities, AgentTurnRequest,
    ClockPort, CommittedGovernedResult, ContextItem, ContextPort, ContextPortError, ContextPurpose,
    ContextRequest, ContextSurface, EventSink, ExecutionId, ExecutionLimits, FactRef,
    GovernedEffectFuture, GovernedEffectPort, MissingUsagePolicy, ModelOnlyLimits,
    ModelRecoveryPolicy, OutputLimitAction, PortFailure, SessionId, SuspensionReason,
    TerminalRecoveryAction, TurnId,
};
use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture, ModelItem, ModelObserver,
    ModelOutputSettings, ModelPort, ModelRequest, ModelRole, ModelStopReason, ModelTargetId,
    ModelUsage, TextMode, TokenCount, UsageSource,
};
use garive_tools::{
    ExecutionCapability, ExecutionRequirements, GovernedEffectFailure, GovernedFailureCode,
    GovernedObservation, GovernedToolResult, InteractionId, InteractionKind, InteractionRequest,
    ObservationOutcome, PreparationError, PreparedToolCall, ReplayClass, SuspensionRequirement,
    ToolDefinition, ToolFeedback, ToolIntent, ToolInvocationId,
};
use serde_json::json;

struct Context {
    positions: Vec<u64>,
}

impl ContextPort for Context {
    fn derive(
        &mut self,
        request: &ContextRequest,
        _: u32,
    ) -> Result<ContextSurface, ContextPortError> {
        self.positions.push(request.through_position);
        Ok(ContextSurface {
            purpose: ContextPurpose::Inference,
            from_position: 1,
            through_position: request.through_position.max(1),
            items: vec![ContextItem::Input {
                fact_ref: FactRef {
                    session_id: request.session_id.clone(),
                    position: 1,
                },
                item: garive_llm::ModelInputItem::Message {
                    role: ModelRole::User,
                    content: vec![garive_llm::ModelInputContent::Text("hi".into())],
                },
            }],
            retained_refs: vec![],
            dropped_refs: vec![],
            filtered_refs: vec![],
            item_count: 1,
            utf8_bytes: 2,
        })
    }
}

struct Model {
    outcomes: Mutex<VecDeque<InvokeOutcome>>,
    tool_counts: Mutex<Vec<usize>>,
}

impl ModelPort for Model {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        self.tool_counts.lock().unwrap().push(request.tools.len());
        let outcome = self.outcomes.lock().unwrap().pop_front().unwrap();
        Box::pin(async move { Ok(outcome) })
    }
}

struct Effects {
    result: GovernedToolResult,
    position: u64,
}

impl GovernedEffectPort for Effects {
    fn reject<'a>(
        &'a mut self,
        _: &'a str,
        intent: &'a ToolIntent,
        error: &'a PreparationError,
    ) -> GovernedEffectFuture<'a> {
        let result = garive_tools::reduce_preparation_failure(intent, error);
        Box::pin(async move {
            Ok(CommittedGovernedResult {
                result,
                through_position: self.position,
            })
        })
    }
    fn invoke<'a>(&'a mut self, _: &'a str, _: &'a PreparedToolCall) -> GovernedEffectFuture<'a> {
        Box::pin(async move {
            Ok(CommittedGovernedResult {
                result: self.result.clone(),
                through_position: self.position,
            })
        })
    }
}

struct Events;
impl EventSink for Events {
    fn emit(&mut self, _: AgentEvent) -> Result<(), PortFailure> {
        Ok(())
    }
}
struct Cancellation;
impl ModelCancellation for Cancellation {
    fn is_cancelled(&self) -> bool {
        false
    }
}
struct Clock;
impl ClockPort for Clock {
    fn now_tick(&self) -> Result<u64, PortFailure> {
        Ok(0)
    }
}

fn tool() -> ToolDefinition {
    ToolDefinition::new(
        "read_file", "1", "Read one file.",
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
        ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1000, 1024).unwrap(),
        ReplayClass::ReadOnly,
    ).unwrap()
}

fn request() -> AgentTurnRequest {
    AgentTurnRequest {
        session_id: SessionId::try_from("session").unwrap(),
        turn_id: TurnId::try_from("turn").unwrap(),
        execution_id: ExecutionId::try_from("execution").unwrap(),
        agent_instance_id: AgentInstanceId::try_from("agent").unwrap(),
        definition_id: AgentDefinitionId::try_from("definition").unwrap(),
        definition_revision: AgentDefinitionRevision::try_from("revision").unwrap(),
        entry: AgentEntry::Start {
            trusted_input: "hi".into(),
        },
        cursor: AgentCursor {
            completed_iterations: 0,
            last_durable_position: 0,
        },
        context_request: ContextRequest {
            session_id: "session".into(),
            turn_id: "turn".into(),
            purpose: ContextPurpose::Inference,
            after_position: None,
            through_position: 1,
            max_items: 8,
            max_utf8_bytes: 1024,
        },
        model_targets: vec![ModelTargetId::new("model")],
        required_capabilities: vec![ModelCapability::Tools],
        model_output: ModelOutputSettings {
            max_output_tokens: Some(64),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        recovery_policy: ModelRecoveryPolicy {
            max_context_rebuilds: 0,
            output_limit: OutputLimitAction::Fail,
            transport: TerminalRecoveryAction::Fail,
            unavailable: TerminalRecoveryAction::Fail,
            missing_usage: MissingUsagePolicy::Stop,
        },
        limits: ModelOnlyLimits {
            execution: ExecutionLimits::new(NonZeroU32::new(3).unwrap()),
            max_total_tokens: Some(100),
            deadline_tick: None,
        },
    }
}

fn usage() -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Known(1),
        output_tokens: TokenCount::Known(1),
        cache_read_tokens: None,
        cache_write_tokens: None,
        source: UsageSource::ProviderReported,
    }
}
fn tool_outcome(name: &str) -> InvokeOutcome {
    InvokeOutcome::Completed {
        items: vec![ModelItem::ToolIntent {
            model_call_id: "call".into(),
            tool_name: name.into(),
            arguments_json: r#"{"path":"a"}"#.into(),
        }],
        usage: usage(),
        stop_reason: ModelStopReason::ToolUse,
    }
}
fn text_outcome() -> InvokeOutcome {
    InvokeOutcome::Completed {
        items: vec![ModelItem::Text {
            text: "done".into(),
        }],
        usage: usage(),
        stop_reason: ModelStopReason::EndTurn,
    }
}
fn observation() -> GovernedToolResult {
    GovernedToolResult::Observation(ToolFeedback::Governed(GovernedObservation {
        invocation_id: ToolInvocationId::new("invocation").unwrap(),
        prepared_digest: "digest".into(),
        model_call_id: "call".into(),
        tool_name: "read_file".into(),
        outcome: ObservationOutcome::Succeeded {
            content: json!({"text":"ok"}),
            truncated: false,
        },
    }))
}

fn run(
    first: InvokeOutcome,
    effect: GovernedToolResult,
    position: u64,
) -> (garive_core::ExecutionReport, Vec<u64>, Vec<usize>) {
    let mut context = Context { positions: vec![] };
    let model = Model {
        outcomes: Mutex::new(VecDeque::from([first, text_outcome()])),
        tool_counts: Mutex::new(vec![]),
    };
    let mut effects = Effects {
        result: effect,
        position,
    };
    let mut events = Events;
    let mut ports = AgentExecutionPorts {
        context: &mut context,
        model: &model,
        events: &mut events,
        cancellation: &Cancellation,
        clock: &Clock,
    };
    let report = block_on(execute_agent(
        &request(),
        &AgentToolCapabilities {
            definitions: vec![tool()],
        },
        &mut ports,
        &mut effects,
    ));
    (
        report,
        context.positions,
        model.tool_counts.into_inner().unwrap(),
    )
}

#[test]
fn governed_observation_advances_context_and_completes() {
    let (report, positions, tool_counts) = run(tool_outcome("read_file"), observation(), 5);
    assert!(matches!(report.outcome, AgentOutcome::Completed { .. }));
    assert_eq!(positions, [1, 5]);
    assert_eq!(tool_counts, [1, 1]);
}

#[test]
fn invalid_intent_is_committed_as_feedback_before_retry() {
    let (report, positions, _) = run(tool_outcome("missing"), observation(), 6);
    assert!(matches!(report.outcome, AgentOutcome::Completed { .. }));
    assert_eq!(positions, [1, 6]);
}

#[test]
fn governed_suspension_and_governed_failure_fail_closed() {
    let interaction =
        GovernedToolResult::Suspend(SuspensionRequirement::Interaction(InteractionRequest {
            interaction_id: InteractionId::new("interaction").unwrap(),
            invocation_id: ToolInvocationId::new("invocation").unwrap(),
            prepared_digest: "digest".into(),
            kind: InteractionKind::Approval,
            prompt: json!({"message":"approve"}),
            response_schema: json!({"type":"boolean"}),
            expiry_policy: "none".into(),
        }));
    let (report, _, _) = run(tool_outcome("read_file"), interaction, 5);
    assert!(matches!(
        report.outcome,
        AgentOutcome::Suspended {
            reason: SuspensionReason::ApprovalRequired,
            last_durable_position: 5,
            ..
        }
    ));
    let failure = GovernedToolResult::Fail(GovernedEffectFailure {
        code: GovernedFailureCode::CorruptRecoveryState,
    });
    let (report, _, _) = run(tool_outcome("read_file"), failure, 0);
    assert!(matches!(report.outcome, AgentOutcome::Failed { .. }));
}
