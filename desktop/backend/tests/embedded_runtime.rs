use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use garive_core::{
    MissingUsagePolicy, ModelRecoveryPolicy, OutputLimitAction, TerminalRecoveryAction,
};
use garive_desktop::{
    DesktopHost, DesktopHostConfig, DesktopOperations, DesktopState, DesktopTerminal,
};
use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture, ModelItem, ModelObserver,
    ModelOutputSettings, ModelPort, ModelRequest, ModelStopReason, ModelUsage, TextMode,
    TokenCount, UsageSource,
};
use garive_runtime::{
    EffectiveRuntimeLimits, HostClock, InstalledAgent, LiveHostLimits, LocalExecutionAttempt,
    LocalExecutionPolicy,
};
use tempfile::tempdir;

struct FixedHostClock;
impl HostClock for FixedHostClock {
    fn recorded_at(&self) -> String {
        "2026-08-29T00:00:00Z".into()
    }
}

struct Operations(AtomicU64);
impl DesktopOperations for Operations {
    fn command_id(
        &self,
        purpose: &'static str,
    ) -> Result<String, garive_desktop::DesktopHostError> {
        Ok(format!(
            "desktop-{purpose}-{}",
            self.0.fetch_add(1, Ordering::SeqCst)
        ))
    }

    fn execution_attempt(&self) -> Result<LocalExecutionAttempt, garive_desktop::DesktopHostError> {
        let ordinal = self.0.fetch_add(1, Ordering::SeqCst);
        Ok(LocalExecutionAttempt {
            worker_owner_id: "desktop-worker".into(),
            lease_token: format!("unpredictable-test-token-{ordinal}"),
            now_ms: 1_000 + ordinal,
            lease_duration_ms: 5_000,
            recorded_at: "2026-08-29T00:00:01Z".into(),
        })
    }
}

struct CompletingModel;
impl ModelPort for CompletingModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            assert_eq!(request.target_id.as_str(), "desktop-target");
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text {
                    text: "desktop durable answer".into(),
                }],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(3),
                    output_tokens: TokenCount::Known(4),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: ModelStopReason::EndTurn,
            })
        })
    }
}

#[tokio::test]
async fn typed_ipc_core_runs_an_embedded_durable_agent() {
    let directory = tempdir().expect("temp directory");
    let host = DesktopHost::new(DesktopHostConfig {
        database_path: directory.path().join("desktop.db"),
        installed_agent: InstalledAgent {
            definition_id: "definition-main".into(),
            definition_revision: "revision-1".into(),
            snapshot_digest: "a".repeat(64),
            agent_instance_namespace: "desktop-main".into(),
            runtime_limits: EffectiveRuntimeLimits {
                max_iterations: 2,
                max_input_tokens: Some(64),
                max_output_tokens: Some(16),
                deadline_budget_ms: Some(2_000),
            },
        },
        host_limits: LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
        },
        execution_policy: LocalExecutionPolicy {
            model_target_id: "desktop-target".into(),
            deployment_id: "desktop-deployment".into(),
            recovery_policy_revision: "recovery-1".into(),
            required_capabilities: vec![ModelCapability::Text],
            model_output: ModelOutputSettings {
                max_output_tokens: Some(16),
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
            max_context_utf8_bytes: 2_048,
            max_model_attempts: 1,
        },
        dispatch_capacity: 2,
        host_clock: Arc::new(FixedHostClock),
        model: Arc::new(CompletingModel),
        operations: Arc::new(Operations(AtomicU64::new(1))),
    })
    .expect("Desktop Host composition");
    let state = DesktopState::default();
    state.install(host).expect("one install");
    let result = state
        .run_turn_isolated("definition-main".into(), "hello desktop".into())
        .await
        .expect("durable turn");
    assert_eq!(result.terminal, DesktopTerminal::Completed);
    assert_eq!(result.text, "desktop durable answer");
    assert!(result.cursor > 2);
    assert!(!result.session_id.is_empty());
    assert!(!result.turn_id.is_empty());
    assert!(!result.execution_id.is_empty());

    let continued = state
        .run_turn_in_session_isolated(
            "definition-main".into(),
            Some(result.session_id.clone()),
            "follow-up desktop".into(),
        )
        .await
        .expect("durable follow-up Turn");
    assert_eq!(continued.session_id, result.session_id);
    assert_ne!(continued.turn_id, result.turn_id);
    assert!(continued.cursor > result.cursor);
}

#[tokio::test]
async fn unconfigured_state_is_stable_and_secret_free() {
    let state = DesktopState::default();
    assert!(!state.capabilities().configured);
    let error = state
        .run_turn("definition", "private input")
        .await
        .expect_err("configuration is required");
    assert_eq!(error.code(), "not_configured");
    assert!(!format!("{error:?}").contains("private input"));
}
