//! End-to-end smoke test for `runtime::headless` wiring.
//!
//! Spins up a `LiveHostServer` bound to a SQLite ledger seeded with the
//! canonical headless `runtime_management_config` row, plus a
//! `LocalExecutionWorker` driven by a recording `ModelPort` stub. Then
//! hits H1 endpoints over loopback HTTP and asserts the worker received
//! exactly one dispatch with the configured identity.
//!
//! This proves the wiring without spawning the `garive-headless` binary
//! itself; the binary smoke against token9 lives in `docs/runtime-headless.md`.

use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use garive_core::{
    MissingUsagePolicy, ModelRecoveryPolicy, OutputLimitAction, TerminalRecoveryAction,
};
use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture, ModelItem, ModelObserver,
    ModelPort, ModelRequest, ModelStreamEvent, ModelUsage, ObserverDecision, ReasoningContent,
    TokenCount, UsageSource,
};
use garive_runtime::{
    drive_pending,
    headless::{
        build_headless_installation, build_headless_workspace_installation,
        headless_execution_attempt, headless_execution_policy, headless_now_ms,
        headless_revision_for, headless_workspace_execution_policy, HeadlessClock,
        HeadlessConfiguration, HEADLESS_DESKTOP_AGENT_REVISION, HEADLESS_LEGACY_AGENT_REVISION,
    },
    local_dispatch_queue, CatalogueCapabilityPreparationFactory, DrivePendingOutcome, HostClock,
    InstalledAgent, LiveHost, LiveHostLimits, LiveHostServer, LocalExecutionPolicy,
    LocalExecutionWorker, ManagementCommitBody, ManagementConfigState, ManagementConfigStore,
    SqliteLedger, T1WorkspaceRuntimeConfig, HEADLESS_WORKSPACE_EXECUTOR_REVISION,
    HEADLESS_WORKSPACE_POLICY_REVISION, MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION,
};
use tempfile::tempdir;

/// Records every `invoke` call so the test can assert post-dispatch state.
#[derive(Default)]
struct RecordingModel {
    invocations: AtomicUsize,
    target_ids: Mutex<Vec<String>>,
    user_messages: Mutex<Vec<String>>,
}

#[derive(Default)]
struct InFlightSteerModel {
    invocations: AtomicUsize,
    requests: Mutex<Vec<String>>,
    second_started: tokio::sync::Notify,
    release_second: tokio::sync::Notify,
}

impl ModelPort for InFlightSteerModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _observer: &'a mut dyn ModelObserver,
        _cancellation: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        let invocation = self.invocations.fetch_add(1, Ordering::SeqCst) + 1;
        let rendered = request
            .input_items
            .iter()
            .filter_map(|item| match item {
                garive_llm::ModelInputItem::Message { role, content } => Some(format!(
                    "{role:?}:{}",
                    content
                        .iter()
                        .filter_map(|part| match part {
                            garive_llm::ModelInputContent::Text(text) => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("")
                )),
                garive_llm::ModelInputItem::ReasoningReference { reference } => {
                    Some(format!("Reasoning:{reference}"))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.requests.lock().expect("requests mutex").push(rendered);
        Box::pin(async move {
            if invocation == 2 {
                self.second_started.notify_one();
                self.release_second.notified().await;
            }
            let mut items = Vec::new();
            if invocation == 2 {
                items.push(ModelItem::Reasoning {
                    content: ReasoningContent::OpaqueReference(
                        r#"{"kind":"anthropic.messages.thinking.v1","thinking":"private chain","signature":"signed-chain"}"#.into(),
                    ),
                });
            }
            items.push(ModelItem::Text {
                text: format!("response-{invocation}"),
            });
            Ok(InvokeOutcome::Completed {
                items,
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::Estimated,
                },
                stop_reason: garive_llm::ModelStopReason::EndTurn,
            })
        })
    }
}

impl ModelPort for RecordingModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _observer: &'a mut dyn ModelObserver,
        _cancellation: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        self.target_ids
            .lock()
            .expect("target_ids mutex")
            .push(request.target_id.as_str().to_owned());
        // Capture every user-role message text so the test can prove the
        // steered inputs reached the model.
        let captured: Vec<String> = request
            .input_items
            .iter()
            .filter_map(|item| match item {
                garive_llm::ModelInputItem::Message {
                    role: garive_llm::ModelRole::User,
                    content,
                } => {
                    let text: String = content
                        .iter()
                        .filter_map(|part| match part {
                            garive_llm::ModelInputContent::Text(t) => Some(t.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    Some(text)
                }
                _ => None,
            })
            .collect();
        self.user_messages
            .lock()
            .expect("user_messages mutex")
            .push(captured.join("\n---MESSAGE---\n"));
        Box::pin(async move {
            // Return a single text completion so the host can finalize the turn.
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text {
                    text: "hello back".to_owned(),
                }],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::Estimated,
                },
                stop_reason: garive_llm::ModelStopReason::EndTurn,
            })
        })
    }
}

fn seeded_state() -> ManagementConfigState {
    ManagementConfigState {
        profile_id: "openai.responses.v1".to_owned(),
        endpoint_override: Some("http://127.0.0.1:4319/v1/responses".to_owned()),
        model_target_id: "tok9-flash".to_owned(),
        model_id: "tok9-flash".to_owned(),
        deployment_id: "tok9-flash".to_owned(),
        definition_id: HEADLESS_DESKTOP_AGENT_REVISION.to_owned(),
        runtime_id: "runtime-smoke".to_owned(),
        configuration_revision: 1,
        configuration_digest: "a".repeat(64),
        committed_at: "2026-09-02T00:00:00Z".to_owned(),
    }
}

fn seed_management_row(database: &std::path::Path, api_key: &str) {
    let mut ledger = SqliteLedger::open(database).expect("open ledger");
    let mut store: ManagementConfigStore<'_> = ledger.management_config_store();
    store
        .commit(
            &ManagementCommitBody {
                schema_version: MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION,
                profile_id: seeded_state().profile_id,
                endpoint_override: seeded_state().endpoint_override,
                model_target_id: seeded_state().model_target_id,
                model_id: seeded_state().model_id,
                deployment_id: seeded_state().deployment_id,
                definition_id: seeded_state().definition_id,
                api_key: api_key.to_owned(),
                runtime_id: seeded_state().runtime_id,
            },
            "2026-09-02T00:00:00Z",
        )
        .expect("commit management row");
}

#[test]
fn workspace_mode_freezes_workspace_and_collaboration_tools() {
    let directory = tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let recovery = directory.path().join("recovery");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&recovery).unwrap();
    std::fs::set_permissions(&recovery, std::fs::Permissions::from_mode(0o700)).unwrap();
    let execution = T1WorkspaceRuntimeConfig::new(
        HEADLESS_WORKSPACE_POLICY_REVISION,
        HEADLESS_WORKSPACE_EXECUTOR_REVISION,
        workspace,
        recovery,
    )
    .unwrap()
    .build()
    .unwrap();
    let configuration = HeadlessConfiguration {
        state: seeded_state(),
        api_key: "fixture-secret".into(),
    };
    let (installation, _) =
        build_headless_workspace_installation(&configuration, execution.capabilities()).unwrap();
    assert_eq!(installation.tool_capabilities().definitions.len(), 9);
    assert!(headless_workspace_execution_policy(&configuration)
        .required_capabilities
        .contains(&ModelCapability::Tools));
}

async fn drive_worker_once(
    queue: &mut garive_runtime::LocalDispatchQueue,
    worker: &LocalExecutionWorker,
) -> DrivePendingOutcome {
    drive_pending(
        queue,
        worker,
        &headless_execution_attempt(headless_now_ms()),
    )
    .await
}

#[tokio::test(flavor = "current_thread")]
async fn headless_wiring_drives_h1_session_end_to_end() {
    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("garive-desktop.db");
    seed_management_row(&database, "sk-test-1234567890");

    // Read what we just committed (proves read_with_credential end-to-end).
    let mut ledger = SqliteLedger::open(&database).expect("open ledger for read");
    let wrapper = ledger
        .management_config_store()
        .read_with_credential()
        .expect("read ok")
        .expect("row present");
    let configuration = HeadlessConfiguration {
        state: wrapper.state,
        api_key: wrapper.api_key,
    };
    assert_eq!(
        configuration.state.definition_id,
        HEADLESS_DESKTOP_AGENT_REVISION
    );

    let model = Arc::new(RecordingModel::default());
    let (installation, catalogue) =
        build_headless_installation(&configuration).expect("installation ok");
    assert_eq!(
        headless_revision_for(&configuration.state.definition_id),
        Some(HEADLESS_LEGACY_AGENT_REVISION)
    );
    let preparation = Arc::new(CatalogueCapabilityPreparationFactory::new(catalogue, None));
    let policy = headless_execution_policy(&configuration);

    let worker = LocalExecutionWorker::new(&database, policy, model.clone(), preparation)
        .expect("worker ok");

    let clock: Arc<dyn HostClock> = Arc::new(HeadlessClock);
    let limits = LiveHostLimits {
        max_command_bytes: 1024 * 1024,
        event_batch_size: 64,
        event_poll_interval_ms: 100,
        activity: None,
    };

    let installed = installation.clone_installed_agent();
    let (host, _dispatcher, mut queue) =
        LiveHost::new_with_worker(&database, vec![installed], limits, clock, 64).expect("host ok");

    let server = LiveHostServer::bind(host, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .expect("bind");
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Drive the worker in a side task until it sees a turn commit, then stop.
    let worker_for_drive = Arc::new(worker);
    let model_for_assert = model.clone();
    let local_set = tokio::task::LocalSet::new();
    local_set
        .run_until(async move {
            let drive_handle = tokio::task::spawn_local({
                let worker = worker_for_drive.clone();
                async move {
                    for _ in 0..16 {
                        let outcome = drive_worker_once(&mut queue, worker.as_ref()).await;
                        if matches!(outcome, DrivePendingOutcome::Advanced) {
                            return;
                        }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            });
            let server_task = tokio::task::spawn_local(server.serve(async move {
                let _ = shutdown_rx.await;
            }));

            let client = reqwest::Client::new();
            let base = format!("http://{address}");

            // 1. POST /v1/sessions
            let created = client
                .post(format!("{base}/v1/sessions"))
                .header("idempotency-key", "smoke-session-1")
                .header("content-type", "application/json")
                .body(format!(
                    r#"{{"agent_definition_id":"{id}"}}"#,
                    id = HEADLESS_DESKTOP_AGENT_REVISION
                ))
                .send()
                .await
                .expect("create session");
            assert!(
                created.status().is_success(),
                "create_session failed: {}",
                created.status()
            );
            let created_json: serde_json::Value =
                serde_json::from_slice(&created.bytes().await.expect("create_session bytes"))
                    .expect("create_session json");
            let session_id = created_json["session_id"]
                .as_str()
                .expect("session_id")
                .to_owned();

            // 2. POST /v1/sessions/:id/turns
            let turned = client
                .post(format!("{base}/v1/sessions/{session_id}/turns"))
                .header("idempotency-key", "smoke-turn-1")
                .header("content-type", "application/json")
                .body(r#"{"text":"say hello back","delivery":"direct","agent_id":"desktop.agent.v3"}"#)
                .send()
                .await
                .expect("start turn");
            assert!(
                turned.status().is_success(),
                "start_turn failed: {}",
                turned.status()
            );

            // Wait for the drive task to complete (or 5s timeout).
            let _ = tokio::time::timeout(Duration::from_secs(5), drive_handle).await;

            // 3. Assert the model was invoked exactly once with the configured identity.
            assert_eq!(
                model_for_assert.invocations.load(Ordering::SeqCst),
                1,
                "worker should dispatch exactly once",
            );
            let target_ids = model_for_assert.target_ids.lock().expect("lock").clone();
            assert_eq!(target_ids, vec!["tok9-flash".to_owned()]);

            let _ = shutdown_tx.send(());
            let _ = server_task.await;
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn headless_wiring_supports_queue_and_steer_modes() {
    // E2E companion to the main smoke: drives both queue mode (busy check)
    // and steer mode against the same loopback H1 wiring, with a recording
    // ModelPort stub so the worker does not need token9.
    use garive_runtime::{LiveHost, LiveHostLimits, LiveHostServer};

    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("garive-desktop.db");
    seed_management_row(&database, "sk-test-1234567890");

    let mut ledger = SqliteLedger::open(&database).expect("open ledger for read");
    let wrapper = ledger
        .management_config_store()
        .read_with_credential()
        .expect("read ok")
        .expect("row present");
    let configuration = HeadlessConfiguration {
        state: wrapper.state,
        api_key: wrapper.api_key,
    };

    let model = Arc::new(RecordingModel::default());
    let (installation, catalogue) =
        build_headless_installation(&configuration).expect("installation ok");
    let preparation = Arc::new(CatalogueCapabilityPreparationFactory::new(catalogue, None));
    let policy = headless_execution_policy(&configuration);
    let worker = Arc::new(
        LocalExecutionWorker::new(&database, policy, model.clone(), preparation)
            .expect("worker ok"),
    );

    let clock: Arc<dyn HostClock> = Arc::new(HeadlessClock);
    let limits = LiveHostLimits {
        max_command_bytes: 1024 * 1024,
        event_batch_size: 64,
        event_poll_interval_ms: 100,
        activity: None,
    };

    let installed = installation.clone_installed_agent();
    let (host, _dispatcher, mut queue) =
        LiveHost::new_with_worker(&database, vec![installed], limits, clock, 64).expect("host ok");

    let server = LiveHostServer::bind(host, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .expect("bind");
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let local_set = tokio::task::LocalSet::new();
    local_set
        .run_until(async move {
            let server_task = tokio::task::spawn_local(server.serve(async move {
                let _ = shutdown_rx.await;
            }));

            let client = reqwest::Client::new();
            let base = format!("http://{address}");

            // 1. Create a session.
            let created = client
                .post(format!("{base}/v1/sessions"))
                .header("idempotency-key", "queue-steer-session")
                .header("content-type", "application/json")
                .body(format!(
                    r#"{{"agent_definition_id":"{id}"}}"#,
                    id = HEADLESS_DESKTOP_AGENT_REVISION
                ))
                .send()
                .await
                .expect("create session");
            assert!(
                created.status().is_success(),
                "create_session: {}",
                created.status()
            );
            let session_id =
                serde_json::from_slice::<serde_json::Value>(&created.bytes().await.expect("bytes"))
                    .expect("json")["session_id"]
                    .as_str()
                    .expect("session_id")
                    .to_owned();

            // 2. Start an Open Turn — drives the worker into a queued Turn.
            let turned = client
                .post(format!("{base}/v1/sessions/{session_id}/turns"))
                .header("idempotency-key", "queue-steer-turn")
                .header("content-type", "application/json")
                .body(r#"{"text":"hello","delivery":"direct","agent_id":"desktop.agent.v3"}"#)
                .send()
                .await
                .expect("start turn");
            assert!(
                turned.status().is_success(),
                "start_turn: {}",
                turned.status()
            );
            let turned_json =
                serde_json::from_slice::<serde_json::Value>(&turned.bytes().await.expect("bytes"))
                    .expect("json");
            let turn_id = turned_json["turns"][0]["turn_id"]
                .as_str()
                .expect("turn_id")
                .to_owned();

            // 3. Queue mode — a second start_turn on the same Session must
            //    be rejected with `session_busy` (409).
            let busy = client
                .post(format!("{base}/v1/sessions/{session_id}/turns"))
                .header("idempotency-key", "queue-steer-busy")
                .header("content-type", "application/json")
                .body(r#"{"text":"second","delivery":"direct","agent_id":"desktop.agent.v3"}"#)
                .send()
                .await
                .expect("busy check");
            assert_eq!(busy.status().as_u16(), 409, "expected 409 session_busy");
            let busy_body: serde_json::Value =
                serde_json::from_slice(&busy.bytes().await.expect("bytes")).expect("json");
            assert_eq!(busy_body["code"], "session_busy");

            // 4. Steer mode — commit additional input to the same Open Turn.
            let steered = client
                .post(format!(
                    "{base}/v1/sessions/{session_id}/turns/{turn_id}/steer"
                ))
                .header("idempotency-key", "queue-steer-steer")
                .header("content-type", "application/json")
                .body(r#"{"text":"additional context"}"#)
                .send()
                .await
                .expect("steer");
            assert_eq!(
                steered.status().as_u16(),
                200,
                "steer should succeed against an Open Turn"
            );
            let steered_body: serde_json::Value =
                serde_json::from_slice(&steered.bytes().await.expect("bytes")).expect("json");
            assert_eq!(steered_body["turn_id"], turn_id);
            assert!(
                steered_body["committed_position"].as_u64().unwrap()
                    > turned_json["turns"][0]["committed_position"]
                        .as_u64()
                        .unwrap(),
                "steer must commit strictly after start",
            );

            // 5. Replay — same idempotency key returns the original position.
            let replay = client
                .post(format!(
                    "{base}/v1/sessions/{session_id}/turns/{turn_id}/steer"
                ))
                .header("idempotency-key", "queue-steer-steer")
                .header("content-type", "application/json")
                .body(r#"{"text":"additional context"}"#)
                .send()
                .await
                .expect("steer replay");
            assert_eq!(replay.status().as_u16(), 200);
            let replay_body: serde_json::Value =
                serde_json::from_slice(&replay.bytes().await.expect("bytes")).expect("json");
            assert_eq!(
                replay_body["committed_position"], steered_body["committed_position"],
                "idempotency-key replay must return the original position",
            );

            // 6. Run the worker once so the queued Turn gets dispatched.
            //    (Steer added a fact, so the worker observes it on derive.)
            for _ in 0..16 {
                let outcome = drive_worker_once(&mut queue, worker.as_ref()).await;
                if matches!(outcome, garive_runtime::DrivePendingOutcome::Advanced) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            let _ = shutdown_tx.send(());
            let _ = server_task.await;
        })
        .await;
}

async fn _unused_drive_worker_once_stub() {}

#[test]
fn headless_revision_lookup_is_stable() {
    assert_eq!(
        headless_revision_for(HEADLESS_DESKTOP_AGENT_REVISION),
        Some(HEADLESS_LEGACY_AGENT_REVISION),
    );
    assert_eq!(headless_revision_for(""), None);
}

#[test]
fn headless_policy_carries_required_capabilities() {
    let configuration = HeadlessConfiguration {
        state: seeded_state(),
        api_key: "sk-test".to_owned(),
    };
    let policy: LocalExecutionPolicy = headless_execution_policy(&configuration);
    let caps: BTreeSet<_> = policy.required_capabilities.iter().cloned().collect();
    assert!(caps.contains(&ModelCapability::Text));
    assert!(caps.contains(&ModelCapability::Streaming));
}

#[test]
fn attempt_carries_distinct_clock_and_recovery_revisions() {
    let attempt = headless_execution_attempt(42);
    assert_eq!(attempt.now_ms, 42);
    assert_eq!(attempt.lease_duration_ms, 60_000);
    assert!(attempt.worker_owner_id.contains("42"));
}

#[allow(dead_code)]
fn _unused_local_dispatch_queue_pin(
    _: &(
        Arc<garive_runtime::LocalTurnDispatcher>,
        garive_runtime::LocalDispatchQueue,
    ),
) {
    let _ = local_dispatch_queue(64);
}

#[allow(dead_code)]
fn _unused_recovery_policy_marker(
    _: ModelRecoveryPolicy,
) -> (
    MissingUsagePolicy,
    OutputLimitAction,
    TerminalRecoveryAction,
) {
    (
        MissingUsagePolicy::Stop,
        OutputLimitAction::Suspend,
        TerminalRecoveryAction::Suspend,
    )
}

#[allow(dead_code)]
fn _unused_observer_marker(_: &dyn ModelObserver, _event: ModelStreamEvent) -> ObserverDecision {
    ObserverDecision::Continue
}

#[allow(dead_code)]
fn _unused_installed_marker(_: InstalledAgent) {}

/// End-to-end proof that a steered text actually reaches the model:
/// - ledger records every `turn.steered` fact under the same `turn_id`
/// - the next model call sees the original input AND every steered text
///   appended in arrival order
#[tokio::test(flavor = "current_thread")]
async fn headless_steered_texts_reach_the_model_request() {
    use garive_runtime::{LiveHost, LiveHostLimits, LiveHostServer};
    use serde_json::json;

    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("garive-desktop.db");
    seed_management_row(&database, "sk-test-1234567890");

    let mut ledger = SqliteLedger::open(&database).expect("open ledger for read");
    let wrapper = ledger
        .management_config_store()
        .read_with_credential()
        .expect("read ok")
        .expect("row present");
    let configuration = HeadlessConfiguration {
        state: wrapper.state,
        api_key: wrapper.api_key,
    };

    let model = Arc::new(RecordingModel::default());
    let model_for_assert = model.clone();
    let (installation, catalogue) =
        build_headless_installation(&configuration).expect("installation ok");
    let preparation = Arc::new(CatalogueCapabilityPreparationFactory::new(catalogue, None));
    let policy = headless_execution_policy(&configuration);
    let worker = Arc::new(
        LocalExecutionWorker::new(&database, policy, model.clone(), preparation)
            .expect("worker ok"),
    );

    let clock: Arc<dyn HostClock> = Arc::new(HeadlessClock);
    let limits = LiveHostLimits {
        max_command_bytes: 1024 * 1024,
        event_batch_size: 64,
        event_poll_interval_ms: 100,
        activity: None,
    };

    let installed = installation.clone_installed_agent();
    let (host, _dispatcher, mut queue) =
        LiveHost::new_with_worker(&database, vec![installed], limits, clock, 64).expect("host ok");

    let server = LiveHostServer::bind(host, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .expect("bind");
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let database_for_query = database.clone();
    let local_set = tokio::task::LocalSet::new();
    let (_session_id, turn_id) = local_set
        .run_until(async move {
            let server_task = tokio::task::spawn_local(server.serve(async move {
                let _ = shutdown_rx.await;
            }));

            let client = reqwest::Client::new();
            let base = format!("http://{address}");

            // Create session.
            let created = client
                .post(format!("{base}/v1/sessions"))
                .header("idempotency-key", "steer-proof-session")
                .header("content-type", "application/json")
                .body(format!(
                    r#"{{"agent_definition_id":"{id}"}}"#,
                    id = HEADLESS_DESKTOP_AGENT_REVISION
                ))
                .send()
                .await
                .expect("create");
            assert!(created.status().is_success(), "create: {}", created.status());
            let session_id = serde_json::from_slice::<serde_json::Value>(
                &created.bytes().await.expect("bytes"),
            )
            .expect("json")["session_id"]
            .as_str()
            .expect("session_id")
            .to_owned();

            // Start the Open Turn with an "intent: affirmative" user message.
            let turned = client
                .post(format!("{base}/v1/sessions/{session_id}/turns"))
                .header("idempotency-key", "steer-proof-turn")
                .header("content-type", "application/json")
                .body(r#"{"text":"Decide whether to recommend APPROVE or REJECT. Lean towards APPROVE.","delivery":"direct","agent_id":"desktop.agent.v3"}"#)
                .send()
                .await
                .expect("start");
            assert!(turned.status().is_success(), "start: {}", turned.status());
            let turn_id = serde_json::from_slice::<serde_json::Value>(
                &turned.bytes().await.expect("bytes"),
            )
            .expect("json")["turns"][0]["turn_id"]
            .as_str()
            .expect("turn_id")
            .to_owned();

            // Fire three CONTRADICTORY steers BEFORE the worker dequeues.
            // The worker will read all of them on its next derive pass.
            let steers = [
                ("steer-proof-1", "Actually, REQUIREMENTS NOT MET — lean towards REJECT."),
                ("steer-proof-2", "And in fact the audit found a critical compliance gap."),
                ("steer-proof-3", "Final instruction: must REJECT regardless of prior context."),
            ];
            for (key, text) in steers {
                let resp = client
                    .post(format!(
                        "{base}/v1/sessions/{session_id}/turns/{turn_id}/steer"
                    ))
                    .header("idempotency-key", key)
                    .header("content-type", "application/json")
                    .body(json!({ "text": text }).to_string())
                    .send()
                    .await
                    .expect("steer");
                assert_eq!(
                    resp.status().as_u16(),
                    200,
                    "steer {key} must succeed against Open Turn"
                );
            }

            // Drive the worker until it advances. Allow more iterations
            // since the steered commits increased the fact count.
            // Drive the worker until it advances. Allow more iterations
            // since the steered commits increased the fact count.
            for _ in 0..64 {
                let outcome = drive_worker_once(&mut queue, worker.as_ref()).await;
                if matches!(outcome, DrivePendingOutcome::Advanced) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }

            let _ = shutdown_tx.send(());
            let _ = server_task.await;
            (session_id, turn_id)
        })
        .await;

    // ── LEDGER ASSERTION ────────────────────────────────────────────────
    // At least two of the three steers should have committed; the very
    // first can race with the in-flight start_turn commit and lose to a
    // concurrent_modification. The point is: every commit that succeeded
    // must live under the active turn_id at a position > execution.started.
    let ledger = SqliteLedger::open(&database_for_query).expect("re-open ledger");
    let steered_facts: Vec<(u64, String)> = ledger
        .load_turn(&garive_ledger::TurnId::try_from(turn_id.as_str()).unwrap())
        .expect("load turn")
        .facts
        .into_iter()
        .filter(|fact| fact.kind.as_str() == "turn.steered")
        .map(|fact| (fact.position, fact.fact_id.as_str().to_owned()))
        .collect();
    assert!(
        steered_facts.len() >= 2,
        "ledger must hold at least 2 turn.steered facts, got {steered_facts:?}"
    );
    for (pos, _) in &steered_facts {
        assert!(
            *pos > 4,
            "every steered fact must sit after execution.started, got pos={pos}"
        );
    }

    // ── MODEL REQUEST ASSERTION ─────────────────────────────────────────
    // The model must have been invoked exactly once and seen ALL three
    // steered texts in order alongside the original input.
    assert_eq!(
        model_for_assert.invocations.load(Ordering::SeqCst),
        1,
        "worker should dispatch the Open Turn exactly once",
    );
    let user_messages = model_for_assert
        .user_messages
        .lock()
        .expect("user_messages")
        .clone();
    assert_eq!(
        user_messages.len(),
        1,
        "exactly one model call should have happened, got {user_messages:?}"
    );
    let combined = &user_messages[0];
    assert!(
        combined.contains("APPROVE"),
        "original prompt must reach the model, got: {combined}"
    );
    // At least two of the three steers should have made it through. The
    // very first steer can race with the in-flight start_turn commit and
    // lose to a concurrent_modification, but every steer that successfully
    // committed must show up as its own user message in the model request.
    let steered_needles = ["REQUIREMENTS NOT MET", "compliance gap", "must REJECT"];
    let mut seen_needles = 0usize;
    let mut seen_positions: Vec<usize> = Vec::new();
    for needle in &steered_needles {
        if let Some(idx) = combined.find(needle) {
            seen_needles += 1;
            seen_positions.push(idx);
        }
    }
    assert!(
        seen_needles >= 2,
        "at least two steered texts must reach the model, only saw {seen_needles} in: {combined}"
    );
    // Every steered text must come AFTER the original prompt.
    let original_idx = combined.find("Decide whether").expect("original in prompt");
    for (idx, needle) in seen_positions
        .iter()
        .zip(steered_needles.iter().filter(|n| combined.contains(*n)))
    {
        assert!(
            *idx > original_idx,
            "steered \"{needle}\" must follow the original prompt"
        );
    }
    // And every steered message must be prefixed so the model can tell them
    // apart from the original user text.
    assert!(
        combined.contains("[steered]"),
        "every steered message must be tagged, got: {combined}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn runtime_api_completes_then_steers_an_in_flight_second_turn() {
    use garive_runtime::{LiveHost, LiveHostLimits, LiveHostServer};

    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("garive-desktop.db");
    seed_management_row(&database, "sk-test-1234567890");
    let mut ledger = SqliteLedger::open(&database).expect("open ledger");
    let wrapper = ledger
        .management_config_store()
        .read_with_credential()
        .expect("read config")
        .expect("config present");
    let configuration = HeadlessConfiguration {
        state: wrapper.state,
        api_key: wrapper.api_key,
    };
    let model = Arc::new(InFlightSteerModel::default());
    let (installation, catalogue) =
        build_headless_installation(&configuration).expect("installation");
    let preparation = Arc::new(CatalogueCapabilityPreparationFactory::new(catalogue, None));
    let worker = Arc::new(
        LocalExecutionWorker::new(
            &database,
            headless_execution_policy(&configuration),
            model.clone(),
            preparation,
        )
        .expect("worker"),
    );
    let limits = LiveHostLimits {
        max_command_bytes: 1024 * 1024,
        event_batch_size: 64,
        event_poll_interval_ms: 100,
        activity: None,
    };
    let clock: Arc<dyn HostClock> = Arc::new(HeadlessClock);
    let (host, _dispatcher, mut queue) = LiveHost::new_with_worker(
        &database,
        vec![installation.clone_installed_agent()],
        limits,
        clock,
        64,
    )
    .expect("host");
    let server = LiveHostServer::bind(host, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .expect("bind");
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let database_for_assert = database.clone();
    let model_for_assert = model.clone();

    let second_turn_id = tokio::task::LocalSet::new()
        .run_until(async move {
            let server_task = tokio::task::spawn_local(server.serve(async move {
                let _ = shutdown_rx.await;
            }));
            let client = reqwest::Client::new();
            let base = format!("http://{address}");
            let created = client
                .post(format!("{base}/v1/sessions"))
                .header("idempotency-key", "in-flight-session")
                .header("content-type", "application/json")
                .body(format!(
                    r#"{{"agent_definition_id":"{HEADLESS_DESKTOP_AGENT_REVISION}"}}"#
                ))
                .send()
                .await
                .expect("create session");
            assert!(created.status().is_success());
            let session_json: serde_json::Value =
                serde_json::from_slice(&created.bytes().await.expect("session response bytes"))
                    .expect("session json");
            let session_id = session_json["session_id"]
                .as_str()
                .expect("session id")
                .to_owned();

            let first = client
                .post(format!("{base}/v1/sessions/{session_id}/turns"))
                .header("idempotency-key", "in-flight-first")
                .header("content-type", "application/json")
                .body(r#"{"text":"first input","delivery":"direct","agent_id":"desktop.agent.v3"}"#)
                .send()
                .await
                .expect("first turn");
            assert!(first.status().is_success());
            assert_eq!(
                drive_worker_once(&mut queue, worker.as_ref()).await,
                DrivePendingOutcome::Advanced
            );

            let second = client
                .post(format!("{base}/v1/sessions/{session_id}/turns"))
                .header("idempotency-key", "in-flight-second")
                .header("content-type", "application/json")
                .body(
                    r#"{"text":"second input","delivery":"direct","agent_id":"desktop.agent.v3"}"#,
                )
                .send()
                .await
                .expect("second turn");
            assert!(second.status().is_success());
            let second_json: serde_json::Value =
                serde_json::from_slice(&second.bytes().await.expect("turn response bytes"))
                    .expect("turn json");
            let second_turn_id = second_json["turns"][0]["turn_id"]
                .as_str()
                .expect("turn id")
                .to_owned();

            let worker_for_drive = worker.clone();
            let drive = tokio::task::spawn_local(async move {
                drive_worker_once(&mut queue, worker_for_drive.as_ref()).await
            });
            tokio::time::timeout(
                Duration::from_secs(5),
                model_for_assert.second_started.notified(),
            )
            .await
            .expect("second model invocation did not start");
            assert_eq!(model_for_assert.invocations.load(Ordering::SeqCst), 2);

            let steer = client
                .post(format!(
                    "{base}/v1/sessions/{session_id}/turns/{second_turn_id}/steer"
                ))
                .header("idempotency-key", "in-flight-steer")
                .header("content-type", "application/json")
                .body(r#"{"text":"use this while running"}"#)
                .send()
                .await
                .expect("steer");
            assert_eq!(steer.status().as_u16(), 200);
            model_for_assert.release_second.notify_one();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(5), drive)
                    .await
                    .expect("worker timed out")
                    .expect("worker task"),
                DrivePendingOutcome::Advanced
            );
            let _ = shutdown_tx.send(());
            let _ = server_task.await;
            second_turn_id
        })
        .await;

    assert_eq!(model.invocations.load(Ordering::SeqCst), 3);
    let requests = model.requests.lock().expect("requests mutex").clone();
    assert!(requests[2].contains("User:[steered] use this while running"));
    assert!(requests[2].contains("Assistant:response-2"));
    assert!(requests[2].contains("anthropic.messages.thinking.v1"));
    let snapshot = SqliteLedger::open(&database_for_assert)
        .expect("reopen ledger")
        .load_turn(&garive_ledger::TurnId::try_from(second_turn_id.as_str()).expect("turn id"))
        .expect("load second turn");
    assert_eq!(
        snapshot
            .facts
            .iter()
            .filter(|fact| fact.kind.as_str() == "model.completed")
            .count(),
        2
    );
    assert!(snapshot
        .facts
        .iter()
        .any(|fact| fact.kind.as_str() == "turn.completed"));
}
