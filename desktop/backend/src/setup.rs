use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    DesktopSystemConfiguration, ANTHROPIC_MESSAGES_PROFILE_ID, DESKTOP_CONFIG_FILE,
    DESKTOP_CREDENTIAL_SERVICE, OPENAI_RESPONSES_PROFILE_ID,
};

const CATALOGUE_REVISION: &str = "desktop-setup-catalogue-1";
const MAX_TEXT_BYTES: usize = 256;
const MAX_ENDPOINT_BYTES: usize = 2_048;
const MAX_SECRET_BYTES: usize = 16_384;

/// One backend-installed connection profile safe to render during setup.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopSetupProfile {
    /// Opaque profile identity submitted back to the backend.
    pub profile_id: &'static str,
    /// Stable frontend localization key.
    pub display_name_key: &'static str,
    /// Whether an optional explicit endpoint is accepted.
    pub endpoint_mode: &'static str,
    /// Stable neutral capabilities exposed by this profile.
    pub supported_capabilities: Vec<&'static str>,
}

/// Redacted setup choices and backend-enforced limits.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopSetupCatalogue {
    /// Exact catalogue schema version.
    pub schema_version: u32,
    /// Immutable installed catalogue identity.
    pub catalogue_revision: &'static str,
    /// Profiles sorted by opaque identity.
    pub profiles: Vec<DesktopSetupProfile>,
    /// Maximum UTF-8 bytes accepted for one normal text field.
    pub max_text_bytes: usize,
    /// Maximum UTF-8 bytes accepted for an endpoint override.
    pub max_endpoint_bytes: usize,
    /// Maximum credential bytes accepted during commit.
    pub max_secret_bytes: usize,
}

/// Bounded user choices submitted for backend-owned setup planning.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopSetupInput {
    /// Exact setup input schema version.
    pub schema_version: u32,
    /// Caller-generated idempotency nonce.
    pub caller_nonce: String,
    /// Catalogue revision observed by the caller.
    pub catalogue_revision: String,
    /// Opaque installed connection profile.
    pub profile_id: String,
    /// Optional explicit endpoint override.
    pub endpoint_override: Option<String>,
    /// Neutral Runtime model target identity.
    pub model_target_id: String,
    /// Exact provider model identity.
    pub model_id: String,
    /// Neutral deployment identity.
    pub deployment_id: String,
    /// Installed Agent definition identity.
    pub definition_id: String,
}

/// Redacted normalized choices shown for explicit setup review.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopSetupSummary {
    /// Opaque installed profile.
    pub profile_id: String,
    /// Fixed or caller-supplied endpoint selection mode.
    pub endpoint_mode: &'static str,
    /// Caller-visible override when supplied.
    pub endpoint_override: Option<String>,
    /// Neutral Runtime model target identity.
    pub model_target_id: String,
    /// Exact provider model identity.
    pub model_id: String,
    /// Neutral deployment identity.
    pub deployment_id: String,
    /// Installed Agent definition identity.
    pub definition_id: String,
}

/// Immutable non-secret setup plan requiring explicit credential commit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopSetupPlan {
    /// Exact plan schema version.
    pub schema_version: u32,
    /// Fresh opaque setup identity.
    pub setup_id: String,
    /// Caller nonce bound by this plan.
    pub caller_nonce: String,
    /// Installed catalogue revision bound by this plan.
    pub catalogue_revision: &'static str,
    /// Digest of the complete private effective configuration.
    pub effective_configuration_digest: String,
    /// Redacted normalized review value.
    pub summary: DesktopSetupSummary,
    /// Canonical digest of this public plan with this field omitted.
    pub plan_digest: String,
}

/// Non-secret proof that setup committed and requires process restart.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopSetupReceipt {
    /// Exact receipt schema version.
    pub schema_version: u32,
    /// Opaque committed setup identity.
    pub setup_id: String,
    /// Exact committed plan digest.
    pub plan_digest: String,
    /// Monotonic stored configuration revision.
    pub configuration_revision: u64,
    /// Digest of the committed private configuration.
    pub configuration_digest: String,
    /// Setup never hot-swaps the current Runtime.
    pub restart_required: bool,
    /// Canonical digest of this public receipt with this field omitted.
    pub receipt_digest: String,
}

/// Stable secret-free setup failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopSetupError {
    /// Submitted choices, bounds, version, or profile are invalid.
    InputInvalid,
    /// The requested plan is missing or no longer current.
    PlanStale,
    /// Credential storage rejected the bounded secret.
    CredentialRejected,
    /// Configuration could not be committed durably.
    PersistenceFailed,
}

impl DesktopSetupError {
    /// Returns the stable frontend-safe error code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InputInvalid => "setup_input_invalid",
            Self::PlanStale => "setup_plan_stale",
            Self::CredentialRejected => "setup_credential_rejected",
            Self::PersistenceFailed => "setup_persistence_failed",
        }
    }
}

/// Write-only credential store used by setup commit.
pub trait SetupCredentialStore: Send + Sync {
    /// Stores a credential under one fresh opaque reference.
    fn store(&self, credential_ref: &str, credential: &str) -> Result<(), DesktopSetupError>;
}

/// Shipping setup writer backed by the operating-system credential store.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemSetupCredentialStore;

impl SetupCredentialStore for SystemSetupCredentialStore {
    fn store(&self, credential_ref: &str, credential: &str) -> Result<(), DesktopSetupError> {
        let entry = keyring::Entry::new(DESKTOP_CREDENTIAL_SERVICE, credential_ref)
            .map_err(|_| DesktopSetupError::CredentialRejected)?;
        entry
            .set_password(credential)
            .map_err(|_| DesktopSetupError::CredentialRejected)
    }
}

#[derive(Clone)]
struct PreparedSetup {
    public: DesktopSetupPlan,
    configuration: Value,
    credential_ref: String,
}

/// Backend-owned write-only setup planner and committer.
pub struct DesktopSetupService<S> {
    directory: PathBuf,
    credentials: S,
    plans: Mutex<BTreeMap<String, PreparedSetup>>,
}

impl<S: SetupCredentialStore> DesktopSetupService<S> {
    /// Constructs setup around one explicit app configuration directory.
    pub fn new(directory: PathBuf, credentials: S) -> Self {
        Self {
            directory,
            credentials,
            plans: Mutex::new(BTreeMap::new()),
        }
    }

    /// Returns only installed redacted profile metadata and input bounds.
    pub fn catalogue(&self) -> DesktopSetupCatalogue {
        DesktopSetupCatalogue {
            schema_version: 1,
            catalogue_revision: CATALOGUE_REVISION,
            profiles: vec![
                DesktopSetupProfile {
                    profile_id: ANTHROPIC_MESSAGES_PROFILE_ID,
                    display_name_key: "setup.profile.anthropic",
                    endpoint_mode: "optional_override",
                    supported_capabilities: vec!["text"],
                },
                DesktopSetupProfile {
                    profile_id: OPENAI_RESPONSES_PROFILE_ID,
                    display_name_key: "setup.profile.openai",
                    endpoint_mode: "optional_override",
                    supported_capabilities: vec!["text"],
                },
            ],
            max_text_bytes: MAX_TEXT_BYTES,
            max_endpoint_bytes: MAX_ENDPOINT_BYTES,
            max_secret_bytes: MAX_SECRET_BYTES,
        }
    }

    /// Validates choices and returns a redacted immutable review plan.
    pub fn prepare(&self, input: DesktopSetupInput) -> Result<DesktopSetupPlan, DesktopSetupError> {
        validate_input(&input)?;
        let setup_id = format!("setup-{}", Uuid::new_v4());
        let credential_ref = format!("credential-{}", Uuid::new_v4());
        let revision = current_revision(&self.directory)?
            .checked_add(1)
            .ok_or(DesktopSetupError::PlanStale)?;
        let configuration = configuration(&input, &setup_id, &credential_ref, revision);
        let effective_configuration_digest = digest_value(&configuration)?;
        let summary = DesktopSetupSummary {
            profile_id: input.profile_id.clone(),
            endpoint_mode: if input.endpoint_override.is_some() {
                "override"
            } else {
                "fixed"
            },
            endpoint_override: input.endpoint_override.clone(),
            model_target_id: input.model_target_id.clone(),
            model_id: input.model_id.clone(),
            deployment_id: input.deployment_id.clone(),
            definition_id: input.definition_id.clone(),
        };
        let public_value = json!({"schema_version":1,"setup_id":setup_id,"caller_nonce":input.caller_nonce,"catalogue_revision":CATALOGUE_REVISION,"effective_configuration_digest":effective_configuration_digest,"summary":summary});
        let plan_digest = digest_value(&public_value)?;
        let public = DesktopSetupPlan {
            schema_version: 1,
            setup_id,
            caller_nonce: input.caller_nonce,
            catalogue_revision: CATALOGUE_REVISION,
            effective_configuration_digest,
            summary,
            plan_digest: plan_digest.clone(),
        };
        self.plans
            .lock()
            .map_err(|_| DesktopSetupError::PersistenceFailed)?
            .insert(
                plan_digest,
                PreparedSetup {
                    public: public.clone(),
                    configuration,
                    credential_ref,
                },
            );
        Ok(public)
    }

    /// Commits one exact plan and bounded credential without returning either value.
    pub fn commit(
        &self,
        plan_digest: &str,
        credential: &str,
    ) -> Result<DesktopSetupReceipt, DesktopSetupError> {
        if plan_digest.len() != 64 || credential.is_empty() || credential.len() > MAX_SECRET_BYTES {
            return Err(DesktopSetupError::CredentialRejected);
        }
        let prepared = self
            .plans
            .lock()
            .map_err(|_| DesktopSetupError::PersistenceFailed)?
            .get(plan_digest)
            .cloned()
            .ok_or(DesktopSetupError::PlanStale)?;
        if prepared.public.plan_digest != plan_digest
            || digest_value(&prepared.configuration)?
                != prepared.public.effective_configuration_digest
        {
            return Err(DesktopSetupError::PlanStale);
        }
        let revision = prepared.configuration["configuration_revision"]
            .as_u64()
            .ok_or(DesktopSetupError::PlanStale)?;
        if current_revision(&self.directory)? != revision - 1 {
            return Err(DesktopSetupError::PlanStale);
        }
        self.credentials
            .store(&prepared.credential_ref, credential)?;
        let bytes = serde_json::to_vec_pretty(&prepared.configuration)
            .map_err(|_| DesktopSetupError::PersistenceFailed)?;
        atomic_write(
            self.directory.join(DESKTOP_CONFIG_FILE),
            self.directory
                .join(format!(".desktop-setup-{}.tmp", prepared.public.setup_id)),
            &bytes,
        )?;
        let receipt_value = json!({"schema_version":1,"setup_id":prepared.public.setup_id,"plan_digest":plan_digest,"configuration_revision":revision,"configuration_digest":prepared.public.effective_configuration_digest,"restart_required":true});
        let receipt_digest = digest_value(&receipt_value)?;
        let receipt = DesktopSetupReceipt {
            schema_version: 1,
            setup_id: prepared.public.setup_id,
            plan_digest: plan_digest.to_owned(),
            configuration_revision: revision,
            configuration_digest: prepared.public.effective_configuration_digest,
            restart_required: true,
            receipt_digest,
        };
        self.plans
            .lock()
            .map_err(|_| DesktopSetupError::PersistenceFailed)?
            .remove(plan_digest);
        Ok(receipt)
    }
}

fn validate_input(input: &DesktopSetupInput) -> Result<(), DesktopSetupError> {
    let profile = matches!(
        input.profile_id.as_str(),
        OPENAI_RESPONSES_PROFILE_ID | ANTHROPIC_MESSAGES_PROFILE_ID
    );
    let texts = [
        &input.caller_nonce,
        &input.model_target_id,
        &input.model_id,
        &input.deployment_id,
        &input.definition_id,
    ];
    if input.schema_version != 1
        || input.catalogue_revision != CATALOGUE_REVISION
        || !profile
        || texts
            .iter()
            .any(|value| value.is_empty() || value.len() > MAX_TEXT_BYTES)
        || input.endpoint_override.as_ref().is_some_and(|value| {
            value.is_empty() || value.len() > MAX_ENDPOINT_BYTES || url::Url::parse(value).is_err()
        })
    {
        return Err(DesktopSetupError::InputInvalid);
    }
    Ok(())
}

fn digest_value(value: &Value) -> Result<String, DesktopSetupError> {
    let bytes = serde_jcs::to_vec(value).map_err(|_| DesktopSetupError::InputInvalid)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn current_revision(directory: &std::path::Path) -> Result<u64, DesktopSetupError> {
    let path = directory.join(DESKTOP_CONFIG_FILE);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(_) => return Err(DesktopSetupError::PersistenceFailed),
    };
    let config = DesktopSystemConfiguration::parse(&bytes, directory)
        .map_err(|_| DesktopSetupError::PersistenceFailed)?;
    Ok(config.configuration_revision().unwrap_or(0))
}

fn configuration(
    input: &DesktopSetupInput,
    setup_id: &str,
    credential_ref: &str,
    revision: u64,
) -> Value {
    let snapshot_digest = format!(
        "{:x}",
        Sha256::digest(format!("garive-desktop-agent-v1\n{}", input.definition_id).as_bytes())
    );
    json!({"schema_version":2,"configuration_revision":revision,"setup_id":setup_id,"database_file":"garive-desktop.db","installed_agent":{"definition_id":input.definition_id,"definition_revision":"revision-1","snapshot_digest":snapshot_digest,"agent_instance_namespace":format!("desktop-{setup_id}"),"max_iterations":12,"max_input_tokens":131072,"max_output_tokens":8192,"deadline_budget_ms":600000},"host":{"max_command_bytes":65536,"event_batch_size":64,"event_poll_interval_ms":100},"execution":{"profile_id":input.profile_id,"credential_ref":credential_ref,"endpoint":input.endpoint_override,"model_target_id":input.model_target_id,"model_id":input.model_id,"deployment_id":input.deployment_id,"recovery_policy_revision":"desktop-recovery-1","max_output_tokens":8192,"max_context_items":64,"max_context_utf8_bytes":524288,"max_model_attempts":2,"max_context_rebuilds":1,"output_limit_action":"suspend","output_limit_max_retries":null,"transport_action":"suspend","unavailable_action":"suspend","missing_usage_policy":"stop","missing_usage_estimate_input_tokens":null,"missing_usage_estimate_output_tokens":null},"http":{"connect_timeout_ms":10000,"request_timeout_ms":120000,"max_response_bytes":8388608},"dispatch_capacity":8,"execution_lease_duration_ms":30000})
}

fn atomic_write(path: PathBuf, temporary: PathBuf, bytes: &[u8]) -> Result<(), DesktopSetupError> {
    fs::create_dir_all(path.parent().ok_or(DesktopSetupError::PersistenceFailed)?)
        .map_err(|_| DesktopSetupError::PersistenceFailed)?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, &path)?;
        fs::File::open(path.parent().ok_or(std::io::ErrorKind::InvalidInput)?)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(DesktopSetupError::PersistenceFailed);
    }
    Ok(())
}
