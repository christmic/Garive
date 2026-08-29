use std::{path::Path, sync::Arc};

use garive_desktop::{
    BuiltinDesktopProfileRegistry, DesktopConfigurationError, DesktopProfileConfiguration,
    DesktopProfileRegistry, DesktopSecretResolver, DesktopState, DesktopSystemConfiguration,
    FileDesktopConfigurationProvider, ANTHROPIC_MESSAGES_PROFILE_ID, MAX_DESKTOP_CONFIG_BYTES,
    OPENAI_RESPONSES_PROFILE_ID,
};
use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelFuture, ModelItem, ModelObserver, ModelPort,
    ModelRequest, ModelStopReason, ModelUsage, TokenCount, UsageSource,
};
use garive_provider_profile::SecretValue;
use garive_runtime::RuntimeHttpLimits;
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
