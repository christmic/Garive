use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroize;

use crate::{
    DesktopSystemConfiguration, ANTHROPIC_MESSAGES_PROFILE_ID, DESKTOP_CONFIG_FILE,
    DESKTOP_CREDENTIAL_SERVICE, OPENAI_RESPONSES_PROFILE_ID,
};

const CATALOGUE_REVISION: &str = "desktop-setup-catalogue-1";
const BALANCED_PRESET_ID: &str = "desktop-balanced-v1";
const MAX_TEXT_BYTES: usize = 256;
const MAX_ENDPOINT_BYTES: usize = 2_048;
const MAX_SECRET_BYTES: usize = 16_384;
const MAX_RECOVERY_BYTES: usize = 32_768;
const MAX_PREPARED_PLANS: usize = 16;
const PLAN_LIFETIME_SECONDS: i64 = 900;
const RECOVERY_FILE: &str = "desktop-setup-recovery.json";
const RECEIPT_FILE: &str = "desktop-setup-receipt.json";
const RECOVERY_TEMP_FILE: &str = ".desktop-setup-recovery.tmp";

/// One backend-installed connection profile safe to render during setup.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopSetupProfile {
    /// Opaque profile identity submitted back to the backend.
    pub profile_id: &'static str,
    /// Stable frontend localization key.
    pub display_name_key: &'static str,
    /// Whether an optional explicit endpoint is accepted.
    pub endpoint_mode: &'static str,
    /// Model selection accepts one exact caller-supplied identity.
    pub model_mode: &'static str,
    /// Stable localization key for the write-only credential field.
    pub credential_label_key: &'static str,
    /// Stable neutral capabilities exposed by this profile.
    pub supported_capabilities: Vec<&'static str>,
}

/// One backend-owned immutable Runtime policy preset.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopSetupPreset {
    /// Opaque preset identity submitted back to the backend.
    pub preset_id: &'static str,
    /// Stable frontend localization key.
    pub display_name_key: &'static str,
    /// Installed profiles accepted by this preset in stable order.
    pub supported_profile_ids: Vec<&'static str>,
}

/// Complete non-zero setup input and lifecycle bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopSetupLimits {
    /// Maximum installed profiles returned by the catalogue.
    pub max_profiles: usize,
    /// Maximum UTF-8 bytes accepted for one normal text field.
    pub max_text_bytes: usize,
    /// Maximum UTF-8 bytes accepted for an endpoint override.
    pub max_endpoint_bytes: usize,
    /// Maximum credential bytes accepted during commit.
    pub max_secret_bytes: usize,
    /// Maximum live prepared plans retained in memory.
    pub max_plan_count: usize,
    /// Lifetime of one prepared plan in whole seconds.
    pub plan_lifetime_seconds: i64,
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
    /// Backend-owned immutable Runtime policy presets.
    pub presets: Vec<DesktopSetupPreset>,
    /// Complete setup input and lifecycle bounds.
    pub limits: DesktopSetupLimits,
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
    /// Backend-owned immutable policy preset.
    pub preset_id: String,
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
    /// Opaque backend-owned policy preset.
    pub preset_id: String,
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
    /// Configuration revision observed while preparing, absent before v2.
    pub expected_configuration_revision: Option<u64>,
    /// Canonical current configuration digest, absent on first setup.
    pub expected_configuration_digest: Option<String>,
    /// Digest of the complete private effective configuration.
    pub effective_configuration_digest: String,
    /// Canonical UTC instant after which this plan cannot commit.
    pub expires_at: String,
    /// Redacted normalized review value.
    pub summary: DesktopSetupSummary,
    /// Canonical digest of this public plan with this field omitted.
    pub plan_digest: String,
}

/// Non-secret proof that setup committed and requires process restart.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
    /// A caller nonce was reused with different normalized choices.
    PlanConflict,
    /// Credential storage rejected the bounded secret.
    CredentialRejected,
    /// Configuration could not be committed durably.
    PersistenceFailed,
    /// A staged setup could not be classified or repaired safely.
    RecoveryFailed,
}

impl DesktopSetupError {
    /// Returns the stable frontend-safe error code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InputInvalid => "setup_input_invalid",
            Self::PlanStale => "setup_plan_stale",
            Self::PlanConflict => "setup_plan_conflict",
            Self::CredentialRejected => "setup_credential_rejected",
            Self::PersistenceFailed => "setup_persistence_failed",
            Self::RecoveryFailed => "setup_recovery_failed",
        }
    }
}

/// Public result of cancelling one prepared setup plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSetupCancellation {
    /// The prepared plan was removed before it touched the credential store.
    Cancelled,
    /// The exact plan already committed and cannot be cancelled.
    AlreadyCommitted,
}

/// Redacted setup lifecycle state exposed to the Desktop frontend.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DesktopSetupState {
    /// No stored Desktop configuration exists.
    NotConfigured,
    /// One valid configuration exists; a new commit may require restart.
    Configured {
        /// Whether the process still runs the prior immutable Runtime snapshot.
        restart_required: bool,
    },
    /// Stored configuration could not construct Runtime.
    InvalidConfiguration {
        /// Stable secret-free configuration failure code.
        code: String,
    },
    /// Startup is classifying or repairing one staged setup.
    SetupRecovering,
}

/// Write-only credential value that cannot be serialized, cloned, or debug-formatted.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct SensitiveSetupCredential(String);

impl SensitiveSetupCredential {
    /// Borrows the credential only for the credential-store commit boundary.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl Drop for SensitiveSetupCredential {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Backend-owned clock used to freeze and validate setup expiry.
pub trait SetupClock: Send + Sync {
    /// Returns whole UTC seconds since the Unix epoch.
    fn unix_seconds(&self) -> Result<i64, DesktopSetupError>;
}

/// Backend-owned source for opaque setup and credential-reference identities.
pub trait SetupIdentitySource: Send + Sync {
    /// Returns one fresh opaque public setup identity.
    fn setup_id(&self) -> Result<String, DesktopSetupError>;
    /// Returns one fresh opaque private credential reference.
    fn credential_ref(&self) -> Result<String, DesktopSetupError>;
}

/// Shipping setup identity source backed by cryptographically random UUIDs.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemSetupIdentitySource;

impl SetupIdentitySource for SystemSetupIdentitySource {
    fn setup_id(&self) -> Result<String, DesktopSetupError> {
        Ok(format!("setup-{}", Uuid::new_v4()))
    }

    fn credential_ref(&self) -> Result<String, DesktopSetupError> {
        Ok(format!("credential-{}", Uuid::new_v4()))
    }
}

/// Durable setup commit stages available to deterministic fault injection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupCommitStage {
    /// Recovery journal committed before credential storage.
    Planned,
    /// New credential stored and journal advanced.
    CredentialStored,
    /// Strict v2 configuration atomically committed.
    ConfigCommitted,
    /// Non-secret receipt committed and journal advanced.
    ReceiptCommitted,
    /// Obsolete credential cleanup remains after Runtime restart.
    CleanupPending,
}

/// Injected fault boundary used to prove every staged commit recovery outcome.
pub trait SetupCommitFaults: Send + Sync {
    /// Returns an injected failure immediately after one durable stage.
    fn after_stage(&self, stage: SetupCommitStage) -> Result<(), DesktopSetupError>;
}

/// Shipping no-op setup fault boundary.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoSetupCommitFaults;

impl SetupCommitFaults for NoSetupCommitFaults {
    fn after_stage(&self, _stage: SetupCommitStage) -> Result<(), DesktopSetupError> {
        Ok(())
    }
}

/// Shipping setup clock backed by the operating system wall clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemSetupClock;

impl SetupClock for SystemSetupClock {
    fn unix_seconds(&self) -> Result<i64, DesktopSetupError> {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| DesktopSetupError::PersistenceFailed)?
            .as_secs();
        i64::try_from(seconds).map_err(|_| DesktopSetupError::PersistenceFailed)
    }
}

/// Write-only credential store used by setup commit.
pub trait SetupCredentialStore: Send + Sync {
    /// Stores a credential under one fresh opaque reference.
    fn store(&self, credential_ref: &str, credential: &str) -> Result<(), DesktopSetupError>;
    /// Removes one exact obsolete or uncommitted credential reference.
    fn delete(&self, credential_ref: &str) -> Result<(), DesktopSetupError>;
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

    fn delete(&self, credential_ref: &str) -> Result<(), DesktopSetupError> {
        let entry = keyring::Entry::new(DESKTOP_CREDENTIAL_SERVICE, credential_ref)
            .map_err(|_| DesktopSetupError::RecoveryFailed)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(DesktopSetupError::RecoveryFailed),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RecoveryStage {
    Planned,
    CredentialStored,
    ConfigCommitted,
    ReceiptCommitted,
    CleanupPending,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecoveryJournal {
    schema_version: u32,
    setup_id: String,
    plan_digest: String,
    configuration_digest: String,
    configuration_revision: u64,
    new_credential_ref: String,
    old_credential_ref: Option<String>,
    stage: RecoveryStage,
}

#[derive(Clone)]
struct PreparedSetup {
    public: DesktopSetupPlan,
    configuration: Value,
    credential_ref: String,
    input_digest: String,
    expires_at_unix: i64,
}

#[derive(Default)]
struct PreparedPlans {
    by_digest: BTreeMap<String, PreparedSetup>,
    by_nonce: BTreeMap<String, String>,
}

/// Backend-owned write-only setup planner and committer.
pub struct DesktopSetupService<S> {
    directory: PathBuf,
    credentials: S,
    clock: Arc<dyn SetupClock>,
    identities: Arc<dyn SetupIdentitySource>,
    faults: Arc<dyn SetupCommitFaults>,
    plans: Mutex<PreparedPlans>,
    state: Mutex<DesktopSetupState>,
}

impl<S: SetupCredentialStore> DesktopSetupService<S> {
    /// Constructs setup around one explicit app configuration directory.
    pub fn new(directory: PathBuf, credentials: S) -> Self {
        Self::with_clock(directory, credentials, Arc::new(SystemSetupClock))
    }

    /// Constructs setup with an explicit clock for deterministic expiry checks.
    pub fn with_clock(directory: PathBuf, credentials: S, clock: Arc<dyn SetupClock>) -> Self {
        Self::with_dependencies(
            directory,
            credentials,
            clock,
            Arc::new(SystemSetupIdentitySource),
            Arc::new(NoSetupCommitFaults),
        )
    }

    /// Constructs setup with every nondeterministic and crash boundary injected.
    pub fn with_dependencies(
        directory: PathBuf,
        credentials: S,
        clock: Arc<dyn SetupClock>,
        identities: Arc<dyn SetupIdentitySource>,
        faults: Arc<dyn SetupCommitFaults>,
    ) -> Self {
        Self {
            directory,
            credentials,
            clock,
            identities,
            faults,
            plans: Mutex::new(PreparedPlans::default()),
            state: Mutex::new(DesktopSetupState::SetupRecovering),
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
                    model_mode: "exact_id",
                    credential_label_key: "setup.credential.connection",
                    supported_capabilities: vec!["text"],
                },
                DesktopSetupProfile {
                    profile_id: OPENAI_RESPONSES_PROFILE_ID,
                    display_name_key: "setup.profile.openai",
                    endpoint_mode: "optional_override",
                    model_mode: "exact_id",
                    credential_label_key: "setup.credential.connection",
                    supported_capabilities: vec!["text"],
                },
            ],
            presets: vec![DesktopSetupPreset {
                preset_id: BALANCED_PRESET_ID,
                display_name_key: "setup.preset.balanced",
                supported_profile_ids: vec![
                    ANTHROPIC_MESSAGES_PROFILE_ID,
                    OPENAI_RESPONSES_PROFILE_ID,
                ],
            }],
            limits: DesktopSetupLimits {
                max_profiles: 2,
                max_text_bytes: MAX_TEXT_BYTES,
                max_endpoint_bytes: MAX_ENDPOINT_BYTES,
                max_secret_bytes: MAX_SECRET_BYTES,
                max_plan_count: MAX_PREPARED_PLANS,
                plan_lifetime_seconds: PLAN_LIFETIME_SECONDS,
            },
        }
    }

    /// Returns the current redacted setup lifecycle state.
    pub fn state(&self) -> DesktopSetupState {
        self.state.lock().map(|state| state.clone()).unwrap_or(
            DesktopSetupState::InvalidConfiguration {
                code: "setup_recovery_failed".to_owned(),
            },
        )
    }

    /// Publishes the secret-free result after recovery and Runtime construction.
    pub fn complete_startup(
        &self,
        runtime_started: bool,
        invalid_code: Option<&str>,
    ) -> Result<(), DesktopSetupError> {
        let state = match (runtime_started, invalid_code) {
            (_, Some(code)) if !code.is_empty() && code.len() <= MAX_TEXT_BYTES => {
                DesktopSetupState::InvalidConfiguration {
                    code: code.to_owned(),
                }
            }
            (true, None) => DesktopSetupState::Configured {
                restart_required: false,
            },
            (false, None) => DesktopSetupState::NotConfigured,
            _ => return Err(DesktopSetupError::RecoveryFailed),
        };
        *self
            .state
            .lock()
            .map_err(|_| DesktopSetupError::RecoveryFailed)? = state;
        Ok(())
    }

    /// Validates choices and returns a redacted immutable review plan.
    pub fn prepare(
        &self,
        mut input: DesktopSetupInput,
    ) -> Result<DesktopSetupPlan, DesktopSetupError> {
        normalize_input(&mut input);
        validate_input(&input)?;
        let now = self.clock.unix_seconds()?;
        let input_digest = digest_value(
            &serde_json::to_value(&input).map_err(|_| DesktopSetupError::InputInvalid)?,
        )?;
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| DesktopSetupError::PersistenceFailed)?;
        plans
            .by_digest
            .retain(|_, plan| plan.expires_at_unix >= now);
        let live_digests = plans.by_digest.keys().cloned().collect::<Vec<_>>();
        plans
            .by_nonce
            .retain(|_, digest| live_digests.binary_search(digest).is_ok());
        if let Some(digest) = plans.by_nonce.get(&input.caller_nonce) {
            let prepared = plans
                .by_digest
                .get(digest)
                .ok_or(DesktopSetupError::PersistenceFailed)?;
            return if prepared.input_digest == input_digest {
                Ok(prepared.public.clone())
            } else {
                Err(DesktopSetupError::PlanConflict)
            };
        }
        if plans.by_digest.len() >= MAX_PREPARED_PLANS {
            return Err(DesktopSetupError::InputInvalid);
        }
        let setup_id = self.identities.setup_id()?;
        let credential_ref = self.identities.credential_ref()?;
        let (expected_configuration_revision, expected_configuration_digest) =
            current_configuration_binding(&self.directory)?;
        let revision = expected_configuration_revision
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(DesktopSetupError::PlanStale)?;
        let configuration = configuration(&input, &setup_id, &credential_ref, revision);
        let effective_configuration_digest = digest_value(&configuration)?;
        let expires_at_unix = now
            .checked_add(PLAN_LIFETIME_SECONDS)
            .ok_or(DesktopSetupError::PersistenceFailed)?;
        let expires_at = DateTime::<Utc>::from_timestamp(expires_at_unix, 0)
            .ok_or(DesktopSetupError::PersistenceFailed)?
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        let summary = DesktopSetupSummary {
            preset_id: input.preset_id.clone(),
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
        let public_value = json!({"schema_version":1,"setup_id":setup_id,"caller_nonce":input.caller_nonce,"catalogue_revision":CATALOGUE_REVISION,"expected_configuration_revision":expected_configuration_revision,"expected_configuration_digest":expected_configuration_digest,"effective_configuration_digest":effective_configuration_digest,"expires_at":expires_at,"summary":summary});
        let plan_digest = digest_value(&public_value)?;
        let public = DesktopSetupPlan {
            schema_version: 1,
            setup_id,
            caller_nonce: input.caller_nonce,
            catalogue_revision: CATALOGUE_REVISION,
            expected_configuration_revision,
            expected_configuration_digest,
            effective_configuration_digest,
            expires_at,
            summary,
            plan_digest: plan_digest.clone(),
        };
        plans
            .by_nonce
            .insert(public.caller_nonce.clone(), plan_digest.clone());
        plans.by_digest.insert(
            plan_digest,
            PreparedSetup {
                public: public.clone(),
                configuration,
                credential_ref,
                input_digest,
                expires_at_unix,
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
        if !valid_digest(plan_digest) {
            return Err(DesktopSetupError::PlanStale);
        }
        if credential.is_empty() || credential.len() > MAX_SECRET_BYTES {
            return Err(DesktopSetupError::CredentialRejected);
        }
        if let Some(receipt) = committed_receipt(&self.directory, plan_digest)? {
            return Ok(receipt);
        }
        let now = self.clock.unix_seconds()?;
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| DesktopSetupError::PersistenceFailed)?;
        let prepared = plans
            .by_digest
            .get(plan_digest)
            .ok_or(DesktopSetupError::PlanStale)?;
        if prepared.expires_at_unix < now {
            let nonce = prepared.public.caller_nonce.clone();
            plans.by_digest.remove(plan_digest);
            plans.by_nonce.remove(&nonce);
            return Err(DesktopSetupError::PlanStale);
        }
        if prepared.public.plan_digest != plan_digest
            || digest_value(&prepared.configuration)?
                != prepared.public.effective_configuration_digest
        {
            return Err(DesktopSetupError::PlanStale);
        }
        let revision = prepared.configuration["configuration_revision"]
            .as_u64()
            .ok_or(DesktopSetupError::PlanStale)?;
        if current_configuration_binding(&self.directory)?
            != (
                prepared.public.expected_configuration_revision,
                prepared.public.expected_configuration_digest.clone(),
            )
        {
            return Err(DesktopSetupError::PlanStale);
        }
        let mut journal = RecoveryJournal {
            schema_version: 1,
            setup_id: prepared.public.setup_id.clone(),
            plan_digest: plan_digest.to_owned(),
            configuration_digest: prepared.public.effective_configuration_digest.clone(),
            configuration_revision: revision,
            new_credential_ref: prepared.credential_ref.clone(),
            old_credential_ref: current_credential_ref(&self.directory)?,
            stage: RecoveryStage::Planned,
        };
        write_journal(&self.directory, &journal)?;
        self.faults.after_stage(SetupCommitStage::Planned)?;
        self.credentials
            .store(&prepared.credential_ref, credential)?;
        journal.stage = RecoveryStage::CredentialStored;
        write_journal(&self.directory, &journal)?;
        self.faults
            .after_stage(SetupCommitStage::CredentialStored)?;
        let bytes = serde_json::to_vec_pretty(&prepared.configuration)
            .map_err(|_| DesktopSetupError::PersistenceFailed)?;
        atomic_write(
            self.directory.join(DESKTOP_CONFIG_FILE),
            self.directory
                .join(format!(".desktop-setup-{}.tmp", prepared.public.setup_id)),
            &bytes,
        )?;
        journal.stage = RecoveryStage::ConfigCommitted;
        write_journal(&self.directory, &journal)?;
        self.faults.after_stage(SetupCommitStage::ConfigCommitted)?;
        let receipt = receipt(&journal)?;
        let receipt_bytes = serde_json::to_vec_pretty(&receipt)
            .map_err(|_| DesktopSetupError::PersistenceFailed)?;
        atomic_write(
            self.directory.join(RECEIPT_FILE),
            self.directory
                .join(format!(".desktop-setup-receipt-{}.tmp", journal.setup_id)),
            &receipt_bytes,
        )?;
        journal.stage = RecoveryStage::ReceiptCommitted;
        write_journal(&self.directory, &journal)?;
        self.faults
            .after_stage(SetupCommitStage::ReceiptCommitted)?;
        let nonce = prepared.public.caller_nonce.clone();
        plans.by_digest.remove(plan_digest);
        plans.by_nonce.remove(&nonce);
        if journal.old_credential_ref.is_some() {
            journal.stage = RecoveryStage::CleanupPending;
            write_journal(&self.directory, &journal)?;
            self.faults.after_stage(SetupCommitStage::CleanupPending)?;
        } else {
            remove_recovery(&self.directory)?;
        }
        *self
            .state
            .lock()
            .map_err(|_| DesktopSetupError::PersistenceFailed)? = DesktopSetupState::Configured {
            restart_required: true,
        };
        Ok(receipt)
    }

    /// Cancels one unexpired plan without touching credentials or configuration.
    pub fn cancel(&self, plan_digest: &str) -> Result<DesktopSetupCancellation, DesktopSetupError> {
        if committed_receipt(&self.directory, plan_digest)?.is_some() {
            return Ok(DesktopSetupCancellation::AlreadyCommitted);
        }
        let now = self.clock.unix_seconds()?;
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| DesktopSetupError::PersistenceFailed)?;
        let prepared = plans
            .by_digest
            .remove(plan_digest)
            .ok_or(DesktopSetupError::PlanStale)?;
        plans.by_nonce.remove(&prepared.public.caller_nonce);
        if prepared.expires_at_unix < now {
            return Err(DesktopSetupError::PlanStale);
        }
        Ok(DesktopSetupCancellation::Cancelled)
    }

    /// Repairs or rolls back one staged setup before Desktop admits Agent IPC.
    pub fn recover(&self, runtime_started: bool) -> Result<(), DesktopSetupError> {
        let Some(mut journal) = read_journal(&self.directory)? else {
            remove_known_temporary(&self.directory, None);
            return Ok(());
        };
        validate_journal(&journal)?;
        remove_known_temporary(&self.directory, Some(&journal.setup_id));
        if configuration_matches(&self.directory, &journal)? {
            let expected = receipt(&journal)?;
            if !receipt_matches(&self.directory, &expected)? {
                let bytes = serde_json::to_vec_pretty(&expected)
                    .map_err(|_| DesktopSetupError::RecoveryFailed)?;
                atomic_write(
                    self.directory.join(RECEIPT_FILE),
                    self.directory
                        .join(format!(".desktop-setup-receipt-{}.tmp", journal.setup_id)),
                    &bytes,
                )
                .map_err(|_| DesktopSetupError::RecoveryFailed)?;
            }
            if runtime_started {
                if let Some(old) = journal.old_credential_ref.as_deref() {
                    self.credentials.delete(old)?;
                }
                remove_recovery(&self.directory)?;
            } else {
                journal.stage = RecoveryStage::CleanupPending;
                write_journal(&self.directory, &journal)?;
            }
            return Ok(());
        }
        if matches!(
            journal.stage,
            RecoveryStage::ConfigCommitted
                | RecoveryStage::ReceiptCommitted
                | RecoveryStage::CleanupPending
        ) {
            return Err(DesktopSetupError::RecoveryFailed);
        }
        self.credentials.delete(&journal.new_credential_ref)?;
        remove_recovery(&self.directory)
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
        || input.preset_id != BALANCED_PRESET_ID
        || !profile
        || texts
            .iter()
            .any(|value| value.is_empty() || value.len() > MAX_TEXT_BYTES)
        || input
            .endpoint_override
            .as_ref()
            .is_some_and(|value| !valid_endpoint(value))
    {
        return Err(DesktopSetupError::InputInvalid);
    }
    Ok(())
}

fn normalize_input(input: &mut DesktopSetupInput) {
    input.caller_nonce = input.caller_nonce.trim().to_owned();
    input.catalogue_revision = input.catalogue_revision.trim().to_owned();
    input.preset_id = input.preset_id.trim().to_owned();
    input.profile_id = input.profile_id.trim().to_owned();
    input.endpoint_override = input
        .endpoint_override
        .take()
        .map(|value| value.trim().to_owned());
    input.model_target_id = input.model_target_id.trim().to_owned();
    input.model_id = input.model_id.trim().to_owned();
    input.deployment_id = input.deployment_id.trim().to_owned();
    input.definition_id = input.definition_id.trim().to_owned();
}

fn valid_endpoint(value: &str) -> bool {
    let Ok(endpoint) = url::Url::parse(value) else {
        return false;
    };
    value.len() <= MAX_ENDPOINT_BYTES
        && matches!(endpoint.scheme(), "http" | "https")
        && endpoint.host_str().is_some()
        && endpoint.username().is_empty()
        && endpoint.password().is_none()
        && endpoint.fragment().is_none()
}

fn digest_value(value: &Value) -> Result<String, DesktopSetupError> {
    let bytes = serde_jcs::to_vec(value).map_err(|_| DesktopSetupError::InputInvalid)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn current_configuration_binding(
    directory: &std::path::Path,
) -> Result<(Option<u64>, Option<String>), DesktopSetupError> {
    let path = directory.join(DESKTOP_CONFIG_FILE);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((None, None)),
        Err(_) => return Err(DesktopSetupError::PersistenceFailed),
    };
    let config = DesktopSystemConfiguration::parse(&bytes, directory)
        .map_err(|_| DesktopSetupError::PersistenceFailed)?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| DesktopSetupError::PersistenceFailed)?;
    Ok((config.configuration_revision(), Some(digest_value(&value)?)))
}

fn current_credential_ref(
    directory: &std::path::Path,
) -> Result<Option<String>, DesktopSetupError> {
    let bytes = match fs::read(directory.join(DESKTOP_CONFIG_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(DesktopSetupError::PersistenceFailed),
    };
    let config = DesktopSystemConfiguration::parse(&bytes, directory)
        .map_err(|_| DesktopSetupError::PersistenceFailed)?;
    Ok(Some(config.credential_ref().to_owned()))
}

fn receipt(journal: &RecoveryJournal) -> Result<DesktopSetupReceipt, DesktopSetupError> {
    let value = json!({"schema_version":1,"setup_id":journal.setup_id,"plan_digest":journal.plan_digest,"configuration_revision":journal.configuration_revision,"configuration_digest":journal.configuration_digest,"restart_required":true});
    Ok(DesktopSetupReceipt {
        schema_version: 1,
        setup_id: journal.setup_id.clone(),
        plan_digest: journal.plan_digest.clone(),
        configuration_revision: journal.configuration_revision,
        configuration_digest: journal.configuration_digest.clone(),
        restart_required: true,
        receipt_digest: digest_value(&value)?,
    })
}

fn write_journal(
    directory: &std::path::Path,
    journal: &RecoveryJournal,
) -> Result<(), DesktopSetupError> {
    let bytes =
        serde_json::to_vec_pretty(journal).map_err(|_| DesktopSetupError::PersistenceFailed)?;
    if bytes.len() > MAX_RECOVERY_BYTES {
        return Err(DesktopSetupError::PersistenceFailed);
    }
    atomic_write(
        directory.join(RECOVERY_FILE),
        directory.join(RECOVERY_TEMP_FILE),
        &bytes,
    )
}

fn read_journal(directory: &std::path::Path) -> Result<Option<RecoveryJournal>, DesktopSetupError> {
    let path = directory.join(RECOVERY_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(DesktopSetupError::RecoveryFailed),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_RECOVERY_BYTES as u64
    {
        return Err(DesktopSetupError::RecoveryFailed);
    }
    let bytes = fs::read(path).map_err(|_| DesktopSetupError::RecoveryFailed)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| DesktopSetupError::RecoveryFailed)
}

fn validate_journal(journal: &RecoveryJournal) -> Result<(), DesktopSetupError> {
    if journal.schema_version != 1
        || journal.setup_id.is_empty()
        || journal.configuration_revision == 0
        || journal.new_credential_ref.is_empty()
        || journal.old_credential_ref.as_deref() == Some("")
        || !valid_digest(&journal.plan_digest)
        || !valid_digest(&journal.configuration_digest)
    {
        return Err(DesktopSetupError::RecoveryFailed);
    }
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn configuration_matches(
    directory: &std::path::Path,
    journal: &RecoveryJournal,
) -> Result<bool, DesktopSetupError> {
    let bytes = match fs::read(directory.join(DESKTOP_CONFIG_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(DesktopSetupError::RecoveryFailed),
    };
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| DesktopSetupError::RecoveryFailed)?;
    let config = DesktopSystemConfiguration::parse(&bytes, directory)
        .map_err(|_| DesktopSetupError::RecoveryFailed)?;
    Ok(config.setup_id() == Some(journal.setup_id.as_str())
        && config.configuration_revision() == Some(journal.configuration_revision)
        && digest_value(&value).map_err(|_| DesktopSetupError::RecoveryFailed)?
            == journal.configuration_digest)
}

fn receipt_matches(
    directory: &std::path::Path,
    expected: &DesktopSetupReceipt,
) -> Result<bool, DesktopSetupError> {
    let bytes = match fs::read(directory.join(RECEIPT_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(DesktopSetupError::RecoveryFailed),
    };
    let actual: DesktopSetupReceipt =
        serde_json::from_slice(&bytes).map_err(|_| DesktopSetupError::RecoveryFailed)?;
    Ok(&actual == expected)
}

fn committed_receipt(
    directory: &std::path::Path,
    plan_digest: &str,
) -> Result<Option<DesktopSetupReceipt>, DesktopSetupError> {
    let bytes = match fs::read(directory.join(RECEIPT_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(DesktopSetupError::PersistenceFailed),
    };
    let receipt: DesktopSetupReceipt =
        serde_json::from_slice(&bytes).map_err(|_| DesktopSetupError::PersistenceFailed)?;
    if receipt.plan_digest == plan_digest {
        validate_committed_receipt(directory, &receipt)?;
        Ok(Some(receipt))
    } else {
        Ok(None)
    }
}

fn validate_committed_receipt(
    directory: &std::path::Path,
    receipt: &DesktopSetupReceipt,
) -> Result<(), DesktopSetupError> {
    let value = json!({
        "schema_version": receipt.schema_version,
        "setup_id": receipt.setup_id,
        "plan_digest": receipt.plan_digest,
        "configuration_revision": receipt.configuration_revision,
        "configuration_digest": receipt.configuration_digest,
        "restart_required": receipt.restart_required,
    });
    if receipt.schema_version != 1
        || !receipt.restart_required
        || !valid_digest(&receipt.plan_digest)
        || !valid_digest(&receipt.configuration_digest)
        || !valid_digest(&receipt.receipt_digest)
        || digest_value(&value)? != receipt.receipt_digest
    {
        return Err(DesktopSetupError::PersistenceFailed);
    }
    let bytes = fs::read(directory.join(DESKTOP_CONFIG_FILE))
        .map_err(|_| DesktopSetupError::PersistenceFailed)?;
    let config = DesktopSystemConfiguration::parse(&bytes, directory)
        .map_err(|_| DesktopSetupError::PersistenceFailed)?;
    let config_value: Value =
        serde_json::from_slice(&bytes).map_err(|_| DesktopSetupError::PersistenceFailed)?;
    if config.setup_id() != Some(receipt.setup_id.as_str())
        || config.configuration_revision() != Some(receipt.configuration_revision)
        || digest_value(&config_value)? != receipt.configuration_digest
    {
        return Err(DesktopSetupError::PersistenceFailed);
    }
    Ok(())
}

fn remove_known_temporary(directory: &std::path::Path, setup_id: Option<&str>) {
    let _ = fs::remove_file(directory.join(RECOVERY_TEMP_FILE));
    if let Some(setup_id) = setup_id {
        let _ = fs::remove_file(directory.join(format!(".desktop-setup-{setup_id}.tmp")));
        let _ = fs::remove_file(directory.join(format!(".desktop-setup-receipt-{setup_id}.tmp")));
    }
}

fn remove_recovery(directory: &std::path::Path) -> Result<(), DesktopSetupError> {
    match fs::remove_file(directory.join(RECOVERY_FILE)) {
        Ok(()) => fs::File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|_| DesktopSetupError::RecoveryFailed),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(DesktopSetupError::RecoveryFailed),
    }
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
