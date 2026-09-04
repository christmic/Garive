use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::Duration,
};

use garive_core::{
    MissingUsagePolicy, ModelRecoveryPolicy, OutputLimitAction, TerminalRecoveryAction,
};
use garive_ledger::TurnId;
use garive_llm::{ModelCapability, ModelOutputSettings, TextMode};
use garive_provider_compatible::{ProtocolErrorPolicy, ResponsesDeployment};
use garive_provider_openai::build_profile;
use garive_provider_profile::{ConnectionInput, EndpointSelection, SecretValue};
use garive_runtime::{
    local_dispatch_queue, EffectiveRuntimeLimits, HostClock, InstalledAgent, LiveHost,
    LiveHostLimits, LiveHostServer, LocalCapabilityPreparationFactory,
    LocalCapabilityPreparationInput, LocalExecutionAttempt, LocalExecutionPolicy,
    LocalExecutionWorker, LocalWorkerDisposition, LocalWorkerError, PreparedAgentCapabilities,
    RuntimeHttpLimits, RuntimeModelHttpTransport, SqliteLedger,
};
use serde_json::Value;
use tempfile::tempdir;

const RESPONSE: &str =
    include_str!("../../../spec/fixtures/protocols/openai-responses/complete.sse");

struct Clock;
impl HostClock for Clock {
    fn recorded_at(&self) -> String {
        "2026-08-29T00:00:00Z".into()
    }
}

struct NoCapabilities;
impl LocalCapabilityPreparationFactory for NoCapabilities {
    fn prepare(
        &self,
        _: &SqliteLedger,
        _: LocalCapabilityPreparationInput<'_>,
    ) -> Result<PreparedAgentCapabilities, LocalWorkerError> {
        Ok(PreparedAgentCapabilities::default())
    }
}

#[tokio::test]
async fn loopback_host_to_protocol_flow_commits_terminal() {
    let model_server = OneResponseServer::start(RESPONSE, "text/event-stream");
    let deployment = ResponsesDeployment {
        target_id: "target-main".into(),
        model_id: "model-fixture".into(),
        capabilities: BTreeSet::from([ModelCapability::Text, ModelCapability::Streaming]),
        default_max_output_tokens: Some(10),
        media_bindings: BTreeMap::new(),
        reasoning: None,
        error_policy: ProtocolErrorPolicy::default(),
    };
    let profile = build_profile(&ConnectionInput::new(
        EndpointSelection::Explicit(model_server.url.clone()),
        SecretValue::new("fixture-secret").expect("secret"),
        vec![],
    ))
    .expect("profile");
    let model = Arc::new(
        RuntimeModelHttpTransport::openai(
            deployment,
            profile,
            RuntimeHttpLimits {
                connect_timeout_ms: 1_000,
                request_timeout_ms: 2_000,
                max_response_bytes: 1_000_000,
            },
        )
        .expect("transport"),
    );

    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("garive.db");
    let (dispatcher, mut queue) = local_dispatch_queue(2).expect("queue");
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
                max_input_tokens: Some(100),
                max_output_tokens: Some(10),
                deadline_budget_ms: Some(5_000),
            },
            public_activity_catalogue: None,
        },
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(Clock),
        dispatcher,
    )
    .expect("host");
    let server = LiveHostServer::bind(
        host.clone(),
        "127.0.0.1:0".parse::<SocketAddr>().expect("address"),
    )
    .await
    .expect("live server");
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(server.serve(async move {
        let _ = shutdown_rx.await;
    }));
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let session_text = client
        .post(format!("http://{address}/v1/sessions"))
        .header("Idempotency-Key", "create-live")
        .header("Content-Type", "application/json")
        .body(r#"{"agent_definition_id":"definition-main"}"#)
        .send()
        .await
        .expect("create request")
        .error_for_status()
        .expect("create status")
        .text()
        .await
        .expect("create JSON");
    let session: Value = serde_json::from_str(&session_text).expect("session JSON");
    let session_id = session["session_id"].as_str().expect("session id");
    client
        .post(format!("http://{address}/v1/sessions/{session_id}/turns"))
        .header("Idempotency-Key", "start-live")
        .header("Content-Type", "application/json")
        .body(r#"{"text":"hello over H1","delivery":"direct","agent_id":"definition-main"}"#)
        .send()
        .await
        .expect("start request")
        .error_for_status()
        .expect("start status");

    let worker = LocalExecutionWorker::new(
        &database,
        LocalExecutionPolicy {
            model_target_id: "target-main".into(),
            deployment_id: "deployment-main".into(),
            recovery_policy_revision: "recovery-1".into(),
            required_capabilities: vec![ModelCapability::Text, ModelCapability::Streaming],
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
            max_context_utf8_bytes: 2_048,
            max_model_attempts: 1,
        },
        model,
        Arc::new(NoCapabilities),
    )
    .expect("worker");
    let disposition = queue
        .try_run_next(
            &worker,
            &LocalExecutionAttempt {
                worker_owner_id: "worker-live".into(),
                lease_token: "unpredictable-live-token".into(),
                now_ms: 1_000,
                clock_revision: "test-monotonic-v1".into(),
                lease_duration_ms: 10_000,
                recorded_at: "2026-08-29T00:00:01Z".into(),
            },
        )
        .await
        .expect("execute");
    assert!(matches!(
        disposition,
        LocalWorkerDisposition::TerminalCommitted { .. }
    ));
    let page = host.read_event_page(session_id, 0).expect("events");
    let terminal = page.events.last().expect("terminal");
    let ledger = SqliteLedger::open(&database).expect("diagnostic ledger");
    let snapshot = ledger
        .load_turn(&TurnId::try_from(terminal.turn_id.as_str()).expect("turn identity"))
        .expect("terminal snapshot");
    let facts: Vec<_> = snapshot
        .facts
        .iter()
        .map(|fact| (fact.kind.as_str(), fact.payload.as_json()))
        .collect();
    assert_eq!(terminal.event, "turn.completed", "{facts:?}");
    assert!(!page.events.last().expect("terminal").text.is_empty());
    assert!(model_server
        .join()
        .contains("authorization: Bearer fixture-secret\r\n"));
    let _ = shutdown_tx.send(());
    task.await.expect("server task").expect("server shutdown");
}

struct OneResponseServer {
    url: String,
    thread: thread::JoinHandle<String>,
}
impl OneResponseServer {
    fn start(body: &'static str, content_type: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("model listener");
        let url = format!(
            "http://{}/v1/responses",
            listener.local_addr().expect("address")
        );
        let thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("model accept");
            let request = read_request(&mut stream);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            stream
                .write_all(response.as_bytes())
                .expect("model response");
            request
        });
        Self { url, thread }
    }

    fn join(self) -> String {
        self.thread.join().expect("model thread")
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout");
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4_096];
    loop {
        let count = stream.read(&mut buffer).expect("request bytes");
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end + 4]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8(bytes).expect("UTF-8 request")
}
