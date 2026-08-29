use std::{
    collections::VecDeque,
    fs,
    num::NonZeroU32,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

use futures::executor::block_on;
use garive_core::{
    execute_model_only, AgentCursor, AgentDefinitionId, AgentDefinitionRevision, AgentEntry,
    AgentEvent, AgentExecutionPorts, AgentInstanceId, AgentOutcome, AgentTurnRequest, ClockPort,
    ContextItem, ContextPort, ContextPortError, ContextPurpose, ContextRequest, ContextSurface,
    EventSink, ExecutionId, ExecutionLimits, FactRef, MissingUsagePolicy, ModelOnlyLimits,
    ModelRecoveryPolicy, OutputLimitAction, PortFailure, ResumeInput, SessionId, StopReason,
    SuspensionReason, TerminalRecoveryAction, TurnId,
};
use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture,
    ModelInputContent, ModelInputItem, ModelItem, ModelObserver, ModelOutputSettings, ModelPort,
    ModelPortFailure, ModelRole, ModelStopReason, ModelStreamEvent, ModelTargetId, ModelUsage,
    ObserverDecision, RejectionKind, TextMode, TokenCount, UnavailableKind, UsageSource,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/model-only-execution.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn known_usage() -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Known(1),
        output_tokens: TokenCount::Known(1),
        cache_read_tokens: None,
        cache_write_tokens: None,
        source: UsageSource::ProviderReported,
    }
}

fn unknown_usage() -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Unknown,
        output_tokens: TokenCount::Unknown,
        cache_read_tokens: None,
        cache_write_tokens: None,
        source: UsageSource::ProviderReported,
    }
}

struct FakeContext {
    scripts: VecDeque<String>,
    calls: usize,
}

impl ContextPort for FakeContext {
    fn derive(
        &mut self,
        request: &ContextRequest,
        _: u32,
    ) -> Result<ContextSurface, ContextPortError> {
        self.calls += 1;
        match self.scripts.pop_front().as_deref() {
            Some("failure") => return Err(ContextPortError::PortFailure),
            Some("required-budget") => return Err(ContextPortError::RequiredFactsExceedBudget),
            _ => {}
        }
        let fact_ref = FactRef {
            session_id: request.session_id.clone(),
            position: request.through_position,
        };
        Ok(ContextSurface {
            purpose: ContextPurpose::Inference,
            from_position: 1,
            through_position: request.through_position,
            items: vec![ContextItem::Input {
                fact_ref: fact_ref.clone(),
                item: ModelInputItem::Message {
                    role: ModelRole::User,
                    content: vec![ModelInputContent::Text("hi".into())],
                },
            }],
            retained_refs: vec![fact_ref],
            dropped_refs: vec![],
            filtered_refs: vec![],
            item_count: 1,
            utf8_bytes: 2,
        })
    }
}

struct FakeModel {
    scripts: Mutex<VecDeque<String>>,
    calls: AtomicUsize,
    targets: Mutex<Vec<String>>,
    request_ids: Mutex<Vec<String>>,
}

impl ModelPort for FakeModel {
    fn invoke<'a>(
        &'a self,
        request: &'a garive_llm::ModelRequest,
        observer: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.targets
            .lock()
            .unwrap()
            .push(request.target_id.as_str().into());
        self.request_ids
            .lock()
            .unwrap()
            .push(request.request_id.as_str().into());
        let script = self.scripts.lock().unwrap().pop_front().unwrap();
        let result = match script.as_str() {
            "completed-text-known" => Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text {
                    text: "done".into(),
                }],
                usage: known_usage(),
                stop_reason: ModelStopReason::EndTurn,
            }),
            "completed-text-unknown" => Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text {
                    text: "done".into(),
                }],
                usage: unknown_usage(),
                stop_reason: ModelStopReason::EndTurn,
            }),
            "completed-tool-known" => Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::ToolIntent {
                    model_call_id: "call".into(),
                    tool_name: "tool".into(),
                    arguments_json: "{}".into(),
                }],
                usage: known_usage(),
                stop_reason: ModelStopReason::ToolUse,
            }),
            "context-overflow" => Ok(InvokeOutcome::Rejected {
                kind: RejectionKind::ContextOverflow,
                sanitized_evidence: "limit".into(),
            }),
            "output-limit" => Ok(InvokeOutcome::Interrupted {
                kind: InterruptionKind::OutputLimit,
                partial_items: vec![ModelItem::Text {
                    text: "part".into(),
                }],
                usage: known_usage(),
            }),
            "rate-limited" => Ok(InvokeOutcome::Unavailable {
                kind: UnavailableKind::RateLimited,
                retry_after: None,
            }),
            "transport" => Ok(InvokeOutcome::Interrupted {
                kind: InterruptionKind::Transport,
                partial_items: vec![],
                usage: known_usage(),
            }),
            "stream-event" => {
                let decision = observer.observe(&ModelStreamEvent::TextDelta {
                    output_index: 0,
                    delta: "x".into(),
                });
                assert_eq!(decision, ObserverDecision::Cancel);
                Ok(InvokeOutcome::Interrupted {
                    kind: InterruptionKind::Cancelled,
                    partial_items: vec![],
                    usage: known_usage(),
                })
            }
            "stream-cancel" => {
                let decision = observer.observe(&ModelStreamEvent::TextDelta {
                    output_index: 0,
                    delta: "x".into(),
                });
                assert_eq!(decision, ObserverDecision::Cancel);
                Ok(InvokeOutcome::Interrupted {
                    kind: InterruptionKind::Cancelled,
                    partial_items: vec![],
                    usage: known_usage(),
                })
            }
            "port-failure" => Err(ModelPortFailure::RequiredPortFailure),
            other => panic!("unknown model script: {other}"),
        };
        Box::pin(async move { result })
    }
}

struct FakeCancellation {
    cancel_after: Option<usize>,
    checks: AtomicUsize,
}

impl ModelCancellation for FakeCancellation {
    fn is_cancelled(&self) -> bool {
        let check = self.checks.fetch_add(1, Ordering::SeqCst) + 1;
        self.cancel_after.is_some_and(|limit| check >= limit)
    }
}

struct FakeEvents {
    failure: Option<String>,
}

impl EventSink for FakeEvents {
    fn emit(&mut self, event: AgentEvent) -> Result<(), PortFailure> {
        if self.failure.as_deref() == Some(event.kind.code()) {
            Err(PortFailure::Event)
        } else {
            Ok(())
        }
    }
}

struct FakeClock {
    tick: u64,
    failure: bool,
}
impl ClockPort for FakeClock {
    fn now_tick(&self) -> Result<u64, PortFailure> {
        if self.failure {
            Err(PortFailure::Clock)
        } else {
            Ok(self.tick)
        }
    }
}

fn request(case: &Value) -> AgentTurnRequest {
    let continuing = case["entry"] == "continue";
    let through = case["last_position"].as_u64().unwrap().max(1);
    AgentTurnRequest {
        session_id: SessionId::try_from("session").unwrap(),
        turn_id: TurnId::try_from("turn").unwrap(),
        execution_id: ExecutionId::try_from(if continuing { "exec-2" } else { "exec-1" }).unwrap(),
        agent_instance_id: AgentInstanceId::try_from("agent").unwrap(),
        definition_id: AgentDefinitionId::try_from("definition").unwrap(),
        definition_revision: AgentDefinitionRevision::try_from("1").unwrap(),
        entry: if continuing {
            AgentEntry::Continue {
                resume_input: ResumeInput::ResourceReady,
            }
        } else {
            AgentEntry::Start {
                trusted_input: "hi".into(),
            }
        },
        cursor: AgentCursor {
            completed_iterations: case["completed"].as_u64().unwrap() as u32,
            last_durable_position: case["last_position"].as_u64().unwrap(),
        },
        context_request: ContextRequest {
            session_id: "session".into(),
            turn_id: "turn".into(),
            purpose: ContextPurpose::Inference,
            after_position: None,
            through_position: through,
            max_items: 10,
            max_utf8_bytes: 100,
        },
        model_targets: if case["unavailable"] == "alternate" {
            vec![
                ModelTargetId::new("primary"),
                ModelTargetId::new("secondary"),
            ]
        } else {
            vec![ModelTargetId::new("primary")]
        },
        required_capabilities: vec![ModelCapability::Text, ModelCapability::Streaming],
        model_output: ModelOutputSettings {
            max_output_tokens: Some(10),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        recovery_policy: ModelRecoveryPolicy {
            max_context_rebuilds: 1,
            output_limit: match case["output_limit"].as_str() {
                Some("retry:1") => OutputLimitAction::Retry { max_retries: 1 },
                Some("complete-partial") => OutputLimitAction::CompletePartial,
                _ => OutputLimitAction::Suspend,
            },
            transport: TerminalRecoveryAction::Suspend,
            unavailable: if case["unavailable"] == "alternate" {
                TerminalRecoveryAction::AlternateThenSuspend
            } else {
                TerminalRecoveryAction::Suspend
            },
            missing_usage: if case["missing_usage"] == "estimate" {
                MissingUsagePolicy::Estimate {
                    input_tokens: 3,
                    output_tokens: 2,
                }
            } else {
                MissingUsagePolicy::Stop
            },
        },
        limits: ModelOnlyLimits {
            execution: ExecutionLimits::new(
                NonZeroU32::new(case["maximum"].as_u64().unwrap() as u32).unwrap(),
            ),
            max_total_tokens: Some(case["max_tokens"].as_u64().unwrap()),
            deadline_tick: case["deadline_tick"].as_u64(),
        },
    }
}

fn render(outcome: &AgentOutcome) -> &'static str {
    match outcome {
        AgentOutcome::Completed { .. } => "completed",
        AgentOutcome::Suspended {
            reason: SuspensionReason::PartialOutput,
            ..
        } => "suspended:partial-output",
        AgentOutcome::Suspended {
            reason: SuspensionReason::ResourceUnavailable,
            ..
        } => "suspended:resource-unavailable",
        AgentOutcome::Stopped {
            reason: StopReason::IterationLimit,
        } => "stopped:iteration-limit",
        AgentOutcome::Stopped {
            reason: StopReason::TokenLimit,
        } => "stopped:token-limit",
        AgentOutcome::Stopped {
            reason: StopReason::Cancelled,
        } => "stopped:cancelled",
        AgentOutcome::Stopped {
            reason: StopReason::Deadline,
        } => "stopped:deadline",
        AgentOutcome::Failed {
            reason: garive_core::AgentFailureReason::RequiredCapabilityUnavailable,
        } => "failed:required-capability",
        AgentOutcome::Failed {
            reason: garive_core::AgentFailureReason::PortFailure,
        } => "failed:port-failure",
        AgentOutcome::Failed {
            reason: garive_core::AgentFailureReason::InvalidModelOutput,
        } => "failed:invalid-model-output",
        other => panic!("unexpected outcome: {other:?}"),
    }
}

fn render_count(count: TokenCount) -> String {
    match count {
        TokenCount::Known(value) => value.to_string(),
        TokenCount::Unknown => "unknown".into(),
    }
}

#[test]
fn rust_consumes_every_model_only_scenario() {
    let document = fixture();
    let cases = document["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 25);
    for case in cases {
        let mut context = FakeContext {
            scripts: case["contexts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().into())
                .collect(),
            calls: 0,
        };
        let model = FakeModel {
            scripts: Mutex::new(
                case["models"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().into())
                    .collect(),
            ),
            calls: AtomicUsize::new(0),
            targets: Mutex::new(vec![]),
            request_ids: Mutex::new(vec![]),
        };
        let cancellation = FakeCancellation {
            cancel_after: case["cancel_after_checks"].as_u64().map(|v| v as usize),
            checks: AtomicUsize::new(0),
        };
        let mut events = FakeEvents {
            failure: case["event_failure"].as_str().map(str::to_owned),
        };
        let clock = FakeClock {
            tick: case["clock_tick"].as_u64().unwrap_or(0),
            failure: case["clock_failure"].as_bool().unwrap_or(false),
        };
        let mut ports = AgentExecutionPorts {
            context: &mut context,
            model: &model,
            events: &mut events,
            cancellation: &cancellation,
            clock: &clock,
        };
        let report = block_on(execute_model_only(&request(case), &mut ports));
        let expected = &case["expected"];
        assert_eq!(
            render(&report.outcome),
            expected["outcome"],
            "{}",
            case["name"]
        );
        assert_eq!(
            report.completed_iterations,
            expected["iterations"].as_u64().unwrap() as u32,
            "{}",
            case["name"]
        );
        assert_eq!(
            context.calls,
            expected["context_calls"].as_u64().unwrap() as usize,
            "{}",
            case["name"]
        );
        assert_eq!(
            model.calls.load(Ordering::SeqCst),
            expected["model_calls"].as_u64().unwrap() as usize,
            "{}",
            case["name"]
        );
        let request_ids = model.request_ids.lock().unwrap();
        assert_eq!(
            request_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            request_ids.len(),
            "{}",
            case["name"]
        );
        if let Some(usage_case) = document["usage_summary_cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|usage_case| usage_case["execution_case"] == case["name"])
        {
            let expected_usage = &usage_case["expected"];
            assert_eq!(
                render_count(report.usage.input_tokens),
                expected_usage["input"]
            );
            assert_eq!(
                render_count(report.usage.output_tokens),
                expected_usage["output"]
            );
            assert_eq!(report.usage.estimated, expected_usage["estimated"]);
        }
        if let Some(targets) = expected.get("targets") {
            let expected_targets: Vec<_> = targets
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(
                *model.targets.lock().unwrap(),
                expected_targets,
                "{}",
                case["name"]
            );
        }
    }
}
