use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
#[cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt};

use garive_desktop::{
    builtin_desktop_agent_installation, builtin_desktop_workspace_agent_installation,
    BuiltinDesktopProfileRegistry, DesktopConfigurationError, DesktopConfigurationProvider,
    DesktopHost, DesktopProfileConfiguration, DesktopProfileRegistry, DesktopSecretResolver,
    DesktopSetupError, DesktopSetupInput, DesktopSetupService, DesktopState,
    DesktopSystemConfiguration, DesktopWorkspaceExecutionFactory, DesktopWorkspaceService,
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
    ProcessBackendHostConfig, ProcessExecutable, ProcessLane, ProcessLaneRegistry,
    RuntimeHttpLimits, SqliteLedger, T1HostSystemConfig,
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
fn strict_v3_normalizes_an_ordered_multi_agent_catalogue() {
    let mut value: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    let mut primary = value
        .as_object_mut()
        .unwrap()
        .remove("installed_agent")
        .unwrap();
    let mut workspace = primary.clone();
    primary["definition_id"] = "agent-general".into();
    workspace["definition_id"] = "agent-workspace".into();
    workspace["agent_instance_namespace"] = "desktop-workspace".into();
    value["schema_version"] = 3.into();
    value["configuration_revision"] = 7.into();
    value["setup_id"] = "setup-v3".into();
    value["default_agent_definition_id"] = "agent-workspace".into();
    value["installed_agents"] = serde_json::json!([primary, workspace]);

    let parsed = parse(&serde_json::to_vec(&value).unwrap()).expect("strict v3");
    assert_eq!(parsed.schema_version(), 3);
    assert_eq!(parsed.configuration_revision(), Some(7));
    assert_eq!(parsed.default_agent_definition_id(), "agent-workspace");
    assert_eq!(parsed.installed_agent_count(), 2);
}

#[test]
fn v3_rejects_legacy_mix_unknown_default_and_noncanonical_catalogue() {
    let mut value: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    let installed = value["installed_agent"].clone();
    value["schema_version"] = 3.into();
    value["configuration_revision"] = 1.into();
    value["setup_id"] = "setup-v3".into();
    value["default_agent_definition_id"] = "definition-main".into();
    value["installed_agents"] = serde_json::json!([installed.clone()]);
    assert_eq!(
        parse(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
        DesktopConfigurationError::InvalidDocument
    );

    value.as_object_mut().unwrap().remove("installed_agent");
    value["default_agent_definition_id"] = "missing".into();
    assert_eq!(
        parse(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
        DesktopConfigurationError::InvalidValue
    );

    value["default_agent_definition_id"] = "definition-main".into();
    value["installed_agents"] = serde_json::json!([installed.clone(), installed]);
    assert_eq!(
        parse(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
        DesktopConfigurationError::InvalidValue
    );
}

#[cfg(unix)]
#[test]
fn provider_reconstructs_workspace_agent_only_from_exact_t1_capabilities() {
    let directory = tempdir().unwrap();
    let patch_recovery = directory.path().join("patch-recovery");
    let process_recovery = directory.path().join("process-recovery");
    for path in [&patch_recovery, &process_recovery] {
        fs::create_dir(path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let lanes = ProcessLaneRegistry::new([ProcessLane::new(
        "rust",
        [ProcessExecutable::new("cargo", "/opt/garive/bin/cargo").unwrap()],
        Vec::new(),
    )
    .unwrap()])
    .unwrap();
    let t1 = T1HostSystemConfig::new(
        "t1.policy.v1",
        "t1.executor.v1",
        patch_recovery,
        lanes,
        ProcessBackendHostConfig::podman(
            "/opt/garive/bin/podman",
            "unix:///var/run/garive-podman.sock",
            format!("localhost/garive-runner@sha256:{}", "a".repeat(64)),
            process_recovery,
            5_000,
        )
        .unwrap(),
    )
    .unwrap();
    let general = builtin_desktop_agent_installation("agent-general", "desktop-general").unwrap();
    let workspace = builtin_desktop_workspace_agent_installation(
        "agent-workspace",
        "desktop-workspace",
        &t1.tool_capabilities().unwrap(),
    )
    .unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    value.as_object_mut().unwrap().remove("installed_agent");
    value["schema_version"] = 3.into();
    value["configuration_revision"] = 1.into();
    value["setup_id"] = "setup-v3".into();
    value["default_agent_definition_id"] = "agent-general".into();
    value["installed_agents"] =
        serde_json::json!([agent_document(&general), agent_document(&workspace)]);
    let document = directory.path().join("desktop-v1.json");
    fs::write(&document, serde_json::to_vec(&value).unwrap()).unwrap();

    let absent = FileDesktopConfigurationProvider::new(
        document.clone(),
        directory.path().to_owned(),
        FixtureSecrets,
        FixtureProfiles,
    );
    assert!(matches!(
        absent.load(),
        Err(DesktopConfigurationError::ConstructionFailure)
    ));
    let configured = FileDesktopConfigurationProvider::new(
        document,
        directory.path().to_owned(),
        FixtureSecrets,
        FixtureProfiles,
    )
    .with_t1_host_system_config(t1)
    .load()
    .unwrap()
    .unwrap();
    assert_eq!(configured.default_agent_definition_id, "agent-general");
    assert_eq!(configured.agent_catalogue.len(), 2);
    assert!(configured.agent_catalogue.get("agent-workspace").is_some());
    let first = configured.operations.execution_attempt().unwrap();
    let second = configured.operations.execution_attempt().unwrap();
    assert!(first.clock_revision.starts_with("os-monotonic-boot-v1:"));
    assert_eq!(first.clock_revision, second.clock_revision);
    assert!(second.now_ms >= first.now_ms);
}

#[cfg(unix)]
fn agent_document(installation: &garive_runtime::RuntimeAgentInstallation) -> serde_json::Value {
    let agent = installation.installed_agent();
    serde_json::json!({
        "definition_id": agent.definition_id,
        "definition_revision": agent.definition_revision,
        "snapshot_digest": agent.snapshot_digest,
        "agent_instance_namespace": agent.agent_instance_namespace,
        "max_iterations": agent.runtime_limits.max_iterations,
        "max_input_tokens": agent.runtime_limits.max_input_tokens,
        "max_output_tokens": agent.runtime_limits.max_output_tokens,
        "deadline_budget_ms": agent.runtime_limits.deadline_budget_ms
    })
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

fn governed_state(database: &Path) -> DesktopState {
    let factory = DesktopWorkspaceExecutionFactory::new(
        database.to_owned(),
        DesktopWorkspaceService::default(),
        "main",
    )
    .unwrap();
    DesktopState::governed(Arc::new(factory))
}

#[test]
fn tool_bearing_config_uses_shared_workspace_governance_composition() {
    let directory = tempdir().expect("temporary config directory");
    let document = directory.path().join("desktop-v1.json");
    std::fs::write(&document, FIXTURE).expect("write fixture");
    let provider = FileDesktopConfigurationProvider::new(
        document,
        directory.path().to_owned(),
        FixtureSecrets,
        FixtureProfiles,
    );
    let ungoverned = provider.load().unwrap().unwrap();
    assert_eq!(
        DesktopHost::new(ungoverned)
            .err()
            .expect("tool-bearing config rejects ungoverned composition")
            .code(),
        "invalid_configuration"
    );
    let governed = provider.load().unwrap().unwrap();
    DesktopHost::new_with_workspaces(governed, DesktopWorkspaceService::default(), "host-cli")
        .expect("shipping GUI and loopback Host share governed composition");
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
    let state = governed_state(&directory.path().join("garive-desktop.db"));
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
    let stored: serde_json::Value = serde_json::from_slice(
        &std::fs::read(directory.path().join("desktop-v1.json")).expect("stored config"),
    )
    .expect("stored JSON");
    assert_eq!(stored["schema_version"], 5);
    assert_eq!(
        stored["knowledge"]["connector_id"],
        "desktop.static-system-guide.v1"
    );
    assert_eq!(
        stored["installed_agents"][0]["definition_revision"],
        "desktop.agent.v3"
    );

    let restarted = governed_state(&directory.path().join("garive-desktop.db"));
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
    let ledger = SqliteLedger::open(directory.path().join("garive-desktop.db")).unwrap();
    let turn = garive_ledger::TurnId::try_from(result.turn_id.as_str()).unwrap();
    let facts = ledger.load_turn(&turn).unwrap().facts;
    let memory = facts
        .iter()
        .position(|fact| fact.kind.as_str() == "memory.retrieval_recorded")
        .unwrap();
    let knowledge_requested = facts
        .iter()
        .position(|fact| fact.kind.as_str() == "knowledge.requested")
        .unwrap();
    let knowledge_dispatched = facts
        .iter()
        .position(|fact| fact.kind.as_str() == "knowledge.dispatched")
        .unwrap();
    let knowledge_completed = facts
        .iter()
        .position(|fact| fact.kind.as_str() == "knowledge.completed")
        .unwrap();
    let model = facts
        .iter()
        .position(|fact| fact.kind.as_str() == "model.started")
        .unwrap();
    assert!(memory < model);
    assert!(knowledge_requested < knowledge_dispatched);
    assert!(knowledge_dispatched < knowledge_completed);
    assert!(knowledge_completed < model);
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
    let state = governed_state(&directory.path().join("garive-desktop.db"));
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
