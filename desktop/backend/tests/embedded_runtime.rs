use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use garive_core::{
    MissingUsagePolicy, ModelRecoveryPolicy, OutputLimitAction, TerminalRecoveryAction,
};
use garive_desktop::{
    DesktopHost, DesktopHostConfig, DesktopOperations, DesktopState, DesktopTerminal,
    DesktopWorkspaceContextFile, DesktopWorkspaceExecutionFactory, DesktopWorkspaceGrant,
    DesktopWorkspaceService,
};
use garive_ledger::SessionId;
use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture, ModelItem,
    ModelObserver, ModelOutputSettings, ModelPort, ModelRequest, ModelStopReason, ModelUsage,
    TextMode, TokenCount, UsageSource,
};
use garive_runtime::{
    EffectiveRuntimeLimits, HostClock, InstalledAgent, LiveHostLimits, LocalExecutionAttempt,
    LocalExecutionPolicy, SqliteLedger,
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

struct SuspendingModel(AtomicU64);
impl ModelPort for SuspendingModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                Ok(InvokeOutcome::Interrupted {
                    kind: InterruptionKind::OutputLimit,
                    partial_items: vec![ModelItem::Text {
                        text: "partial".into(),
                    }],
                    usage: usage(),
                })
            } else {
                Ok(InvokeOutcome::Completed {
                    items: vec![ModelItem::Text {
                        text: "resumed answer".into(),
                    }],
                    usage: usage(),
                    stop_reason: ModelStopReason::EndTurn,
                })
            }
        })
    }
}

struct WorkspaceWritingModel {
    calls: AtomicU64,
    arguments: String,
}

impl ModelPort for WorkspaceWritingModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(InvokeOutcome::Completed {
                items: if call < 2 {
                    vec![ModelItem::ToolIntent {
                        model_call_id: format!("write-call-{call}"),
                        tool_name: "write_file".into(),
                        arguments_json: self.arguments.clone(),
                    }]
                } else {
                    vec![ModelItem::Text {
                        text: "artifact committed".into(),
                    }]
                },
                usage: usage(),
                stop_reason: if call < 2 {
                    ModelStopReason::ToolUse
                } else {
                    ModelStopReason::EndTurn
                },
            })
        })
    }
}

fn usage() -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Known(3),
        output_tokens: TokenCount::Known(4),
        cache_read_tokens: None,
        cache_write_tokens: None,
        source: UsageSource::ProviderReported,
    }
}

fn desktop_host(database: &Path, model: Arc<dyn ModelPort>) -> DesktopHost {
    DesktopHost::new(desktop_host_config(database, model)).expect("Desktop Host composition")
}

fn desktop_host_config(database: &Path, model: Arc<dyn ModelPort>) -> DesktopHostConfig {
    DesktopHostConfig {
        database_path: database.to_owned(),
        installed_agent: InstalledAgent {
            definition_id: "definition-main".into(),
            definition_revision: "revision-1".into(),
            snapshot_digest: "a".repeat(64),
            agent_instance_namespace: "desktop-main".into(),
            public_capabilities: Vec::new(),
            runtime_limits: EffectiveRuntimeLimits {
                max_iterations: 4,
                max_input_tokens: Some(64),
                max_output_tokens: Some(16),
                deadline_budget_ms: Some(2_000),
            },
            public_activity_catalogue: None,
        },
        host_limits: LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: None,
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
        model,
        operations: Arc::new(Operations(AtomicU64::new(1))),
    }
}

#[tokio::test]
async fn typed_ipc_core_runs_an_embedded_durable_agent() {
    let directory = tempdir().expect("temp directory");
    let host = desktop_host(
        &directory.path().join("desktop.db"),
        Arc::new(CompletingModel),
    );
    let state = DesktopState::default();
    state.install(host).expect("one install");
    assert_eq!(
        state.capabilities().agent_definition_id.as_deref(),
        Some("definition-main")
    );
    assert!(state.capabilities().durable_navigation);
    assert!(state.capabilities().workspaces);
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

    let recents = state.recent_sessions(8).expect("durable recents");
    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].session_id, result.session_id);
    assert_eq!(recents[0].turn_count, 2);
    let timeline = state
        .session_timeline(&result.session_id, 0, 8)
        .expect("durable timeline");
    assert_eq!(timeline.items.len(), 2);
    assert_eq!(timeline.items[0].user_text, "hello desktop");
    assert_eq!(
        timeline.items[1].completion_text.as_deref(),
        Some("desktop durable answer")
    );
}

#[tokio::test]
async fn restart_safe_partial_output_can_resume_the_same_turn() {
    let directory = tempdir().expect("temp directory");
    let state = DesktopState::default();
    state
        .install(desktop_host(
            &directory.path().join("suspended.db"),
            Arc::new(SuspendingModel(AtomicU64::new(0))),
        ))
        .unwrap();
    let first = state
        .run_turn_isolated("definition-main".into(), "long outcome".into())
        .await
        .unwrap();
    assert_eq!(first.terminal, DesktopTerminal::Suspended);
    let timeline = state.session_timeline(&first.session_id, 0, 8).unwrap();
    let suspension = timeline.items[0].suspension.as_ref().unwrap();
    assert_eq!(suspension.kind, "partial_output");

    let resumed = state
        .continue_turn_isolated(
            first.session_id.clone(),
            first.turn_id.clone(),
            suspension.suspension_id.clone(),
            suspension.session_version,
            "continue".into(),
        )
        .await
        .unwrap();
    assert_eq!(resumed.turn_id, first.turn_id);
    assert_eq!(resumed.terminal, DesktopTerminal::Completed);
    let restored = state.session_timeline(&first.session_id, 0, 8).unwrap();
    assert_eq!(restored.items.len(), 1);
    assert_eq!(
        restored.items[0].completion_text.as_deref(),
        Some("resumed answer")
    );
    assert!(restored.items[0].suspension.is_none());
}

#[tokio::test]
async fn unconfigured_state_is_stable_and_secret_free() {
    let state = DesktopState::default();
    assert!(!state.capabilities().configured);
    assert!(state.capabilities().agent_definition_id.is_none());
    let error = state
        .run_turn("definition", "private input")
        .await
        .expect_err("configuration is required");
    assert_eq!(error.code(), "not_configured");
    assert!(!format!("{error:?}").contains("private input"));
}

#[test]
fn workspace_attachment_survives_desktop_host_restart_without_paths() {
    let directory = tempdir().expect("temp directory");
    let database = directory.path().join("workspace.db");
    let host = desktop_host(&database, Arc::new(CompletingModel));
    let session_id = host.create_session("definition-main").unwrap();
    let grant = DesktopWorkspaceGrant {
        schema_version: 1,
        workspace_id: "workspace-opaque".into(),
        display_name: "Briefs".into(),
        access: "enumerate",
        grant_revision: 1,
        state: "active",
        expires_at: "2026-08-30T12:00:00Z".into(),
    };
    let attached = host.attach_workspace(&session_id, &grant).unwrap();
    assert_eq!(attached.workspace_id, "workspace-opaque");

    let restarted = desktop_host(&database, Arc::new(CompletingModel));
    let restored = restarted.session_workspaces(&session_id).unwrap();
    assert_eq!(restored, vec![attached]);
    let public = serde_json::to_string(&restored).unwrap();
    assert!(!public.contains(directory.path().to_string_lossy().as_ref()));
    assert!(!public.contains("path"));
}

#[tokio::test]
async fn selected_workspace_text_reaches_the_embedded_runtime_without_frontend_content() {
    let directory = tempdir().unwrap();
    let state = DesktopState::default();
    state
        .install(desktop_host(
            &directory.path().join("context.db"),
            Arc::new(CompletingModel),
        ))
        .unwrap();
    let session_id = state.create_session("definition-main").unwrap();
    let grant = DesktopWorkspaceGrant {
        schema_version: 1,
        workspace_id: "workspace-opaque".into(),
        display_name: "Briefs".into(),
        access: "enumerate",
        grant_revision: 1,
        state: "active",
        expires_at: "2026-08-30T12:00:00Z".into(),
    };
    state.attach_workspace(&session_id, &grant).unwrap();
    let result = state
        .run_turn_with_context_isolated(
            "definition-main".into(),
            session_id.clone(),
            "summarize".into(),
            vec![DesktopWorkspaceContextFile {
                workspace_id: grant.workspace_id,
                grant_revision: 1,
                entry_id: "entry-opaque".into(),
                display_name: "brief.md".into(),
                kind: "text",
                content_digest: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
                    .into(),
                content_utf8: "hello".into(),
            }],
        )
        .await
        .unwrap();
    assert_eq!(result.terminal, DesktopTerminal::Completed);
    assert_eq!(result.session_id, session_id);
}

#[tokio::test]
async fn approved_workspace_write_commits_receipt_and_creates_an_atomic_artifact() {
    let directory = tempdir().unwrap();
    let workspace_path = directory.path().join("Workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let workspaces = DesktopWorkspaceService::default();
    let selected = workspaces.admit_selected(&workspace_path, "main").unwrap();
    let writable = workspaces
        .authorize_writes(&selected.workspace_id, &workspace_path, "main")
        .unwrap();
    let database = directory.path().join("governed.db");
    let arguments = serde_json::json!({
        "workspace_id":writable.workspace_id,
        "artifact_name":"result.md",
        "content_utf8":"durable artifact"
    })
    .to_string();
    let model = Arc::new(WorkspaceWritingModel {
        calls: AtomicU64::new(0),
        arguments,
    });
    let factory = Arc::new(
        DesktopWorkspaceExecutionFactory::new(database.clone(), workspaces.clone(), "main")
            .unwrap(),
    );
    let state = DesktopState::default();
    state
        .install(DesktopHost::new_governed(desktop_host_config(&database, model), factory).unwrap())
        .unwrap();
    let session_id = state.create_session("definition-main").unwrap();
    state.attach_workspace(&session_id, &writable).unwrap();

    let suspended = state
        .run_turn_in_session_isolated(
            "definition-main".into(),
            Some(session_id.clone()),
            "create the result".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        suspended.terminal,
        DesktopTerminal::Suspended,
        "{suspended:?}"
    );
    let timeline = state.session_timeline(&session_id, 0, 8).unwrap();
    let approval = timeline.items[0].suspension.as_ref().unwrap();
    assert_eq!(approval.kind, "approval_required");

    let completed = state
        .continue_approval_isolated(
            session_id.clone(),
            suspended.turn_id,
            approval.suspension_id.clone(),
            approval.session_version,
            true,
        )
        .await
        .unwrap();
    assert_eq!(completed.terminal, DesktopTerminal::Completed);
    assert_eq!(completed.text, "artifact committed");
    assert_eq!(
        std::fs::read_to_string(workspace_path.join("result.md")).unwrap(),
        "durable artifact"
    );
    let artifacts = state.artifacts(&session_id, 0, 8).unwrap();
    assert_eq!(artifacts.items.len(), 1);
    let artifact_view = &artifacts.items[0];
    assert_eq!(artifact_view.display_name, "result.md");
    assert_eq!(artifact_view.mime_type, "text/markdown");
    assert_eq!(artifact_view.byte_size, 16);
    assert_eq!(artifact_view.preview, "text");
    assert_eq!(
        artifact_view.workspace_id.as_deref(),
        Some(writable.workspace_id.as_str())
    );
    assert!(!serde_json::to_string(&artifacts)
        .unwrap()
        .contains(directory.path().to_string_lossy().as_ref()));
    let preview = workspaces
        .preview_text_artifact(
            &artifact_view.artifact_id,
            artifact_view.revision,
            artifact_view.workspace_id.as_deref().unwrap(),
            &artifact_view.display_name,
            &artifact_view.content_digest,
            "main",
        )
        .unwrap();
    assert_eq!(preview.content_utf8, "durable artifact");
    assert!(!serde_json::to_string(&preview)
        .unwrap()
        .contains(directory.path().to_string_lossy().as_ref()));
    std::fs::write(workspace_path.join("result.md"), "tampered").unwrap();
    assert_eq!(
        workspaces
            .preview_text_artifact(
                &artifact_view.artifact_id,
                artifact_view.revision,
                artifact_view.workspace_id.as_deref().unwrap(),
                &artifact_view.display_name,
                &artifact_view.content_digest,
                "main",
            )
            .unwrap_err(),
        garive_desktop::DesktopWorkspaceError::Unavailable
    );

    let restarted = desktop_host(&database, Arc::new(CompletingModel));
    assert_eq!(restarted.artifacts(&session_id, 0, 8).unwrap(), artifacts);

    let ledger = SqliteLedger::open(&database).unwrap();
    let session = SessionId::try_from(session_id.as_str()).unwrap();
    let watermark = ledger.session_watermark(&session).unwrap().unwrap();
    let facts = ledger
        .read_facts(&session, 0, watermark.max_position, None)
        .unwrap();
    let kinds = facts
        .iter()
        .map(|fact| fact.kind.as_str().to_owned())
        .collect::<Vec<_>>();
    for required in [
        "interaction.requested",
        "interaction.resolved",
        "effect.authorized",
        "effect.started",
        "effect.receipt",
        "effect.completed",
        "artifact.committed",
        "turn.completed",
    ] {
        assert!(
            kinds.iter().any(|kind| kind == required),
            "missing {required}"
        );
    }
    let completed_position = facts
        .iter()
        .find(|fact| fact.kind.as_str() == "effect.completed")
        .unwrap()
        .position;
    let artifact = facts
        .iter()
        .find(|fact| fact.kind.as_str() == "artifact.committed")
        .unwrap();
    assert_eq!(artifact.position, completed_position + 1);
    assert!(artifact.payload.as_json().contains("artifact-"));
}
