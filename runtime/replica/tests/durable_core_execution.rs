use std::{num::NonZeroU32, path::PathBuf};

use futures::executor::block_on;
use garive_core::{
    AgentCursor, AgentDefinitionId, AgentDefinitionRevision, AgentEntry, AgentEvent,
    AgentInstanceId, AgentTurnRequest, ClockPort, ContextItem, ContextPort, ContextPortError,
    ContextPurpose, ContextRequest, ContextSurface, EventSink, ExecutionId as CoreExecutionId,
    ExecutionLimits, FactRef, MissingUsagePolicy, ModelOnlyLimits, ModelRecoveryPolicy,
    OutputLimitAction, PortFailure, SessionId as CoreSessionId, TerminalRecoveryAction,
    TurnId as CoreTurnId,
};
use garive_ledger::{
    AgentDefinitionId as LedgerDefinitionId, AgentDefinitionRevision as LedgerRevision,
    AgentInstanceId as LedgerAgentId, CanonicalPayload, FactDraft, FactId, FactKind, SessionId,
};
use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture, ModelInputContent,
    ModelInputItem, ModelItem, ModelObserver, ModelOutputSettings, ModelPort, ModelRequest,
    ModelStopReason, ModelTargetId, ModelUsage, TextMode, TokenCount, UsageSource,
};
use garive_runtime::{
    execute_durable_model_only, plan_start_turn, DurableExecutionConfig, EffectiveRuntimeLimits,
    ModelLifecycleContext, RuntimeCommandId, SqliteLedger, StartTurnCommand,
    TerminalPublicationError, TerminalPublisher,
};
use tempfile::tempdir;

struct Context;
impl ContextPort for Context {
    fn derive(
        &mut self,
        request: &ContextRequest,
        _: u32,
    ) -> Result<ContextSurface, ContextPortError> {
        let fact = FactRef {
            session_id: request.session_id.clone(),
            position: 3,
        };
        Ok(ContextSurface {
            purpose: ContextPurpose::Inference,
            from_position: 1,
            through_position: request.through_position,
            items: vec![ContextItem::Input {
                fact_ref: fact.clone(),
                item: ModelInputItem::Message {
                    role: garive_llm::ModelRole::User,
                    content: vec![ModelInputContent::Text("hello".into())],
                },
            }],
            retained_refs: vec![fact],
            dropped_refs: vec![],
            filtered_refs: vec![],
            item_count: 1,
            utf8_bytes: 5,
        })
    }
}

struct Model {
    path: PathBuf,
    session: SessionId,
}
impl ModelPort for Model {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let ledger = SqliteLedger::open(&self.path).unwrap();
            let active = ledger.list_uncertain_model_requests(&self.session).unwrap();
            assert_eq!(active[0].as_str(), request.request_id.as_str());
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text {
                    text: "done".into(),
                }],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: ModelStopReason::EndTurn,
            })
        })
    }
}

struct Signals;
impl ModelCancellation for Signals {
    fn is_cancelled(&self) -> bool {
        false
    }
}
impl ClockPort for Signals {
    fn now_tick(&self) -> Result<u64, PortFailure> {
        Ok(0)
    }
}
impl EventSink for Signals {
    fn emit(&mut self, _: AgentEvent) -> Result<(), PortFailure> {
        Ok(())
    }
}

struct Publisher {
    path: PathBuf,
    turn: garive_ledger::TurnId,
    fail: bool,
    calls: usize,
}
impl TerminalPublisher for Publisher {
    fn publish_terminal(
        &mut self,
        _: &garive_core::ExecutionReport,
        positions: &[u64],
    ) -> Result<(), TerminalPublicationError> {
        let ledger = SqliteLedger::open(&self.path).unwrap();
        let snapshot = ledger.load_turn(&self.turn).unwrap();
        let kinds: Vec<_> = snapshot
            .facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect();
        assert_eq!(
            &kinds[kinds.len() - 2..],
            ["execution.completed", "turn.completed"]
        );
        assert_eq!(positions.len(), 2);
        self.calls += 1;
        if self.fail {
            Err(TerminalPublicationError)
        } else {
            Ok(())
        }
    }
}

fn open_session() -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from("session-open").unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("session.opened").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&serde_json::json!({})).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

#[test]
fn sqlite_dispatch_and_publication_cross_only_after_their_commits() {
    for fail_publication in [false, true] {
        let directory = tempdir().unwrap();
        let path = directory.path().join("runtime.sqlite3");
        let session = SessionId::try_from(if fail_publication {
            "session-fail"
        } else {
            "session"
        })
        .unwrap();
        let mut ledger = SqliteLedger::open(&path).unwrap();
        ledger
            .commit(session.clone(), 0, vec![open_session()])
            .unwrap();
        let start = StartTurnCommand {
            command_id: RuntimeCommandId::new("start").unwrap(),
            session_id: session.clone(),
            agent_instance_id: LedgerAgentId::try_from("agent").unwrap(),
            definition_id: LedgerDefinitionId::try_from("definition").unwrap(),
            definition_revision: LedgerRevision::try_from("revision").unwrap(),
            snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .into(),
            trusted_input: "hello".into(),
            limits: EffectiveRuntimeLimits {
                max_iterations: 2,
                max_input_tokens: None,
                max_output_tokens: None,
                deadline_budget_ms: None,
            },
            recorded_at: "2026-08-29T00:00:00Z".into(),
        };
        let plan = plan_start_turn(&start, 1).unwrap();
        let execution = plan.execution_id.clone().unwrap();
        ledger.commit(session.clone(), 1, plan.facts).unwrap();
        let request = core_request(&session, &plan.turn_id, &execution);
        let config = DurableExecutionConfig {
            session_id: session.clone(),
            expected_session_version: 2,
            model: ModelLifecycleContext {
                turn_id: plan.turn_id.clone(),
                execution_id: execution,
                deployment_id: "deployment".into(),
                recovery_policy_revision: "policy".into(),
                max_attempts: 1,
                recorded_at: "2026-08-29T00:00:01Z".into(),
            },
        };
        let model = Model {
            path: path.clone(),
            session,
        };
        let mut context = Context;
        let signals = Signals;
        let mut events = Signals;
        let mut publisher = Publisher {
            path: path.clone(),
            turn: plan.turn_id.clone(),
            fail: fail_publication,
            calls: 0,
        };
        let result = block_on(execute_durable_model_only(
            &mut ledger,
            &config,
            &request,
            &mut context,
            &model,
            &mut events,
            &signals,
            &signals,
            &mut publisher,
        ))
        .unwrap();
        assert_eq!(result.publication.is_err(), fail_publication);
        assert_eq!(publisher.calls, 1);
        assert_eq!(ledger.load_turn(&plan.turn_id).unwrap().facts.len(), 8);
    }
}

fn core_request(
    session: &SessionId,
    turn: &garive_ledger::TurnId,
    execution: &garive_ledger::ExecutionId,
) -> AgentTurnRequest {
    AgentTurnRequest {
        session_id: CoreSessionId::try_from(session.as_str()).unwrap(),
        turn_id: CoreTurnId::try_from(turn.as_str()).unwrap(),
        execution_id: CoreExecutionId::try_from(execution.as_str()).unwrap(),
        agent_instance_id: AgentInstanceId::try_from("agent").unwrap(),
        definition_id: AgentDefinitionId::try_from("definition").unwrap(),
        definition_revision: AgentDefinitionRevision::try_from("revision").unwrap(),
        entry: AgentEntry::Start {
            trusted_input: "hello".into(),
        },
        cursor: AgentCursor {
            completed_iterations: 0,
            last_durable_position: 0,
        },
        context_request: ContextRequest {
            session_id: session.as_str().into(),
            turn_id: turn.as_str().into(),
            purpose: ContextPurpose::Inference,
            after_position: None,
            through_position: 4,
            max_items: 10,
            max_utf8_bytes: 100,
        },
        model_targets: vec![ModelTargetId::new("target")],
        required_capabilities: vec![ModelCapability::Text],
        model_output: ModelOutputSettings {
            max_output_tokens: Some(10),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        recovery_policy: ModelRecoveryPolicy {
            max_context_rebuilds: 0,
            output_limit: OutputLimitAction::Suspend,
            transport: TerminalRecoveryAction::Suspend,
            unavailable: TerminalRecoveryAction::Suspend,
            missing_usage: MissingUsagePolicy::Stop,
        },
        limits: ModelOnlyLimits {
            execution: ExecutionLimits::new(NonZeroU32::new(2).unwrap()),
            max_total_tokens: Some(10),
            deadline_tick: None,
        },
    }
}
