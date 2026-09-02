use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use garive_runtime::{
    AllowAllValidator, CommittedTurn, HostClock, InstalledAgent, LiveHost, LiveHostLimits,
    LiveHostServer, ManagementValidator, SqliteLedger, TurnDispatchError, TurnDispatcher,
};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::sync::oneshot;

const NOW: &str = "2026-09-02T00:00:00Z";

struct FixedClock;

impl HostClock for FixedClock {
    fn recorded_at(&self) -> String {
        NOW.to_owned()
    }
}

struct NoopDispatcher;

impl TurnDispatcher for NoopDispatcher {
    fn dispatch(&self, _turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        Ok(())
    }
}

struct AllowlistValidator {
    allowed_profiles: Mutex<Vec<String>>,
    allowed_agents: Mutex<Vec<String>>,
}

impl AllowlistValidator {
    fn new(profiles: &[&str], agents: &[&str]) -> Self {
        Self {
            allowed_profiles: Mutex::new(profiles.iter().map(|value| value.to_string()).collect()),
            allowed_agents: Mutex::new(agents.iter().map(|value| value.to_string()).collect()),
        }
    }
}

impl ManagementValidator for AllowlistValidator {
    fn validate(
        &self,
        body: &garive_runtime::ManagementCommitBody,
    ) -> Result<(), garive_runtime::ManagementConfigError> {
        if !self
            .allowed_profiles
            .lock()
            .unwrap()
            .iter()
            .any(|value| value == &body.profile_id)
        {
            return Err(garive_runtime::ManagementConfigError::ProfileUnknown);
        }
        if !self
            .allowed_agents
            .lock()
            .unwrap()
            .iter()
            .any(|value| value == &body.definition_id)
        {
            return Err(garive_runtime::ManagementConfigError::DefinitionUnknown);
        }
        Ok(())
    }
}

struct Harness {
    _directory: TempDir,
    host: LiveHost,
}

impl Harness {
    fn with_validator(validator: Arc<dyn ManagementValidator>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("host.sqlite3");
        let installed = InstalledAgent {
            definition_id: "definition-main".into(),
            definition_revision: "revision-1".into(),
            snapshot_digest: "0".repeat(64),
            agent_instance_namespace: "installed-main".into(),
            public_capabilities: vec![],
            runtime_limits: garive_runtime::EffectiveRuntimeLimits {
                max_iterations: 4,
                max_input_tokens: None,
                max_output_tokens: None,
                deadline_budget_ms: None,
            },
            public_activity_catalogue: None,
        };
        let host = LiveHost::new_with_read_limits(
            &database,
            installed,
            LiveHostLimits {
                max_command_bytes: 8_192,
                event_batch_size: 16,
                event_poll_interval_ms: 10,
                activity: None,
            },
            garive_runtime::HostReadLimits::PRODUCT_DEFAULT,
            Arc::new(FixedClock),
            Arc::new(NoopDispatcher),
        )
        .unwrap()
        .with_management_validator(validator);
        Self {
            _directory: directory,
            host,
        }
    }

    fn permissive() -> Self {
        Self::with_validator(Arc::new(AllowAllValidator))
    }
}

async fn spawn_server(
    host: LiveHost,
) -> (
    SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<(), garive_runtime::LiveHostServerError>>,
) {
    let server = LiveHostServer::bind(host, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .unwrap();
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(server.serve(async move {
        let _ = shutdown_rx.await;
    }));
    (address, shutdown_tx, task)
}

fn sample_body() -> Value {
    json!({
        "schema_version": 1,
        "profile_id": "openai.responses.v1",
        "endpoint_override": "https://api.openai.com/v1",
        "model_target_id": "gpt-5.6",
        "model_id": "gpt-5.6",
        "deployment_id": "tok9-flash",
        "definition_id": "desktop.agent.v3",
        "api_key": "sk-test-1234567890",
        "runtime_id": "runtime-7e22bcbe-bfa4-4c8f-a0c3-94e07be8f363",
    })
}

#[tokio::test]
async fn get_setup_returns_404_before_first_commit() {
    let harness = Harness::permissive();
    let (address, shutdown, task) = spawn_server(harness.host.clone()).await;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{address}/v1/management/setup"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    let body: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(body["code"], "management_not_configured");
    let _ = shutdown.send(());
    let _ = task.await;
}

#[tokio::test]
async fn health_endpoint_reports_not_configured_initially() {
    let harness = Harness::permissive();
    let (address, shutdown, task) = spawn_server(harness.host.clone()).await;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{address}/v1/management/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(body["configured"], false);
    assert_eq!(body["configuration_revision"], Value::Null);
    let _ = shutdown.send(());
    let _ = task.await;
}

#[tokio::test]
async fn post_commit_then_get_returns_redacted_metadata() {
    let harness = Harness::permissive();
    let (address, shutdown, task) = spawn_server(harness.host.clone()).await;
    let client = reqwest::Client::new();
    let post = client
        .post(format!("http://{address}/v1/management/setup"))
        .header("content-type", "application/json")
        .body(sample_body().to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(post.status(), reqwest::StatusCode::OK);
    let receipt: Value = serde_json::from_slice(&post.bytes().await.unwrap()).unwrap();
    assert_eq!(receipt["schema_version"], 1);
    assert_eq!(receipt["configuration_revision"], 1);
    assert_eq!(receipt["restart_required"], true);
    assert_eq!(receipt["configuration_digest"].as_str().unwrap().len(), 64);
    assert_eq!(receipt["receipt_digest"].as_str().unwrap().len(), 64);

    let get = client
        .get(format!("http://{address}/v1/management/setup"))
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), reqwest::StatusCode::OK);
    let read: Value = serde_json::from_slice(&get.bytes().await.unwrap()).unwrap();
    assert_eq!(read["profile_id"], "openai.responses.v1");
    assert_eq!(read["definition_id"], "desktop.agent.v3");
    assert_eq!(read["configuration_revision"], 1);
    assert!(read.get("api_key").is_none());

    let _ = shutdown.send(());
    let _ = task.await;
}

#[tokio::test]
async fn post_commit_invalid_api_key_returns_400() {
    let harness = Harness::permissive();
    let (address, shutdown, task) = spawn_server(harness.host.clone()).await;
    let client = reqwest::Client::new();
    let mut body = sample_body();
    body["api_key"] = json!("   ");
    let response = client
        .post(format!("http://{address}/v1/management/setup"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let err: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(err["code"], "management_api_key_invalid");
    let _ = shutdown.send(());
    let _ = task.await;
}

#[tokio::test]
async fn post_commit_unknown_profile_returns_400_via_validator() {
    let validator = Arc::new(AllowlistValidator::new(
        &["openai.responses.v1", "anthropic.messages.v1"],
        &["desktop.agent.v3"],
    ));
    let harness = Harness::with_validator(validator);
    let (address, shutdown, task) = spawn_server(harness.host.clone()).await;
    let client = reqwest::Client::new();
    let mut body = sample_body();
    body["profile_id"] = json!("unknown.provider.v9");
    let response = client
        .post(format!("http://{address}/v1/management/setup"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let err: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(err["code"], "management_profile_unknown");
    let _ = shutdown.send(());
    let _ = task.await;
}

#[tokio::test]
async fn post_commit_unknown_definition_returns_400_via_validator() {
    let validator = Arc::new(AllowlistValidator::new(
        &["openai.responses.v1"],
        &["desktop.agent.v3", "desktop.workspace-agent.v3"],
    ));
    let harness = Harness::with_validator(validator);
    let (address, shutdown, task) = spawn_server(harness.host.clone()).await;
    let client = reqwest::Client::new();
    let mut body = sample_body();
    body["definition_id"] = json!("unknown.agent.v9");
    let response = client
        .post(format!("http://{address}/v1/management/setup"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let err: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(err["code"], "management_definition_unknown");
    let _ = shutdown.send(());
    let _ = task.await;
}

#[tokio::test]
async fn delete_setup_removes_singleton_and_unblocks_subsequent_commit() {
    let harness = Harness::permissive();
    let (address, shutdown, task) = spawn_server(harness.host.clone()).await;
    let client = reqwest::Client::new();
    client
        .post(format!("http://{address}/v1/management/setup"))
        .header("content-type", "application/json")
        .body(sample_body().to_string())
        .send()
        .await
        .unwrap();
    let delete = client
        .delete(format!("http://{address}/v1/management/setup"))
        .send()
        .await
        .unwrap();
    assert_eq!(delete.status(), reqwest::StatusCode::NO_CONTENT);
    let get = client
        .get(format!("http://{address}/v1/management/setup"))
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), reqwest::StatusCode::NOT_FOUND);
    let post = client
        .post(format!("http://{address}/v1/management/setup"))
        .header("content-type", "application/json")
        .body(sample_body().to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(post.status(), reqwest::StatusCode::OK);
    let receipt: Value = serde_json::from_slice(&post.bytes().await.unwrap()).unwrap();
    assert_eq!(receipt["configuration_revision"], 1);
    let _ = shutdown.send(());
    let _ = task.await;
}

#[tokio::test]
async fn health_endpoint_reflects_committed_state() {
    let harness = Harness::permissive();
    let (address, shutdown, task) = spawn_server(harness.host.clone()).await;
    let client = reqwest::Client::new();
    client
        .post(format!("http://{address}/v1/management/setup"))
        .header("content-type", "application/json")
        .body(sample_body().to_string())
        .send()
        .await
        .unwrap();
    let response = client
        .get(format!("http://{address}/v1/management/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(body["configured"], true);
    assert_eq!(body["configuration_revision"], 1);
    let _ = shutdown.send(());
    let _ = task.await;
}

#[tokio::test]
async fn non_loopback_bind_is_rejected() {
    let harness = Harness::permissive();
    let result = LiveHostServer::bind(
        harness.host,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0),
    )
    .await;
    assert!(result.is_err());
}

// Suppress unused-import noise when only some tests use the type.
#[allow(dead_code)]
fn _unused(_path: &PathBuf, _ledger: &SqliteLedger) {}
