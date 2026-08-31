use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use garive_desktop::{
    BuiltinDesktopProfileRegistry, DesktopConfigurationError, DesktopProfileConfiguration,
    DesktopProfileRegistry, DesktopSecretResolver, DesktopSetupError, DesktopSetupInput,
    DesktopSetupService, DesktopState, DesktopSystemConfiguration,
    FileDesktopConfigurationProvider, SetupCredentialStore, ANTHROPIC_MESSAGES_PROFILE_ID,
    MAX_DESKTOP_CONFIG_BYTES, OPENAI_RESPONSES_PROFILE_ID,
};
use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture, ModelItem, ModelObserver,
    ModelPort, ModelRequest, ModelStopReason, ModelUsage, TokenCount, UsageSource,
};
use garive_provider_profile::SecretValue;
use garive_runtime::{
    CommittedTurn, LocalGovernedExecution, LocalGovernedExecutionFactory, LocalWorkerError,
    RuntimeHttpLimits,
};
use tempfile::tempdir;

const FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../spec/fixtures/host/desktop-system-config-v1.json"
));

#[test]
fn fixture_parses_as_bounded_non_secret_snapshot() {
    let config = DesktopSystemConfiguration::parse(FIXTURE, Path::new("/tmp/garive-config"))
        .expect("valid fixture");
    assert_eq!(
        config.database_path(),
        Path::new("/tmp/garive-config/garive-desktop.db")
    );
    assert_eq!(config.profile_id(), "fixture.responses");
    let debug = format!("{config:?}");
    assert!(!debug.contains("desktop-fixture"));
    assert!(debug.contains("<redacted-reference>"));
}

#[test]
fn malformed_duplicate_oversized_and_unsafe_documents_fail_closed() {
    let duplicate = String::from_utf8(FIXTURE.to_vec()).unwrap().replace(
        "\"schema_version\": 1,",
        "\"schema_version\": 1,\n  \"schema_version\": 1,",
    );
    assert_eq!(
        parse(duplicate.as_bytes()).unwrap_err(),
        DesktopConfigurationError::InvalidDocument
    );
    let unknown = String::from_utf8(FIXTURE.to_vec()).unwrap().replace(
        "\"schema_version\": 1,",
        "\"schema_version\": 1,\n  \"future\": true,",
    );
    assert_eq!(
        parse(unknown.as_bytes()).unwrap_err(),
        DesktopConfigurationError::InvalidDocument
    );
    let traversal = String::from_utf8(FIXTURE.to_vec())
        .unwrap()
        .replace("garive-desktop.db", "../outside.db");
    assert_eq!(
        parse(traversal.as_bytes()).unwrap_err(),
        DesktopConfigurationError::InvalidPath
    );
    let zero = String::from_utf8(FIXTURE.to_vec())
        .unwrap()
        .replace("\"dispatch_capacity\": 2", "\"dispatch_capacity\": 0");
    assert_eq!(
        parse(zero.as_bytes()).unwrap_err(),
        DesktopConfigurationError::InvalidValue
    );
    assert_eq!(
        parse(&vec![b' '; MAX_DESKTOP_CONFIG_BYTES + 1]).unwrap_err(),
        DesktopConfigurationError::TooLarge
    );
}

#[test]
fn strict_v2_requires_revision_and_setup_identity() {
    let v2 = String::from_utf8(FIXTURE.to_vec()).unwrap().replace(
        "\"schema_version\": 1,",
        "\"schema_version\": 2,\n  \"configuration_revision\": 1,\n  \"setup_id\": \"setup-1\",",
    );
    DesktopSystemConfiguration::parse(v2.as_bytes(), Path::new("/tmp/garive-config"))
        .expect("strict v2");
    let incomplete = String::from_utf8(FIXTURE.to_vec())
        .unwrap()
        .replace("\"schema_version\": 1", "\"schema_version\": 2");
    assert_eq!(
        parse(incomplete.as_bytes()).unwrap_err(),
        DesktopConfigurationError::InvalidDocument
    );
}

#[test]
fn contradictory_policy_options_fail_closed() {
    let retry_without_bound = String::from_utf8(FIXTURE.to_vec()).unwrap().replace(
        "\"output_limit_action\": \"suspend\"",
        "\"output_limit_action\": \"retry\"",
    );
    assert_eq!(
        parse(retry_without_bound.as_bytes()).unwrap_err(),
        DesktopConfigurationError::InvalidValue
    );
    let estimate_without_charges = String::from_utf8(FIXTURE.to_vec()).unwrap().replace(
        "\"missing_usage_policy\": \"stop\"",
        "\"missing_usage_policy\": \"estimate\"",
    );
    assert_eq!(
        parse(estimate_without_charges.as_bytes()).unwrap_err(),
        DesktopConfigurationError::InvalidValue
    );
}

#[test]
fn h3_catalogue_and_projection_limits_must_be_installed_together() {
    let mut value: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    value["host"]["activity"] = serde_json::json!({
        "max_activities_per_turn": 8,
        "max_activity_facts": 64,
        "max_label_bytes": 128,
        "max_activity_id_bytes": 128,
        "max_encoded_bytes_per_turn": 8192
    });
    assert_eq!(
        parse(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
        DesktopConfigurationError::InvalidValue
    );
}

fn parse(bytes: &[u8]) -> Result<DesktopSystemConfiguration, DesktopConfigurationError> {
    DesktopSystemConfiguration::parse(bytes, Path::new("/tmp/garive-config"))
}

struct FixtureSecrets;
impl DesktopSecretResolver for FixtureSecrets {
    fn resolve(&self, credential_ref: &str) -> Result<SecretValue, DesktopConfigurationError> {
        if credential_ref != "desktop-fixture" {
            return Err(DesktopConfigurationError::SecretUnavailable);
        }
        SecretValue::new("fixture-secret-never-serialized")
            .map_err(|_| DesktopConfigurationError::SecretUnavailable)
    }
}

struct FixtureProfiles;
impl DesktopProfileRegistry for FixtureProfiles {
    fn construct(
        &self,
        config: DesktopProfileConfiguration<'_>,
        credential: SecretValue,
    ) -> Result<Arc<dyn ModelPort>, DesktopConfigurationError> {
        assert_eq!(config.profile_id, "fixture.responses");
        assert_eq!(config.model_target_id, "desktop-target");
        assert_eq!(
            credential.expose_secret(),
            "fixture-secret-never-serialized"
        );
        Ok(Arc::new(CompletingModel))
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
            assert_eq!(
                request.required_capabilities,
                vec![ModelCapability::Text, ModelCapability::Streaming]
            );
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text {
                    text: "configured durable answer".into(),
                }],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(2),
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
async fn file_provider_installs_and_runs_one_durable_turn() {
    let directory = tempdir().expect("temporary config directory");
    let document = directory.path().join("desktop-v1.json");
    std::fs::write(&document, FIXTURE).expect("write fixture");
    let provider = FileDesktopConfigurationProvider::new(
        document,
        directory.path().to_owned(),
        FixtureSecrets,
        FixtureProfiles,
    );
    let state = DesktopState::default();
    assert!(state.install_from(&provider).expect("installed"));
    let result = state
        .run_turn_isolated("definition-main".into(), "hello configured desktop".into())
        .await
        .expect("durable terminal");
    assert_eq!(result.text, "configured durable answer");
    assert!(!result.session_id.is_empty());
    assert!(!format!("{result:?}").contains("fixture-secret-never-serialized"));
}

struct RejectingGovernedFactory;
impl LocalGovernedExecutionFactory for RejectingGovernedFactory {
    fn create(&self, _: &CommittedTurn) -> Result<LocalGovernedExecution, LocalWorkerError> {
        Err(LocalWorkerError::InvalidComposition)
    }
}

#[tokio::test]
async fn configured_state_routes_execution_through_its_governed_factory() {
    let directory = tempdir().unwrap();
    let document = directory.path().join("desktop-v1.json");
    std::fs::write(&document, FIXTURE).unwrap();
    let provider = FileDesktopConfigurationProvider::new(
        document,
        directory.path().to_owned(),
        FixtureSecrets,
        FixtureProfiles,
    );
    let state = DesktopState::governed(Arc::new(RejectingGovernedFactory));
    assert!(state.install_from(&provider).unwrap());
    assert_eq!(
        state
            .run_turn_isolated("definition-main".into(), "governed".into())
            .await
            .unwrap_err()
            .code(),
        "execution_failure"
    );
}

#[test]
fn missing_document_is_distinct_from_invalid_configuration() {
    let directory = tempdir().expect("temporary config directory");
    let provider = FileDesktopConfigurationProvider::new(
        directory.path().join("desktop-v1.json"),
        directory.path().to_owned(),
        FixtureSecrets,
        FixtureProfiles,
    );
    assert!(!DesktopState::default()
        .install_from(&provider)
        .expect("absence is not malformed"));
}

#[derive(Clone, Default)]
struct RestartSecrets(Arc<Mutex<BTreeMap<String, String>>>);

impl SetupCredentialStore for RestartSecrets {
    fn store(&self, credential_ref: &str, credential: &str) -> Result<(), DesktopSetupError> {
        self.0
            .lock()
            .unwrap()
            .insert(credential_ref.into(), credential.into());
        Ok(())
    }

    fn delete(&self, credential_ref: &str) -> Result<(), DesktopSetupError> {
        self.0.lock().unwrap().remove(credential_ref);
        Ok(())
    }
}

impl DesktopSecretResolver for RestartSecrets {
    fn resolve(&self, credential_ref: &str) -> Result<SecretValue, DesktopConfigurationError> {
        let secret = self
            .0
            .lock()
            .unwrap()
            .get(credential_ref)
            .cloned()
            .ok_or(DesktopConfigurationError::SecretUnavailable)?;
        SecretValue::new(secret).map_err(|_| DesktopConfigurationError::SecretUnavailable)
    }
}

struct RestartProfiles;

impl DesktopProfileRegistry for RestartProfiles {
    fn construct(
        &self,
        config: DesktopProfileConfiguration<'_>,
        credential: SecretValue,
    ) -> Result<Arc<dyn ModelPort>, DesktopConfigurationError> {
        assert_eq!(config.profile_id, OPENAI_RESPONSES_PROFILE_ID);
        assert_eq!(credential.expose_secret(), "restart-secret");
        Ok(Arc::new(CompletingModel))
    }
}

#[tokio::test]
async fn committed_setup_constructs_runtime_after_explicit_restart() {
    let directory = tempdir().expect("temporary config directory");
    let secrets = RestartSecrets::default();
    let setup = DesktopSetupService::new(directory.path().to_owned(), secrets.clone());
    let plan = setup
        .prepare(DesktopSetupInput {
            schema_version: 1,
            caller_nonce: "restart-nonce".into(),
            catalogue_revision: "desktop-setup-catalogue-1".into(),
            preset_id: "desktop-balanced-v1".into(),
            profile_id: OPENAI_RESPONSES_PROFILE_ID.into(),
            endpoint_override: None,
            model_target_id: "desktop-target".into(),
            model_id: "restart-model".into(),
            deployment_id: "desktop-deployment".into(),
            definition_id: "definition-main".into(),
        })
        .expect("prepared setup");
    let receipt = setup
        .commit(&plan.plan_digest, "restart-secret")
        .expect("committed setup");
    assert_eq!(receipt.configuration_revision, 1);

    let restarted = DesktopState::default();
    let provider = FileDesktopConfigurationProvider::new(
        directory.path().join("desktop-v1.json"),
        directory.path().to_owned(),
        secrets,
        RestartProfiles,
    );
    assert!(restarted
        .install_from(&provider)
        .expect("restart installs revision"));
    let result = restarted
        .run_turn_isolated("definition-main".into(), "hello after restart".into())
        .await
        .expect("durable turn after setup restart");
    assert_eq!(result.text, "configured durable answer");
}

#[test]
fn builtin_registry_constructs_exact_profiles_and_rejects_unknown_identity() {
    let registry = BuiltinDesktopProfileRegistry;
    for profile_id in [OPENAI_RESPONSES_PROFILE_ID, ANTHROPIC_MESSAGES_PROFILE_ID] {
        registry
            .construct(
                DesktopProfileConfiguration {
                    profile_id,
                    endpoint: Some("http://127.0.0.1:4319/v1/model"),
                    model_target_id: "desktop-target",
                    model_id: "fixture-model",
                    max_output_tokens: Some(16),
                    http_limits: RuntimeHttpLimits {
                        connect_timeout_ms: 1_000,
                        request_timeout_ms: 2_000,
                        max_response_bytes: 65_536,
                    },
                },
                SecretValue::new("registry-secret").unwrap(),
            )
            .expect("installed profile");
    }
    let failure = registry
        .construct(
            DesktopProfileConfiguration {
                profile_id: "future.uninstalled",
                endpoint: None,
                model_target_id: "desktop-target",
                model_id: "fixture-model",
                max_output_tokens: Some(16),
                http_limits: RuntimeHttpLimits {
                    connect_timeout_ms: 1_000,
                    request_timeout_ms: 2_000,
                    max_response_bytes: 65_536,
                },
            },
            SecretValue::new("unknown-secret").unwrap(),
        )
        .err()
        .expect("unknown profile fails");
    assert_eq!(failure, DesktopConfigurationError::UnknownProfile);
}

#[tokio::test]
async fn configured_builtin_profile_completes_over_real_loopback_http() {
    let server = OneResponseServer::start();
    let directory = tempdir().expect("temporary config directory");
    let document = directory.path().join("desktop-v1.json");
    let config = String::from_utf8(FIXTURE.to_vec())
        .unwrap()
        .replace("fixture.responses", OPENAI_RESPONSES_PROFILE_ID)
        .replace("http://127.0.0.1:4319/v1/responses", server.url.as_str());
    std::fs::write(&document, config).expect("write live config");
    let provider = FileDesktopConfigurationProvider::new(
        document,
        directory.path().to_owned(),
        FixtureSecrets,
        BuiltinDesktopProfileRegistry,
    );
    let state = DesktopState::default();
    assert!(state.install_from(&provider).expect("installed"));
    let result = state
        .run_turn_isolated("definition-main".into(), "hello live desktop".into())
        .await
        .expect("live durable terminal");
    assert_eq!(result.text, "hello back");
    let request = server.join();
    assert!(request.contains("authorization: Bearer fixture-secret-never-serialized\r\n"));
    assert!(request.contains("accept: text/event-stream\r\n"));
    assert!(request.contains("\"stream\":true"));
    assert!(!format!("{result:?}").contains("fixture-secret-never-serialized"));
}

const MODEL_RESPONSE: &str =
    include_str!("../../../spec/fixtures/protocols/openai-responses/complete.sse");

struct OneResponseServer {
    url: String,
    thread: thread::JoinHandle<String>,
}

impl OneResponseServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("model listener");
        let url = format!(
            "http://{}/v1/responses",
            listener.local_addr().expect("model address")
        );
        let thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("model accept");
            let request = read_request(&mut stream);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                MODEL_RESPONSE.len(),
                MODEL_RESPONSE
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
