use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, SecondsFormat, Utc};
use garive_core::{
    MissingUsagePolicy, ModelRecoveryPolicy, OutputLimitAction, TerminalRecoveryAction,
};
use garive_llm::{ModelCapability, ModelOutputSettings, ModelPort, TextMode};
use garive_provider_anthropic::build_profile as build_anthropic_profile;
use garive_provider_compatible::{MessagesDeployment, ProtocolErrorPolicy, ResponsesDeployment};
use garive_provider_openai::build_profile as build_openai_profile;
use garive_provider_profile::{ConnectionInput, EndpointSelection, SecretValue};
use garive_runtime::{
    ActivityProjectionLimits, EffectiveRuntimeLimits, HostClock, InstalledActivityCatalogue,
    InstalledActivityDescriptor, InstalledAgent, LiveHostLimits, LocalExecutionAttempt,
    LocalExecutionPolicy, RuntimeHttpLimits, RuntimeModelHttpTransport,
};
use uuid::Uuid;

use crate::{
    system_configuration::{
        MissingUsageDocument, OutputLimitDocument, TerminalActionDocument, MAX_DESKTOP_CONFIG_BYTES,
    },
    DesktopConfigurationError, DesktopHostConfig, DesktopOperations, DesktopSystemConfiguration,
};

/// Installed profile identity for the official Responses connection profile.
pub const OPENAI_RESPONSES_PROFILE_ID: &str = "openai.responses.v1";
/// Installed profile identity for the official Messages connection profile.
pub const ANTHROPIC_MESSAGES_PROFILE_ID: &str = "anthropic.messages.v1";
/// Exact versioned document name under Tauri's app configuration directory.
pub const DESKTOP_CONFIG_FILE: &str = "desktop-v1.json";
/// OS credential-store service owned by Garive Desktop.
pub const DESKTOP_CREDENTIAL_SERVICE: &str = "com.garive.desktop";

/// Backend-only resolver for one opaque credential reference.
pub trait DesktopSecretResolver: Send + Sync {
    /// Resolves a reference into a redacting value without fallback discovery.
    fn resolve(&self, credential_ref: &str) -> Result<SecretValue, DesktopConfigurationError>;
}

/// Shipping resolver backed only by the operating-system credential store.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemDesktopSecretResolver;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
impl DesktopSecretResolver for SystemDesktopSecretResolver {
    fn resolve(&self, credential_ref: &str) -> Result<SecretValue, DesktopConfigurationError> {
        let entry = keyring::Entry::new(DESKTOP_CREDENTIAL_SERVICE, credential_ref)
            .map_err(|_| DesktopConfigurationError::SecretUnavailable)?;
        let credential = entry
            .get_password()
            .map_err(|_| DesktopConfigurationError::SecretUnavailable)?;
        SecretValue::new(credential).map_err(|_| DesktopConfigurationError::SecretUnavailable)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
impl DesktopSecretResolver for SystemDesktopSecretResolver {
    fn resolve(&self, _: &str) -> Result<SecretValue, DesktopConfigurationError> {
        Err(DesktopConfigurationError::SecretUnavailable)
    }
}

/// Non-secret values supplied to one installed profile constructor.
#[derive(Clone, Copy, Debug)]
pub struct DesktopProfileConfiguration<'a> {
    /// Exact opaque registry identity.
    pub profile_id: &'a str,
    /// Explicit endpoint, or profile-owned pinned default when absent.
    pub endpoint: Option<&'a str>,
    /// Neutral target identity.
    pub model_target_id: &'a str,
    /// Exact protocol model identity.
    pub model_id: &'a str,
    /// Optional output-token default.
    pub max_output_tokens: Option<u64>,
    /// Runtime HTTP bounds.
    pub http_limits: RuntimeHttpLimits,
}

/// Extensible backend registry constructing one exact installed model profile.
pub trait DesktopProfileRegistry: Send + Sync {
    /// Constructs a model port or rejects an unknown/incompatible profile.
    fn construct(
        &self,
        config: DesktopProfileConfiguration<'_>,
        credential: SecretValue,
    ) -> Result<Arc<dyn ModelPort>, DesktopConfigurationError>;
}

/// Registry containing the two current official P2-V0 connection profiles.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltinDesktopProfileRegistry;

impl DesktopProfileRegistry for BuiltinDesktopProfileRegistry {
    fn construct(
        &self,
        config: DesktopProfileConfiguration<'_>,
        credential: SecretValue,
    ) -> Result<Arc<dyn ModelPort>, DesktopConfigurationError> {
        let endpoint = config.endpoint.map_or(EndpointSelection::Default, |value| {
            EndpointSelection::Explicit(value.to_owned())
        });
        let connection = ConnectionInput::new(endpoint, credential, Vec::new());
        let capabilities = BTreeSet::from([ModelCapability::Text, ModelCapability::Streaming]);
        let model: Arc<dyn ModelPort> = match config.profile_id {
            OPENAI_RESPONSES_PROFILE_ID => {
                let deployment = ResponsesDeployment {
                    target_id: config.model_target_id.to_owned(),
                    model_id: config.model_id.to_owned(),
                    capabilities,
                    default_max_output_tokens: config.max_output_tokens,
                    media_bindings: BTreeMap::new(),
                    reasoning: None,
                    error_policy: ProtocolErrorPolicy::default(),
                };
                let profile = build_openai_profile(&connection)
                    .map_err(|_| DesktopConfigurationError::ConstructionFailure)?;
                Arc::new(
                    RuntimeModelHttpTransport::openai(deployment, profile, config.http_limits)
                        .map_err(|_| DesktopConfigurationError::ConstructionFailure)?,
                )
            }
            ANTHROPIC_MESSAGES_PROFILE_ID => {
                let deployment = MessagesDeployment {
                    target_id: config.model_target_id.to_owned(),
                    model_id: config.model_id.to_owned(),
                    capabilities,
                    default_max_output_tokens: config.max_output_tokens,
                    media_bindings: BTreeMap::new(),
                    thinking: None,
                    error_policy: ProtocolErrorPolicy::default(),
                };
                let profile = build_anthropic_profile(&connection)
                    .map_err(|_| DesktopConfigurationError::ConstructionFailure)?;
                Arc::new(
                    RuntimeModelHttpTransport::anthropic(deployment, profile, config.http_limits)
                        .map_err(|_| DesktopConfigurationError::ConstructionFailure)?,
                )
            }
            _ => return Err(DesktopConfigurationError::UnknownProfile),
        };
        Ok(model)
    }
}

/// Backend provider of one complete immutable Desktop composition.
pub trait DesktopConfigurationProvider: Send + Sync {
    /// Loads no composition when absent, or one fully constructed snapshot.
    fn load(&self) -> Result<Option<DesktopHostConfig>, DesktopConfigurationError>;
}

/// Bounded file provider with injected secret and profile ownership.
pub struct FileDesktopConfigurationProvider<R, P> {
    document_path: PathBuf,
    app_config_directory: PathBuf,
    secret_resolver: R,
    profile_registry: P,
}

impl<R, P> FileDesktopConfigurationProvider<R, P> {
    /// Constructs a provider from exact backend-owned paths and ports.
    pub fn new(
        document_path: PathBuf,
        app_config_directory: PathBuf,
        secret_resolver: R,
        profile_registry: P,
    ) -> Self {
        Self {
            document_path,
            app_config_directory,
            secret_resolver,
            profile_registry,
        }
    }
}

impl<R: DesktopSecretResolver, P: DesktopProfileRegistry> DesktopConfigurationProvider
    for FileDesktopConfigurationProvider<R, P>
{
    fn load(&self) -> Result<Option<DesktopHostConfig>, DesktopConfigurationError> {
        let bytes = match read_bounded(&self.document_path) {
            Ok(value) => value,
            Err(DesktopConfigurationError::NotPresent) => return Ok(None),
            Err(error) => return Err(error),
        };
        let config = DesktopSystemConfiguration::parse(&bytes, &self.app_config_directory)?;
        let credential = self
            .secret_resolver
            .resolve(&config.execution.credential_ref)?;
        let model = self.profile_registry.construct(
            DesktopProfileConfiguration {
                profile_id: &config.execution.profile_id,
                endpoint: config.execution.endpoint.as_deref(),
                model_target_id: &config.execution.model_target_id,
                model_id: &config.execution.model_id,
                max_output_tokens: config.execution.max_output_tokens,
                http_limits: RuntimeHttpLimits {
                    connect_timeout_ms: config.http.connect_timeout_ms,
                    request_timeout_ms: config.http.request_timeout_ms,
                    max_response_bytes: config.http.max_response_bytes,
                },
            },
            credential,
        )?;
        let lease_duration_ms = config.execution_lease_duration_ms;
        let installed_agent = installed_agent(&config);
        let execution_policy = execution_policy(&config);
        Ok(Some(DesktopHostConfig {
            database_path: config.database_path,
            installed_agent,
            host_limits: LiveHostLimits {
                max_command_bytes: config.host.max_command_bytes,
                event_batch_size: config.host.event_batch_size,
                event_poll_interval_ms: config.host.event_poll_interval_ms,
                activity: config.host.activity.map(|limits| ActivityProjectionLimits {
                    max_activities_per_turn: limits.max_activities_per_turn,
                    max_activity_facts: limits.max_activity_facts,
                    max_label_bytes: limits.max_label_bytes,
                    max_activity_id_bytes: limits.max_activity_id_bytes,
                    max_encoded_bytes_per_turn: limits.max_encoded_bytes_per_turn,
                }),
            },
            execution_policy,
            dispatch_capacity: config.dispatch_capacity,
            host_clock: Arc::new(SystemHostClock),
            model,
            operations: Arc::new(SystemDesktopOperations {
                worker_owner_id: format!("desktop-worker-{}", Uuid::new_v4()),
                lease_duration_ms,
            }),
        }))
    }
}

fn installed_agent(config: &DesktopSystemConfiguration) -> InstalledAgent {
    InstalledAgent {
        definition_id: config.installed_agent.definition_id.clone(),
        definition_revision: config.installed_agent.definition_revision.clone(),
        snapshot_digest: config.installed_agent.snapshot_digest.clone(),
        agent_instance_namespace: config.installed_agent.agent_instance_namespace.clone(),
        public_capabilities: Vec::new(),
        runtime_limits: EffectiveRuntimeLimits {
            max_iterations: config.installed_agent.max_iterations,
            max_input_tokens: config.installed_agent.max_input_tokens,
            max_output_tokens: config.installed_agent.max_output_tokens,
            deadline_budget_ms: config.installed_agent.deadline_budget_ms,
        },
        public_activity_catalogue: config
            .installed_agent
            .public_activity_catalogue
            .as_ref()
            .map(|catalogue| InstalledActivityCatalogue {
                schema_version: catalogue.schema_version,
                catalogue_revision: catalogue.catalogue_revision.clone(),
                descriptors: catalogue
                    .descriptors
                    .iter()
                    .map(|item| InstalledActivityDescriptor {
                        tool_name: item.tool_name.clone(),
                        tool_revision: item.tool_revision.clone(),
                        label_key: item.label_key.clone(),
                    })
                    .collect(),
            }),
    }
}

fn execution_policy(config: &DesktopSystemConfiguration) -> LocalExecutionPolicy {
    LocalExecutionPolicy {
        model_target_id: config.execution.model_target_id.clone(),
        deployment_id: config.execution.deployment_id.clone(),
        recovery_policy_revision: config.execution.recovery_policy_revision.clone(),
        required_capabilities: vec![ModelCapability::Text, ModelCapability::Streaming],
        model_output: ModelOutputSettings {
            max_output_tokens: config.execution.max_output_tokens,
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        recovery_policy: ModelRecoveryPolicy {
            max_context_rebuilds: config.execution.max_context_rebuilds,
            output_limit: output_limit(config),
            transport: terminal_action(config.execution.transport_action),
            unavailable: terminal_action(config.execution.unavailable_action),
            missing_usage: missing_usage(config),
        },
        max_context_items: config.execution.max_context_items,
        max_context_utf8_bytes: config.execution.max_context_utf8_bytes,
        max_model_attempts: u64::from(config.execution.max_model_attempts),
    }
}

fn output_limit(config: &DesktopSystemConfiguration) -> OutputLimitAction {
    match config.execution.output_limit_action {
        OutputLimitDocument::CompletePartial => OutputLimitAction::CompletePartial,
        OutputLimitDocument::Retry => OutputLimitAction::Retry {
            max_retries: config.execution.output_limit_max_retries.unwrap_or(0),
        },
        OutputLimitDocument::Suspend => OutputLimitAction::Suspend,
        OutputLimitDocument::Stop => OutputLimitAction::Stop,
        OutputLimitDocument::Fail => OutputLimitAction::Fail,
    }
}

fn terminal_action(value: TerminalActionDocument) -> TerminalRecoveryAction {
    match value {
        TerminalActionDocument::Suspend => TerminalRecoveryAction::Suspend,
        TerminalActionDocument::Stop => TerminalRecoveryAction::Stop,
        TerminalActionDocument::Fail => TerminalRecoveryAction::Fail,
        TerminalActionDocument::AlternateThenSuspend => {
            TerminalRecoveryAction::AlternateThenSuspend
        }
    }
}

fn missing_usage(config: &DesktopSystemConfiguration) -> MissingUsagePolicy {
    match config.execution.missing_usage_policy {
        MissingUsageDocument::Stop => MissingUsagePolicy::Stop,
        MissingUsageDocument::Estimate => MissingUsagePolicy::Estimate {
            input_tokens: config
                .execution
                .missing_usage_estimate_input_tokens
                .unwrap_or(0),
            output_tokens: config
                .execution
                .missing_usage_estimate_output_tokens
                .unwrap_or(0),
        },
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, DesktopConfigurationError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(DesktopConfigurationError::NotPresent)
        }
        Err(_) => return Err(DesktopConfigurationError::ReadFailure),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(DesktopConfigurationError::ReadFailure);
    }
    if metadata.len() > MAX_DESKTOP_CONFIG_BYTES as u64 {
        return Err(DesktopConfigurationError::TooLarge);
    }
    fs::read(path).map_err(|_| DesktopConfigurationError::ReadFailure)
}

struct SystemHostClock;
impl HostClock for SystemHostClock {
    fn recorded_at(&self) -> String {
        timestamp(SystemTime::now())
    }
}

struct SystemDesktopOperations {
    worker_owner_id: String,
    lease_duration_ms: u64,
}
impl DesktopOperations for SystemDesktopOperations {
    fn command_id(&self, purpose: &'static str) -> Result<String, crate::DesktopHostError> {
        Ok(format!("desktop-{purpose}-{}", Uuid::new_v4()))
    }

    fn execution_attempt(&self) -> Result<LocalExecutionAttempt, crate::DesktopHostError> {
        let now = SystemTime::now();
        let now_ms = now
            .duration_since(UNIX_EPOCH)
            .map_err(|_| crate::DesktopHostError::InvalidConfiguration)?
            .as_millis()
            .try_into()
            .map_err(|_| crate::DesktopHostError::InvalidConfiguration)?;
        Ok(LocalExecutionAttempt {
            worker_owner_id: self.worker_owner_id.clone(),
            lease_token: Uuid::new_v4().to_string(),
            now_ms,
            lease_duration_ms: self.lease_duration_ms,
            recorded_at: timestamp(now),
        })
    }
}

fn timestamp(value: SystemTime) -> String {
    let datetime: DateTime<Utc> = value.into();
    datetime.to_rfc3339_opts(SecondsFormat::Millis, true)
}
