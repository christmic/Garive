use std::sync::{Arc, Mutex};

use garive_core::{
    derive_context, CandidateKind, ContextPort, MissingUsagePolicy, ModelRecoveryPolicy,
    OutputLimitAction, Retention, TerminalRecoveryAction,
};
use garive_llm::{ModelCapability, ModelOutputSettings, TextMode};
use garive_runtime::{
    reconstruct_local_start, CommittedTurn, EffectiveRuntimeLimits, HostClock,
    HostWorkspaceContextEntry, InstalledAgent, LiveHost, LiveHostLimits, LocalExecutionAttempt,
    LocalExecutionPolicy, LocalReconstructionError, SqliteLedger, TurnDispatchError,
    TurnDispatcher,
};
use tempfile::tempdir;

struct Clock;
impl HostClock for Clock {
    fn recorded_at(&self) -> String {
        "2026-08-29T00:00:00Z".into()
    }
}

#[derive(Default)]
struct Capture(Mutex<Vec<CommittedTurn>>);
impl TurnDispatcher for Capture {
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        self.0.lock().expect("capture lock").push(turn.clone());
        Ok(())
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

#[test]
fn reconstructs_only_committed_durable_start_values() {
    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("garive.db");
    let capture = Arc::new(Capture::default());
    let host = LiveHost::new(
        &database,
        InstalledAgent {
            definition_id: "definition-main".into(),
            definition_revision: "revision-1".into(),
            snapshot_digest: "a".repeat(64),
            agent_instance_namespace: "local-main".into(),
            public_capabilities: Vec::new(),
            runtime_limits: EffectiveRuntimeLimits {
                max_iterations: 2,
                max_input_tokens: Some(20),
                max_output_tokens: Some(10),
                deadline_budget_ms: Some(1_000),
            },
            public_activity_catalogue: None,
        },
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 32,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(Clock),
        capture.clone(),
    )
    .expect("host");
    let session = host
        .create_session("create-1", "definition-main")
        .expect("session");
    host.start_turn("start-1", &session.session_id, "hello durable")
        .expect("turn start");
    let committed = capture.0.lock().expect("capture")[0].clone();
    let ledger = SqliteLedger::open(&database).expect("ledger");
    let mut reconstructed =
        reconstruct_local_start(&ledger, &committed, &policy(), &attempt()).expect("reconstruct");

    assert_eq!(
        reconstructed.request.entry,
        garive_core::AgentEntry::Start {
            trusted_input: "hello durable".into(),
        }
    );
    assert_eq!(reconstructed.request.context_request.through_position, 4);
    assert_eq!(reconstructed.request.limits.max_total_tokens, Some(30));
    assert_eq!(reconstructed.request.limits.deadline_tick, Some(2_000));
    assert_eq!(reconstructed.durable.expected_session_version, 2);
    assert_eq!(reconstructed.durable.lease.now_ms, 1_000);
    let candidates = reconstructed
        .context
        .read_candidates(&reconstructed.request.context_request, 0)
        .expect("context");
    let surface =
        derive_context(&reconstructed.request.context_request, &candidates).expect("surface");
    assert_eq!(surface.item_count, 1);
    assert_eq!(surface.utf8_bytes, "hello durable".len());
    assert_eq!(surface.retained_refs[0].position, 3);
}

#[test]
fn invalid_explicit_values_and_uncommitted_prefix_fail_before_model() {
    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("garive.db");
    let capture = Arc::new(Capture::default());
    let host = LiveHost::new(
        &database,
        InstalledAgent {
            definition_id: "definition-main".into(),
            definition_revision: "revision-1".into(),
            snapshot_digest: "a".repeat(64),
            agent_instance_namespace: "local-main".into(),
            public_capabilities: Vec::new(),
            runtime_limits: EffectiveRuntimeLimits {
                max_iterations: 2,
                max_input_tokens: Some(20),
                max_output_tokens: Some(10),
                deadline_budget_ms: None,
            },
            public_activity_catalogue: None,
        },
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 32,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(Clock),
        capture.clone(),
    )
    .expect("host");
    let session = host
        .create_session("create-1", "definition-main")
        .expect("session");
    host.start_turn("start-1", &session.session_id, "hello")
        .expect("turn start");
    let committed = capture.0.lock().expect("capture")[0].clone();
    let ledger = SqliteLedger::open(&database).expect("ledger");

    let mut invalid = policy();
    invalid.max_context_items = 0;
    assert_eq!(
        reconstruct_local_start(&ledger, &committed, &invalid, &attempt())
            .err()
            .expect("invalid policy"),
        LocalReconstructionError::InvalidComposition
    );
    let mut incomplete = committed;
    incomplete.committed_position = 3;
    assert_eq!(
        reconstruct_local_start(&ledger, &incomplete, &policy(), &attempt())
            .err()
            .expect("execution was not committed in prefix"),
        LocalReconstructionError::ReconstructionFailed
    );
}

#[test]
fn reconstructs_selected_workspace_text_as_required_knowledge_before_user_input() {
    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("workspace.db");
    let capture = Arc::new(Capture::default());
    let host = LiveHost::new(
        &database,
        InstalledAgent {
            definition_id: "definition-main".into(),
            definition_revision: "revision-1".into(),
            snapshot_digest: "a".repeat(64),
            agent_instance_namespace: "local-main".into(),
            public_capabilities: Vec::new(),
            runtime_limits: EffectiveRuntimeLimits {
                max_iterations: 2,
                max_input_tokens: Some(20),
                max_output_tokens: Some(10),
                deadline_budget_ms: None,
            },
            public_activity_catalogue: None,
        },
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 32,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(Clock),
        capture.clone(),
    )
    .unwrap();
    let session = host
        .create_session("create-context", "definition-main")
        .unwrap();
    host.attach_workspace(
        "attach-context",
        &session.session_id,
        "workspace-opaque",
        "Briefs",
        1,
        "enumerate",
    )
    .unwrap();
    host.start_turn_with_workspace_context(
        "start-context",
        &session.session_id,
        "summarize this",
        "workspace-opaque",
        1,
        &[HostWorkspaceContextEntry {
            entry_id: "entry-opaque".into(),
            display_name: "brief.md".into(),
            kind: "text".into(),
            content_digest: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
                .into(),
            content_utf8: "hello".into(),
        }],
    )
    .unwrap();
    let committed = capture.0.lock().unwrap()[0].clone();
    let ledger = SqliteLedger::open(&database).unwrap();
    let mut reconstructed =
        reconstruct_local_start(&ledger, &committed, &policy(), &attempt()).unwrap();
    let candidates = reconstructed
        .context
        .read_candidates(&reconstructed.request.context_request, 0)
        .unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].kind, CandidateKind::Knowledge);
    assert_eq!(candidates[0].retention, Retention::Required);
    assert_eq!(candidates[0].fact_ref.position, 3);
    assert_eq!(candidates[1].kind, CandidateKind::UserInput);
    assert_eq!(candidates[1].fact_ref.position, 5);
    let encoded = format!("{:?}", candidates[0].items);
    assert!(encoded.contains("garive.workspace_file"));
    assert!(encoded.contains("hello"));
    assert!(!encoded.contains('/'));
}
