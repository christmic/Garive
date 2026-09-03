//! Headless Runtime wiring used by the `garive-headless` binary.
//!
//! The headless binary drives H1 sessions against a single committed
//! `runtime_management_config` SQLite row (the management-port wire
//! contract). It never reads `desktop-v1.json`, never instantiates a
//! `DesktopHost`, and never imports anything from `garive-desktop`.
//!
//! The two entry points here are:
//!
//! - [`build_headless_model_port`] — turns the committed `ManagementConfigState`
//!   into an `Arc<dyn ModelPort>` using only the runtime provider crates.
//! - [`build_headless_installation`] — turns the committed `definition_id`
//!   into one collaboration-capable `InstalledAgent` and its backing
//!   `RuntimeAgentCatalogue`.
//!
//! Both functions return explicit error codes so the binary can map them
//! 1-1 to the management-port stable wire codes (`management_*`).
//!
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use garive_config::{
    resolve_definition, AgentDefinition, CapabilityKind, CapabilityReference,
    ContextPolicyReference, DefaultLimits, GovernancePolicy, ProductPolicy, ResolutionRegistry,
};
use garive_core::{
    AgentToolCapabilities, MissingUsagePolicy, ModelRecoveryPolicy, OutputLimitAction,
    TerminalRecoveryAction,
};
use garive_llm::{ModelCapability, ModelOutputSettings, ModelPort, TextMode};
use garive_multiagent::CollaborationToolCatalogue;
use garive_provider_anthropic::build_profile as build_anthropic_profile;
use garive_provider_compatible::{MessagesDeployment, ProtocolErrorPolicy, ResponsesDeployment};
use garive_provider_openai::build_profile as build_openai_profile;
use garive_provider_profile::{ConnectionInput, EndpointSelection, SecretValue};

use crate::{
    HostClock, LocalExecutionAttempt, LocalExecutionPolicy, ManagementConfigState,
    RuntimeAgentCatalogue, RuntimeAgentInstallation, RuntimeAgentInstallationError,
    RuntimeHttpLimits, RuntimeHttpTransportError, RuntimeModelHttpTransport,
};

/// Stable identifier of the OpenAI Responses-compatible profile.
///
/// Must match the value committed to the management port and the desktop
/// `BuiltinManagementValidator` allowlist.
pub const HEADLESS_OPENAI_RESPONSES_PROFILE_ID: &str = "openai.responses.v1";

/// Stable identifier of the Anthropic Messages-compatible profile.
pub const HEADLESS_ANTHROPIC_MESSAGES_PROFILE_ID: &str = "anthropic.messages.v1";

/// Stable identifier of the legacy built-in Desktop agent revision.
///
/// The headless binary mirrors the same definition id the Tauri Desktop
/// Setup flow commits, so an H1 session created with this id accepts
/// the same catalogue the management port can refer to.
pub const HEADLESS_DESKTOP_AGENT_REVISION: &str = "desktop.agent.v3";

/// Exact revision installed under [`HEADLESS_DESKTOP_AGENT_REVISION`] in
/// the headless catalogue. Distinct from the Desktop revision tag so
/// that snapshot digests stay unambiguous across the two paths.
pub const HEADLESS_LEGACY_AGENT_REVISION: &str = "headless.agent.v1";

/// Default per-request output-token ceiling applied to every headless model port.
pub const HEADLESS_DEFAULT_MAX_OUTPUT_TOKENS: u64 = 4_096;
/// Default wall-clock budget for one headless Agent execution and model request.
pub const HEADLESS_DEFAULT_DEADLINE_MS: u64 = 120_000;

/// Default bounded HTTP limits applied to every headless model port.
pub const HEADLESS_DEFAULT_HTTP_LIMITS: RuntimeHttpLimits = RuntimeHttpLimits {
    connect_timeout_ms: 1_000,
    request_timeout_ms: HEADLESS_DEFAULT_DEADLINE_MS,
    max_response_bytes: 1_048_576,
};

/// Default `LocalExecutionPolicy` values used when constructing the worker.
pub const HEADLESS_DEFAULT_MAX_CONTEXT_ITEMS: usize = 32;
/// Default maximum visible context bytes per turn.
pub const HEADLESS_DEFAULT_MAX_CONTEXT_UTF8_BYTES: usize = 1024 * 1024;
/// Default durable model dispatch-attempt bound.
pub const HEADLESS_DEFAULT_MAX_MODEL_ATTEMPTS: u64 = 3;
/// Stable revision tag stamped on every recovery policy in this slice.
pub const HEADLESS_RECOVERY_POLICY_REVISION: &str = "headless.v1";
/// Stable monotonic-clock revision used by every headless execution attempt.
pub const HEADLESS_CLOCK_REVISION: &str = "headless-clock-v1";
/// Stable agent-instance namespace used by the singular catalogue entry.
pub const HEADLESS_AGENT_NAMESPACE: &str = "headless-ns";
/// Stable capability label exposed by the singular catalogue entry.
pub const HEADLESS_AGENT_CAPABILITY: &str = "model_only";
/// Stable public capability label for an explicitly bound Workspace.
pub const HEADLESS_WORKSPACE_CAPABILITY: &str = "workspace";
/// Stable public capability label for autonomous Session collaboration.
pub const HEADLESS_COLLABORATION_CAPABILITY: &str = "collaboration";

/// Stable failure codes emitted by the headless wiring helpers.
///
/// These are surfaced by the `garive-headless` binary as the stable
/// management-port wire codes; the headless binary never constructs a
/// `LiveHost` until every helper has succeeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeadlessConstructionError {
    /// `state.profile_id` is not in the two built-in profile allowlist.
    ProfileUnknown,
    /// `state.definition_id` is not in the headless allowlist.
    DefinitionUnknown,
    /// `state.api_key` is empty or fails internal validation.
    EndpointInvalid,
    /// Constructing the `SecretValue` or `EndpointSelection` failed.
    ConnectionInvalid,
    /// `build_openai_profile` / `build_anthropic_profile` rejected the input.
    ProfileRejected,
    /// `RuntimeModelHttpTransport::openai` / `::anthropic` failed.
    TransportInvalid,
    /// Constructing the agent `AgentDefinition` / `ResolutionRegistry`
    /// pipeline failed.
    ResolutionFailed,
    /// Constructing the `RuntimeAgentInstallation` rejected the snapshot.
    InstallationInvalid,
}

impl HeadlessConstructionError {
    /// Returns the stable machine-readable wire code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::ProfileUnknown => "management_profile_unknown",
            Self::DefinitionUnknown => "management_definition_unknown",
            Self::EndpointInvalid => "management_endpoint_invalid",
            Self::ConnectionInvalid => "management_connection_invalid",
            Self::ProfileRejected => "management_profile_rejected",
            Self::TransportInvalid => "management_transport_invalid",
            Self::ResolutionFailed => "management_resolution_failed",
            Self::InstallationInvalid => "management_installation_invalid",
        }
    }
}

/// All values extracted from the headless binary's H1 read of the
/// management-port singleton row.
///
/// `state` is the same struct exposed over the wire; `api_key` is only
/// present when the caller reached the binary through the trusted
/// in-process `read_with_credential` path (never the H1 GET endpoint).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeadlessConfiguration {
    /// Public `ManagementConfigState` view (no credential).
    pub state: ManagementConfigState,
    /// Plaintext Provider API key used to build the model transport.
    pub api_key: String,
}

/// Builds the headless model port from one trusted configuration.
///
/// Mirrors the two arms of `BuiltinDesktopProfileRegistry::construct` in
/// `desktop/backend/src/system_provider.rs` but uses **only** the runtime
/// provider crates (no `garive-desktop` dependency).
pub fn build_headless_model_port(
    config: &HeadlessConfiguration,
) -> Result<Arc<dyn ModelPort>, HeadlessConstructionError> {
    let secret = SecretValue::new(config.api_key.clone())
        .map_err(|_| HeadlessConstructionError::EndpointInvalid)?;
    let endpoint = config
        .state
        .endpoint_override
        .as_deref()
        .map_or(EndpointSelection::Default, |value| {
            EndpointSelection::Explicit(value.to_owned())
        });
    let connection = ConnectionInput::new(endpoint, secret, Vec::new());
    let capabilities = BTreeSet::from([
        ModelCapability::Text,
        ModelCapability::Streaming,
        ModelCapability::Tools,
    ]);
    let port: Arc<dyn ModelPort> = match config.state.profile_id.as_str() {
        HEADLESS_OPENAI_RESPONSES_PROFILE_ID => {
            let deployment = ResponsesDeployment {
                target_id: config.state.model_target_id.clone(),
                model_id: config.state.model_id.clone(),
                capabilities: capabilities.clone(),
                default_max_output_tokens: Some(HEADLESS_DEFAULT_MAX_OUTPUT_TOKENS),
                media_bindings: BTreeMap::new(),
                reasoning: None,
                error_policy: ProtocolErrorPolicy::default(),
            };
            let profile = build_openai_profile(&connection)
                .map_err(|_| HeadlessConstructionError::ProfileRejected)?;
            let transport = RuntimeModelHttpTransport::openai(
                deployment,
                profile,
                HEADLESS_DEFAULT_HTTP_LIMITS,
            )
            .map_err(map_transport_error)?;
            Arc::new(transport)
        }
        HEADLESS_ANTHROPIC_MESSAGES_PROFILE_ID => {
            let deployment = MessagesDeployment {
                target_id: config.state.model_target_id.clone(),
                model_id: config.state.model_id.clone(),
                capabilities,
                default_max_output_tokens: Some(HEADLESS_DEFAULT_MAX_OUTPUT_TOKENS),
                media_bindings: BTreeMap::new(),
                thinking: None,
                error_policy: ProtocolErrorPolicy::default(),
            };
            let profile = build_anthropic_profile(&connection)
                .map_err(|_| HeadlessConstructionError::ProfileRejected)?;
            let transport = RuntimeModelHttpTransport::anthropic(
                deployment,
                profile,
                HEADLESS_DEFAULT_HTTP_LIMITS,
            )
            .map_err(map_transport_error)?;
            Arc::new(transport)
        }
        _ => return Err(HeadlessConstructionError::ProfileUnknown),
    };
    Ok(port)
}

fn map_transport_error(_: RuntimeHttpTransportError) -> HeadlessConstructionError {
    HeadlessConstructionError::TransportInvalid
}

/// Returns the singular headless agent revision for the given committed
/// `definition_id`. The headless catalogue stays singular for now; any
/// definition id other than [`HEADLESS_DESKTOP_AGENT_REVISION`] is rejected.
pub fn headless_revision_for(definition_id: &str) -> Option<&'static str> {
    match definition_id {
        HEADLESS_DESKTOP_AGENT_REVISION => Some(HEADLESS_LEGACY_AGENT_REVISION),
        _ => None,
    }
}

/// Builds the singular autonomous-collaboration installation and catalogue.
///
/// Returns both the [`RuntimeAgentInstallation`] (used to derive the
/// `InstalledAgent` view passed to `LiveHost`) and the
/// [`RuntimeAgentCatalogue`] (used to wire `CatalogueCapabilityPreparationFactory`).
pub fn build_headless_installation(
    config: &HeadlessConfiguration,
) -> Result<(RuntimeAgentInstallation, Arc<RuntimeAgentCatalogue>), HeadlessConstructionError> {
    build_headless_installation_inner(
        config,
        collaboration_tools()?,
        vec![
            HEADLESS_COLLABORATION_CAPABILITY.into(),
            HEADLESS_AGENT_CAPABILITY.into(),
        ],
    )
}

/// Builds a headless installation that freezes one exact Workspace tool set.
pub fn build_headless_workspace_installation(
    config: &HeadlessConfiguration,
    tools: &AgentToolCapabilities,
) -> Result<(RuntimeAgentInstallation, Arc<RuntimeAgentCatalogue>), HeadlessConstructionError> {
    let mut definitions = tools.definitions.clone();
    definitions.extend(collaboration_tools()?);
    definitions.sort_by(|left, right| left.name().cmp(right.name()));
    build_headless_installation_inner(
        config,
        definitions,
        vec![
            HEADLESS_COLLABORATION_CAPABILITY.into(),
            HEADLESS_AGENT_CAPABILITY.into(),
            HEADLESS_WORKSPACE_CAPABILITY.into(),
        ],
    )
}

fn collaboration_tools() -> Result<Vec<garive_tools::ToolDefinition>, HeadlessConstructionError> {
    CollaborationToolCatalogue::new(crate::COLLABORATION_POLICY_REVISION)
        .map(|catalogue| catalogue.definitions().to_vec())
        .map_err(|_| HeadlessConstructionError::ResolutionFailed)
}

fn build_headless_installation_inner(
    config: &HeadlessConfiguration,
    tools: Vec<garive_tools::ToolDefinition>,
    public_capabilities: Vec<String>,
) -> Result<(RuntimeAgentInstallation, Arc<RuntimeAgentCatalogue>), HeadlessConstructionError> {
    let revision = headless_revision_for(&config.state.definition_id)
        .ok_or(HeadlessConstructionError::DefinitionUnknown)?;
    let requirement_capabilities = tools
        .iter()
        .flat_map(|tool| tool.requirements().capabilities())
        .map(|capability| capability.wire_name().to_owned())
        .collect::<BTreeSet<_>>();
    let capability_references = tools
        .iter()
        .map(|tool| {
            CapabilityReference::new(
                CapabilityKind::Tool,
                tool.name(),
                tool.revision(),
                tool.prepared_contract_version().into(),
                true,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| HeadlessConstructionError::ResolutionFailed)?;
    let limits = DefaultLimits::new(
        8,
        Some(16_384),
        Some(4_096),
        Some(HEADLESS_DEFAULT_DEADLINE_MS),
    )
    .map_err(|_| HeadlessConstructionError::ResolutionFailed)?;
    let governance = GovernancePolicy::new(
        "headless.governance",
        "headless.governance.v1",
        requirement_capabilities.clone(),
        Vec::<garive_config::InteractionMode>::new(),
    )
    .map_err(|_| HeadlessConstructionError::ResolutionFailed)?;
    let context_policy = ContextPolicyReference::new("headless.context", "headless.context.v1")
        .map_err(|_| HeadlessConstructionError::ResolutionFailed)?;
    let definition = AgentDefinition::new(
        config.state.definition_id.clone(),
        revision,
        Vec::new(),
        Vec::new(),
        capability_references,
        governance,
        context_policy,
        limits.clone(),
        BTreeMap::from([("effective_snapshot".to_owned(), 1)]),
    )
    .map_err(|_| HeadlessConstructionError::ResolutionFailed)?;
    let registry = ResolutionRegistry {
        instructions: Vec::new(),
        model_roles: Vec::new(),
        tools,
        capability_descriptors: Vec::new(),
        governance_policies: vec![garive_config::GovernancePolicyCandidate {
            policy_id: "headless.governance".to_owned(),
            exact_revision: "headless.governance.v1".to_owned(),
            allowed_requirement_capabilities: requirement_capabilities.clone(),
            interaction_modes: BTreeSet::new(),
        }],
        context_policies: vec![garive_config::ContextPolicyCandidate {
            policy_id: "headless.context".to_owned(),
            exact_revision: "headless.context.v1".to_owned(),
            descriptor_digest: "a".repeat(64),
        }],
        public_tool_activity_catalogue: None,
    };
    let product_policy = ProductPolicy {
        allowed_requirement_capabilities: requirement_capabilities,
        interaction_modes: BTreeSet::new(),
        limit_caps: limits,
        admitted_contract_versions: BTreeMap::from([(
            "effective_snapshot".to_owned(),
            BTreeSet::from([1_u64]),
        )]),
    };
    let snapshot = resolve_definition(&definition, &registry, &product_policy)
        .map_err(|_| HeadlessConstructionError::ResolutionFailed)?;
    let installation =
        RuntimeAgentInstallation::new(snapshot, HEADLESS_AGENT_NAMESPACE, public_capabilities)
            .map_err(map_installation_error)?;
    let catalogue = Arc::new(
        RuntimeAgentCatalogue::new(vec![installation.clone()])
            .map_err(|_| HeadlessConstructionError::InstallationInvalid)?,
    );
    Ok((installation, catalogue))
}

fn map_installation_error(_: RuntimeAgentInstallationError) -> HeadlessConstructionError {
    HeadlessConstructionError::InstallationInvalid
}

/// Builds the bounded [`LocalExecutionPolicy`] for the headless binary.
pub fn headless_execution_policy(config: &HeadlessConfiguration) -> LocalExecutionPolicy {
    LocalExecutionPolicy {
        model_target_id: config.state.model_target_id.clone(),
        deployment_id: config.state.deployment_id.clone(),
        recovery_policy_revision: HEADLESS_RECOVERY_POLICY_REVISION.to_owned(),
        required_capabilities: vec![
            ModelCapability::Text,
            ModelCapability::Streaming,
            ModelCapability::Tools,
        ],
        model_output: ModelOutputSettings {
            max_output_tokens: Some(HEADLESS_DEFAULT_MAX_OUTPUT_TOKENS),
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
        max_context_items: HEADLESS_DEFAULT_MAX_CONTEXT_ITEMS,
        max_context_utf8_bytes: HEADLESS_DEFAULT_MAX_CONTEXT_UTF8_BYTES,
        max_model_attempts: HEADLESS_DEFAULT_MAX_MODEL_ATTEMPTS,
    }
}

/// Adds neutral tool calling to the normal headless model requirements.
pub fn headless_workspace_execution_policy(config: &HeadlessConfiguration) -> LocalExecutionPolicy {
    headless_execution_policy(config)
}

/// Returns the current Unix epoch milliseconds using the system clock.
///
/// The headless binary uses this to stamp `LocalExecutionAttempt.now_ms`.
pub fn headless_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Constructs a [`LocalExecutionAttempt`] suitable for the headless drive loop.
///
/// Caller is expected to mint one fresh attempt per `drive_pending` tick
/// (mirroring the `DesktopHost::drive_pending` pattern). `now_ms` is the
/// same Unix epoch millisecond value the binary uses to feed
/// [`headless_now_ms`].
pub fn headless_execution_attempt(now_ms: u64) -> LocalExecutionAttempt {
    LocalExecutionAttempt {
        worker_owner_id: format!("headless-worker-{now_ms}"),
        lease_token: format!("lease-{now_ms}"),
        now_ms,
        clock_revision: HEADLESS_CLOCK_REVISION.to_owned(),
        lease_duration_ms: 60_000,
        recorded_at: now_ms_rfc3339(now_ms),
    }
}

/// Hand-rolled RFC 3339 formatter for `now_ms`.
///
/// Avoids depending on `chrono` features the workspace hasn't enabled
/// (the runtime crate pulls `chrono` for `LocalExecutionAttempt` only).
fn now_ms_rfc3339(now_ms: u64) -> String {
    let seconds = now_ms / 1_000;
    let (year, month, day, hour, minute, second) = unix_to_civil(seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn unix_to_civil(seconds: u64) -> (i32, u32, u32, u32, u32, u32) {
    let days = (seconds / 86_400) as i64;
    let secs_of_day = (seconds % 86_400) as u32;
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    (year, month, day, hour, minute, second)
}

fn civil_from_days(days_since_1970: i64) -> (i32, u32, u32) {
    let z = days_since_1970 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Concrete [`HostClock`] implementation used by the `garive-headless`
/// binary. Reads the system wall clock on every call so durable H1 fact
/// timestamps line up with the recorded_at values stamped onto
/// `LocalExecutionAttempt`s.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeadlessClock;

impl HostClock for HeadlessClock {
    fn recorded_at(&self) -> String {
        now_ms_rfc3339(headless_now_ms())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ManagementCommitBody;

    fn sample_state() -> ManagementConfigState {
        ManagementConfigState {
            profile_id: HEADLESS_OPENAI_RESPONSES_PROFILE_ID.to_owned(),
            endpoint_override: Some("http://127.0.0.1:4319/v1/responses".to_owned()),
            model_target_id: "tok9-flash".to_owned(),
            model_id: "tok9-flash".to_owned(),
            deployment_id: "tok9-flash".to_owned(),
            definition_id: HEADLESS_DESKTOP_AGENT_REVISION.to_owned(),
            runtime_id: "runtime-test".to_owned(),
            configuration_revision: 1,
            configuration_digest: "a".repeat(64),
            committed_at: "2026-09-02T00:00:00Z".to_owned(),
        }
    }

    fn sample_configuration() -> HeadlessConfiguration {
        HeadlessConfiguration {
            state: sample_state(),
            api_key: "sk-test-1234567890".to_owned(),
        }
    }

    #[test]
    fn revision_lookup_rejects_unknown_definitions() {
        assert_eq!(
            headless_revision_for(HEADLESS_DESKTOP_AGENT_REVISION),
            Some(HEADLESS_LEGACY_AGENT_REVISION),
        );
        assert_eq!(headless_revision_for("desktop.unknown"), None);
        assert_eq!(headless_revision_for(""), None);
    }

    #[test]
    fn unknown_profile_id_is_rejected_before_any_io() {
        let mut configuration = sample_configuration();
        configuration.state.profile_id = "openai.unknown.v9".to_owned();
        match build_headless_model_port(&configuration) {
            Err(HeadlessConstructionError::ProfileUnknown) => {}
            Err(other) => panic!("expected ProfileUnknown, got {other:?}"),
            Ok(_) => panic!("expected ProfileUnknown, got Ok"),
        }
    }

    #[test]
    fn empty_api_key_is_rejected() {
        let mut configuration = sample_configuration();
        configuration.api_key = String::new();
        match build_headless_model_port(&configuration) {
            Err(HeadlessConstructionError::EndpointInvalid) => {}
            Err(other) => panic!("expected EndpointInvalid, got {other:?}"),
            Ok(_) => panic!("expected EndpointInvalid, got Ok"),
        }
    }

    #[test]
    fn execution_policy_carries_state_identity() {
        let configuration = sample_configuration();
        let policy = headless_execution_policy(&configuration);
        assert_eq!(policy.model_target_id, configuration.state.model_target_id);
        assert_eq!(policy.deployment_id, configuration.state.deployment_id);
        assert_eq!(
            policy.recovery_policy_revision,
            HEADLESS_RECOVERY_POLICY_REVISION
        );
    }

    #[test]
    fn model_timeout_covers_the_agent_execution_deadline() {
        let configuration = sample_configuration();
        let (installation, _) = build_headless_installation(&configuration).unwrap();
        let deadline = installation
            .clone_installed_agent()
            .runtime_limits
            .deadline_budget_ms
            .expect("headless deadline");
        assert!(deadline >= 120_000);
        assert!(HEADLESS_DEFAULT_HTTP_LIMITS.request_timeout_ms >= deadline);
    }

    #[test]
    fn execution_attempt_stamps_now_ms_into_worker_owner_id() {
        let attempt = headless_execution_attempt(1_700_000_000_000);
        assert!(attempt.worker_owner_id.starts_with("headless-worker-"));
        assert!(attempt.lease_token.starts_with("lease-"));
        assert_eq!(attempt.clock_revision, HEADLESS_CLOCK_REVISION);
        assert_eq!(attempt.lease_duration_ms, 60_000);
        assert!(attempt.recorded_at.ends_with('Z'));
    }

    #[test]
    fn construction_error_codes_match_management_port_contract() {
        assert_eq!(
            HeadlessConstructionError::ProfileUnknown.code(),
            "management_profile_unknown",
        );
        assert_eq!(
            HeadlessConstructionError::DefinitionUnknown.code(),
            "management_definition_unknown",
        );
        assert_eq!(
            HeadlessConstructionError::EndpointInvalid.code(),
            "management_endpoint_invalid",
        );
    }

    #[test]
    fn build_installation_rejects_unknown_definition() {
        let mut configuration = sample_configuration();
        configuration.state.definition_id = "desktop.unknown.v9".to_owned();
        match build_headless_installation(&configuration) {
            Err(HeadlessConstructionError::DefinitionUnknown) => {}
            Err(other) => panic!("expected DefinitionUnknown, got {other:?}"),
            Ok(_) => panic!("expected DefinitionUnknown, got Ok"),
        }
    }

    #[test]
    fn build_installation_succeeds_for_known_definition() {
        let configuration = sample_configuration();
        let result = build_headless_installation(&configuration);
        let (installation, catalogue) = result.expect("known definition must install");
        assert_eq!(
            installation.installed_agent().definition_id,
            HEADLESS_DESKTOP_AGENT_REVISION
        );
        assert_eq!(catalogue.len(), 1);
    }

    #[test]
    fn commit_body_is_unaffected_by_headless_helpers() {
        // Sanity check: the headless module never imports ManagementCommitBody;
        // this test just exercises the type's basic shape to confirm the test
        // file can still construct one without dragging the validator chain.
        let body = ManagementCommitBody {
            schema_version: 1,
            profile_id: HEADLESS_OPENAI_RESPONSES_PROFILE_ID.to_owned(),
            endpoint_override: None,
            model_target_id: "tok9-flash".to_owned(),
            model_id: "tok9-flash".to_owned(),
            deployment_id: "tok9-flash".to_owned(),
            definition_id: HEADLESS_DESKTOP_AGENT_REVISION.to_owned(),
            api_key: "sk-test-1234567890".to_owned(),
            runtime_id: "runtime-test".to_owned(),
        };
        assert_eq!(body.profile_id, HEADLESS_OPENAI_RESPONSES_PROFILE_ID);
    }
}
