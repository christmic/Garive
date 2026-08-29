use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use garive_core::{
    MissingUsagePolicy, ModelRecoveryPolicy, OutputLimitAction, TerminalRecoveryAction,
};
use garive_ledger::{ExecutionId, SessionId, TurnId};
use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture, ModelItem, ModelObserver,
    ModelOutputSettings, ModelPort, ModelRequest, ModelStopReason, ModelUsage, TextMode,
    TokenCount, UsageSource,
};
use garive_runtime::{
    local_dispatch_queue, CommittedTurn, EffectiveRuntimeLimits, HostClock, InstalledAgent,
    LiveHost, LiveHostLimits, LocalExecutionAttempt, LocalExecutionPolicy, LocalExecutionWorker,
    LocalWorkerDisposition, LocalWorkerError,
};
use tempfile::tempdir;

struct Clock;
impl HostClock for Clock {
    fn recorded_at(&self) -> String {
        "2026-08-29T00:00:00Z".into()
    }
}

struct CompletingModel(AtomicUsize);
impl ModelPort for CompletingModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            assert_eq!(request.target_id.as_str(), "target-main");
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text {
                    text: "durable answer".into(),
                }],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(2),
                    output_tokens: TokenCount::Known(3),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: ModelStopReason::EndTurn,
            })
        })
    }
}

fn policy() -> LocalExecutionPolicy {
    LocalExecutionPolicy {
        model_target_id: "target-main".into(),
        deployment_id: "deployment-main".into(),
        recovery_policy_revision: "recovery-1".into(),
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
        max_context_items: 8,
        max_context_utf8_bytes: 1_024,
        max_model_attempts: 1,
    }
}

fn attempt() -> LocalExecutionAttempt {
    LocalExecutionAttempt {
        worker_owner_id: "worker-1".into(),
        lease_token: "unpredictable-test-token".into(),
        now_ms: 1_000,
        lease_duration_ms: 5_000,
        recorded_at: "2026-08-29T00:00:01Z".into(),
    }
}

#[tokio::test]
async fn committed_turn_runs_to_durable_host_terminal_once() {
    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("garive.db");
    let (dispatcher, mut queue) = local_dispatch_queue(1).expect("queue");
    let host = LiveHost::new(
        &database,
        InstalledAgent {
            definition_id: "definition-main".into(),
            definition_revision: "revision-1".into(),
            snapshot_digest: "a".repeat(64),
            agent_instance_namespace: "local-main".into(),
            runtime_limits: EffectiveRuntimeLimits {
                max_iterations: 2,
                max_input_tokens: Some(20),
                max_output_tokens: Some(10),
                deadline_budget_ms: Some(1_000),
            },
        },
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
        },
        Arc::new(Clock),
        dispatcher,
    )
    .expect("host");
    let session = host
        .create_session("create-1", "definition-main")
        .expect("session");
    let turn = host
        .start_turn("start-1", &session.session_id, "hello")
        .expect("turn start committed");

    let model = Arc::new(CompletingModel(AtomicUsize::new(0)));
    let worker = LocalExecutionWorker::new(&database, policy(), model.clone()).expect("worker");
    let disposition = queue
        .try_run_next(&worker, &attempt())
        .await
        .expect("worker terminal");
    let terminal_positions = match disposition {
        LocalWorkerDisposition::TerminalCommitted { positions } => positions,
        LocalWorkerDisposition::AlreadyTerminal => panic!("first dispatch was terminal"),
    };
    assert_eq!(terminal_positions.len(), 2);
    assert!(terminal_positions[0] > turn.committed_position);
    assert_eq!(model.0.load(Ordering::SeqCst), 1);
    let duplicate = CommittedTurn {
        session_id: SessionId::try_from(session.session_id.as_str()).expect("session identity"),
        turn_id: TurnId::try_from(turn.turn_id.as_str()).expect("turn identity"),
        execution_id: ExecutionId::try_from(turn.execution_id.as_str())
            .expect("execution identity"),
        session_version: 2,
        committed_position: turn.committed_position,
    };
    assert_eq!(
        worker.execute(&duplicate, &attempt()).await,
        Ok(LocalWorkerDisposition::AlreadyTerminal)
    );
    assert_eq!(model.0.load(Ordering::SeqCst), 1);

    let page = host
        .read_event_page(&session.session_id, 0)
        .expect("event page");
    assert_eq!(
        page.events
            .iter()
            .map(|event| event.event.as_str())
            .collect::<Vec<_>>(),
        ["session.created", "turn.started", "turn.completed"]
    );
    assert_eq!(
        page.events.last().expect("terminal event").text,
        "durable answer"
    );
    assert!(page.events[2].position > page.events[1].position + 1);
    assert_eq!(
        queue.try_run_next(&worker, &attempt()).await,
        Err(LocalWorkerError::QueueEmpty)
    );
}

#[test]
fn zero_capacity_is_rejected() {
    assert_eq!(
        local_dispatch_queue(0).err().expect("zero capacity"),
        LocalWorkerError::InvalidComposition
    );
}
