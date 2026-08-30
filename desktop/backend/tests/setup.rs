use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc, Mutex,
};

use garive_desktop::{
    authorize_setup_window, DesktopSetupCancellation, DesktopSetupError, DesktopSetupInput,
    DesktopSetupService, DesktopSetupState, DesktopSystemConfiguration, NoSetupCommitFaults,
    SensitiveSetupCredential, SetupClock, SetupCommitFaults, SetupCommitStage,
    SetupCredentialStore, SetupIdentitySource, OPENAI_RESPONSES_PROFILE_ID,
};
use serde::Deserialize;

#[derive(Clone, Default)]
struct RecordingCredentials(Arc<Mutex<Vec<(String, String)>>>);

impl SetupCredentialStore for RecordingCredentials {
    fn store(&self, credential_ref: &str, credential: &str) -> Result<(), DesktopSetupError> {
        self.0
            .lock()
            .map_err(|_| DesktopSetupError::CredentialRejected)?
            .push((credential_ref.to_owned(), credential.to_owned()));
        Ok(())
    }

    fn delete(&self, credential_ref: &str) -> Result<(), DesktopSetupError> {
        self.0
            .lock()
            .map_err(|_| DesktopSetupError::RecoveryFailed)?
            .push((credential_ref.to_owned(), "<deleted>".into()));
        Ok(())
    }
}

#[derive(Default)]
struct FixedClock(AtomicI64);

impl FixedClock {
    fn at(unix_seconds: i64) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(unix_seconds)))
    }

    fn set(&self, unix_seconds: i64) {
        self.0.store(unix_seconds, Ordering::SeqCst);
    }
}

impl SetupClock for FixedClock {
    fn unix_seconds(&self) -> Result<i64, DesktopSetupError> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}

struct FixedIdentities;

impl SetupIdentitySource for FixedIdentities {
    fn setup_id(&self) -> Result<String, DesktopSetupError> {
        Ok("setup-fixture".into())
    }

    fn credential_ref(&self) -> Result<String, DesktopSetupError> {
        Ok("credential-fixture".into())
    }
}

struct FailAfter(SetupCommitStage);

impl SetupCommitFaults for FailAfter {
    fn after_stage(&self, stage: SetupCommitStage) -> Result<(), DesktopSetupError> {
        if stage == self.0 {
            Err(DesktopSetupError::PersistenceFailed)
        } else {
            Ok(())
        }
    }
}

struct FixtureIdentities {
    setup_id: String,
    credential_ref: String,
}

impl SetupIdentitySource for FixtureIdentities {
    fn setup_id(&self) -> Result<String, DesktopSetupError> {
        Ok(self.setup_id.clone())
    }

    fn credential_ref(&self) -> Result<String, DesktopSetupError> {
        Ok(self.credential_ref.clone())
    }
}

#[derive(Deserialize)]
struct SetupFixture {
    schema_version: u32,
    catalogue_revision: String,
    expected_profile_count: usize,
    expected_preset_id: String,
    limits: FixtureLimits,
    plan_cases: Vec<FixturePlanCase>,
    failure_codes: Vec<String>,
    receipt_forbidden_fragments: Vec<String>,
}

#[derive(Deserialize)]
struct FixtureLimits {
    max_profiles: usize,
    max_text_bytes: usize,
    max_endpoint_bytes: usize,
    max_secret_bytes: usize,
    max_plan_count: usize,
    plan_lifetime_seconds: i64,
}

#[derive(Deserialize)]
struct FixturePlanCase {
    name: String,
    now_unix: i64,
    setup_id: String,
    credential_ref: String,
    input: DesktopSetupInput,
    expected_expires_at: String,
    expected_endpoint_mode: String,
    effective_configuration_digest: String,
    plan_digest: String,
}

fn input(revision: &str) -> DesktopSetupInput {
    DesktopSetupInput {
        schema_version: 1,
        caller_nonce: format!("nonce-{revision}"),
        catalogue_revision: "desktop-setup-catalogue-1".into(),
        preset_id: "desktop-balanced-v1".into(),
        profile_id: OPENAI_RESPONSES_PROFILE_ID.into(),
        endpoint_override: None,
        model_target_id: "desktop-target".into(),
        model_id: "gpt-5".into(),
        deployment_id: "desktop-deployment".into(),
        definition_id: "garive-work".into(),
    }
}

#[test]
fn catalogue_plan_and_commit_are_redacted_and_restart_safe() {
    let directory = tempfile::tempdir().unwrap();
    let credentials = RecordingCredentials::default();
    let service = DesktopSetupService::new(directory.path().to_owned(), credentials.clone());
    let catalogue = service.catalogue();
    assert_eq!(catalogue.schema_version, 1);
    assert!(catalogue.profiles[0].profile_id < catalogue.profiles[1].profile_id);
    assert_eq!(catalogue.presets.len(), 1);
    assert_eq!(catalogue.presets[0].preset_id, "desktop-balanced-v1");
    assert_eq!(catalogue.limits.max_plan_count, 16);
    assert_eq!(catalogue.limits.plan_lifetime_seconds, 900);
    assert_eq!(catalogue.profiles[0].model_mode, "exact_id");

    let plan = service.prepare(input("1")).unwrap();
    assert_eq!(plan.summary.preset_id, "desktop-balanced-v1");
    assert_eq!(plan.expected_configuration_revision, None);
    assert_eq!(plan.expected_configuration_digest, None);
    assert_eq!(plan.summary.model_id, "gpt-5");
    assert_eq!(plan.plan_digest.len(), 64);
    let encoded = serde_json::to_string(&plan).unwrap();
    assert!(!encoded.contains("credential-"));
    assert!(!encoded.contains("private-api-key"));
    let receipt = service
        .commit(&plan.plan_digest, "private-api-key")
        .unwrap();
    assert!(receipt.restart_required);
    assert_eq!(receipt.configuration_revision, 1);
    assert_eq!(receipt.receipt_digest.len(), 64);

    let stored = credentials.0.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].1, "private-api-key");
    drop(stored);
    let bytes = std::fs::read(directory.path().join("desktop-v1.json")).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("private-api-key"));
    let config = DesktopSystemConfiguration::parse(&bytes, directory.path()).unwrap();
    assert_eq!(config.schema_version(), 2);
    assert_eq!(config.configuration_revision(), Some(1));
    assert_eq!(config.setup_id(), Some(plan.setup_id.as_str()));
    assert_eq!(
        service.state(),
        DesktopSetupState::Configured {
            restart_required: true
        }
    );
}

#[test]
fn injected_clock_and_identities_freeze_public_plan() {
    let directory = tempfile::tempdir().unwrap();
    let service = DesktopSetupService::with_dependencies(
        directory.path().to_owned(),
        RecordingCredentials::default(),
        FixedClock::at(1_800_000_000),
        Arc::new(FixedIdentities),
        Arc::new(NoSetupCommitFaults),
    );
    let plan = service.prepare(input("fixture")).unwrap();
    assert_eq!(plan.setup_id, "setup-fixture");
    assert_eq!(plan.expires_at, "2027-01-15T08:15:00Z");
}

#[test]
fn every_staged_commit_crash_recovers_to_old_or_new_configuration() {
    for stage in [
        SetupCommitStage::Planned,
        SetupCommitStage::CredentialStored,
        SetupCommitStage::ConfigCommitted,
        SetupCommitStage::ReceiptCommitted,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let credentials = RecordingCredentials::default();
        let service = DesktopSetupService::with_dependencies(
            directory.path().to_owned(),
            credentials.clone(),
            FixedClock::at(1_800_000_000),
            Arc::new(FixedIdentities),
            Arc::new(FailAfter(stage)),
        );
        let plan = service.prepare(input("crash")).unwrap();
        assert_eq!(
            service.commit(&plan.plan_digest, "secret").unwrap_err(),
            DesktopSetupError::PersistenceFailed
        );
        let recovery = DesktopSetupService::new(directory.path().to_owned(), credentials);
        recovery.recover(false).unwrap();
        let committed = matches!(
            stage,
            SetupCommitStage::ConfigCommitted | SetupCommitStage::ReceiptCommitted
        );
        assert_eq!(directory.path().join("desktop-v1.json").exists(), committed);
        assert_eq!(
            directory.path().join("desktop-setup-receipt.json").exists(),
            committed
        );
        recovery.recover(committed).unwrap();
        assert!(!directory
            .path()
            .join("desktop-setup-recovery.json")
            .exists());
    }

    let directory = tempfile::tempdir().unwrap();
    let credentials = RecordingCredentials::default();
    let baseline = DesktopSetupService::new(directory.path().to_owned(), credentials.clone());
    let first = baseline.prepare(input("first-stage")).unwrap();
    baseline.commit(&first.plan_digest, "old-secret").unwrap();
    let rotating = DesktopSetupService::with_dependencies(
        directory.path().to_owned(),
        credentials.clone(),
        FixedClock::at(1_800_000_000),
        Arc::new(FixedIdentities),
        Arc::new(FailAfter(SetupCommitStage::CleanupPending)),
    );
    let second = rotating.prepare(input("cleanup-stage")).unwrap();
    assert_eq!(
        rotating
            .commit(&second.plan_digest, "new-secret")
            .unwrap_err(),
        DesktopSetupError::PersistenceFailed
    );
    let recovery = DesktopSetupService::new(directory.path().to_owned(), credentials.clone());
    recovery.recover(false).unwrap();
    recovery.recover(true).unwrap();
    assert!(credentials
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|(_, value)| value == "<deleted>"));
}

#[test]
fn shared_setup_fixture_freezes_catalogue_plans_and_redaction() {
    let fixture: SetupFixture = serde_json::from_str(include_str!(
        "../../../spec/fixtures/desktop/desktop-setup-v1.json"
    ))
    .unwrap();
    assert_eq!(fixture.schema_version, 1);
    let sample = DesktopSetupService::new(
        tempfile::tempdir().unwrap().path().to_owned(),
        RecordingCredentials::default(),
    )
    .catalogue();
    assert_eq!(sample.catalogue_revision, fixture.catalogue_revision);
    assert_eq!(sample.profiles.len(), fixture.expected_profile_count);
    assert_eq!(sample.presets[0].preset_id, fixture.expected_preset_id);
    assert_eq!(
        (
            sample.limits.max_profiles,
            sample.limits.max_text_bytes,
            sample.limits.max_endpoint_bytes,
            sample.limits.max_secret_bytes,
            sample.limits.max_plan_count,
            sample.limits.plan_lifetime_seconds
        ),
        (
            fixture.limits.max_profiles,
            fixture.limits.max_text_bytes,
            fixture.limits.max_endpoint_bytes,
            fixture.limits.max_secret_bytes,
            fixture.limits.max_plan_count,
            fixture.limits.plan_lifetime_seconds
        ),
    );
    assert_eq!(
        fixture.failure_codes,
        [
            "setup_not_allowed",
            "setup_input_invalid",
            "setup_plan_stale",
            "setup_plan_conflict",
            "setup_credential_rejected",
            "setup_persistence_failed",
            "setup_recovery_failed",
        ]
    );

    for case in fixture.plan_cases {
        let directory = tempfile::tempdir().unwrap();
        let service = DesktopSetupService::with_dependencies(
            directory.path().to_owned(),
            RecordingCredentials::default(),
            FixedClock::at(case.now_unix),
            Arc::new(FixtureIdentities {
                setup_id: case.setup_id.clone(),
                credential_ref: case.credential_ref,
            }),
            Arc::new(NoSetupCommitFaults),
        );
        let plan = service.prepare(case.input).unwrap();
        assert_eq!(plan.setup_id, case.setup_id, "{}", case.name);
        assert_eq!(plan.expires_at, case.expected_expires_at, "{}", case.name);
        assert_eq!(
            plan.summary.endpoint_mode, case.expected_endpoint_mode,
            "{}",
            case.name
        );
        assert_eq!(
            (&plan.effective_configuration_digest, &plan.plan_digest),
            (&case.effective_configuration_digest, &case.plan_digest),
            "{}",
            case.name,
        );
        let receipt = service.commit(&plan.plan_digest, "fixture-secret").unwrap();
        let encoded = serde_json::to_string(&receipt).unwrap();
        for forbidden in &fixture.receipt_forbidden_fragments {
            assert!(!encoded.contains(forbidden), "{}", case.name);
        }
    }
}

#[test]
fn tauri_capability_limits_setup_commands_to_the_main_window() {
    let capability: serde_json::Value =
        serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
    assert_eq!(capability["windows"], serde_json::json!(["main"]));
    let permissions = capability["permissions"].as_array().unwrap();
    for permission in [
        "allow-get-setup-state",
        "allow-get-setup-catalogue",
        "allow-prepare-setup",
        "allow-commit-setup",
        "allow-cancel-setup",
    ] {
        assert!(permissions.iter().any(|value| value == permission));
    }
    assert!(capability.get("remote").is_none());
    authorize_setup_window("main").unwrap();
    assert_eq!(
        authorize_setup_window("extension").unwrap_err().code(),
        "setup_not_allowed"
    );
}

#[test]
fn invalid_input_secret_and_replayed_commit_are_stable_and_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let service =
        DesktopSetupService::new(directory.path().to_owned(), RecordingCredentials::default());
    let mut invalid = input("invalid");
    invalid.catalogue_revision = "future".into();
    assert_eq!(
        service.prepare(invalid).unwrap_err().code(),
        "setup_input_invalid"
    );
    let mut invalid_endpoint = input("invalid-endpoint");
    invalid_endpoint.endpoint_override = Some("file:///private/model".into());
    assert_eq!(
        service.prepare(invalid_endpoint).unwrap_err().code(),
        "setup_input_invalid"
    );
    let plan = service.prepare(input("valid")).unwrap();
    assert_eq!(
        service.commit("not-a-digest", "secret").unwrap_err().code(),
        "setup_plan_stale"
    );
    assert_eq!(
        service.commit(&plan.plan_digest, "").unwrap_err().code(),
        "setup_credential_rejected"
    );
    let receipt = service.commit(&plan.plan_digest, "secret").unwrap();
    assert_eq!(
        service.commit(&plan.plan_digest, "secret").unwrap(),
        receipt
    );
}

#[test]
fn prepared_rotation_binds_exact_current_revision_and_digest() {
    let directory = tempfile::tempdir().unwrap();
    let service =
        DesktopSetupService::new(directory.path().to_owned(), RecordingCredentials::default());
    let first = service.prepare(input("binding-first")).unwrap();
    let first_receipt = service.commit(&first.plan_digest, "first-secret").unwrap();
    let second = service.prepare(input("binding-second")).unwrap();
    assert_eq!(second.expected_configuration_revision, Some(1));
    assert_eq!(
        second.expected_configuration_digest.as_deref(),
        Some(first_receipt.configuration_digest.as_str())
    );
    let path = directory.path().join("desktop-v1.json");
    let changed = std::fs::read_to_string(&path)
        .unwrap()
        .replace("desktop-target", "changed-target");
    std::fs::write(path, changed).unwrap();
    assert_eq!(
        service
            .commit(&second.plan_digest, "second-secret")
            .unwrap_err()
            .code(),
        "setup_plan_stale"
    );
}

#[test]
fn replay_rejects_a_corrupt_committed_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let service =
        DesktopSetupService::new(directory.path().to_owned(), RecordingCredentials::default());
    let plan = service.prepare(input("corrupt-receipt")).unwrap();
    service.commit(&plan.plan_digest, "secret").unwrap();
    let path = directory.path().join("desktop-setup-receipt.json");
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    receipt["receipt_digest"] = serde_json::Value::String("c".repeat(64));
    std::fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert_eq!(
        service
            .commit(&plan.plan_digest, "secret")
            .unwrap_err()
            .code(),
        "setup_persistence_failed"
    );
}

#[test]
fn setup_state_reports_startup_without_configuration_values() {
    let directory = tempfile::tempdir().unwrap();
    let service =
        DesktopSetupService::new(directory.path().to_owned(), RecordingCredentials::default());
    assert_eq!(service.state(), DesktopSetupState::SetupRecovering);
    service.complete_startup(false, None).unwrap();
    assert_eq!(service.state(), DesktopSetupState::NotConfigured);
    service
        .complete_startup(false, Some("config_invalid_document"))
        .unwrap();
    assert_eq!(
        service.state(),
        DesktopSetupState::InvalidConfiguration {
            code: "config_invalid_document".to_owned()
        }
    );
    assert!(!serde_json::to_string(&service.state())
        .unwrap()
        .contains(directory.path().to_string_lossy().as_ref()));
}

#[test]
fn sensitive_credential_is_deserialize_only_and_secret_free_in_errors() {
    let credential: SensitiveSetupCredential = serde_json::from_str("\"secret-once\"").unwrap();
    assert_eq!(credential.expose_secret(), "secret-once");
    let invalid = match serde_json::from_str::<SensitiveSetupCredential>("123") {
        Ok(_) => panic!("numeric credential must reject"),
        Err(error) => error,
    };
    assert!(!invalid.to_string().contains("secret-once"));
}

#[test]
fn duplicate_nonce_conflict_expiry_and_cancellation_are_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let clock = FixedClock::at(1_800_000_000);
    let service = DesktopSetupService::with_clock(
        directory.path().to_owned(),
        RecordingCredentials::default(),
        clock.clone(),
    );
    let original = input("same");
    let plan = service.prepare(original.clone()).unwrap();
    assert_eq!(service.prepare(original).unwrap(), plan);
    let mut conflict = input("same");
    conflict.model_id = "different-model".into();
    assert_eq!(
        service.prepare(conflict).unwrap_err().code(),
        "setup_plan_conflict"
    );
    assert_eq!(
        service.cancel(&plan.plan_digest).unwrap(),
        DesktopSetupCancellation::Cancelled
    );
    assert_eq!(
        service
            .commit(&plan.plan_digest, "secret")
            .unwrap_err()
            .code(),
        "setup_plan_stale"
    );

    let expiring = service.prepare(input("expiring")).unwrap();
    assert!(expiring.expires_at.ends_with('Z'));
    clock.set(1_800_001_000);
    assert_eq!(
        service
            .commit(&expiring.plan_digest, "secret")
            .unwrap_err()
            .code(),
        "setup_plan_stale"
    );
}

#[test]
fn recovery_rolls_back_uncommitted_credentials_without_configuration() {
    let directory = tempfile::tempdir().unwrap();
    let credentials = RecordingCredentials::default();
    let journal = serde_json::json!({
        "schema_version": 1,
        "setup_id": "setup-interrupted",
        "plan_digest": "a".repeat(64),
        "configuration_digest": "b".repeat(64),
        "configuration_revision": 1,
        "new_credential_ref": "credential-uncommitted",
        "old_credential_ref": null,
        "stage": "credential_stored"
    });
    std::fs::write(
        directory.path().join("desktop-setup-recovery.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    DesktopSetupService::new(directory.path().to_owned(), credentials.clone())
        .recover(false)
        .unwrap();
    assert_eq!(
        credentials.0.lock().unwrap().as_slice(),
        &[("credential-uncommitted".into(), "<deleted>".into())]
    );
    assert!(!directory
        .path()
        .join("desktop-setup-recovery.json")
        .exists());
}

#[test]
fn recovery_repairs_receipt_then_cleans_obsolete_credential_after_runtime_start() {
    let directory = tempfile::tempdir().unwrap();
    let credentials = RecordingCredentials::default();
    let service = DesktopSetupService::new(directory.path().to_owned(), credentials.clone());
    let first = service.prepare(input("first")).unwrap();
    service.commit(&first.plan_digest, "first-secret").unwrap();
    let old_ref = credentials.0.lock().unwrap()[0].0.clone();
    let second = service.prepare(input("second")).unwrap();
    service
        .commit(&second.plan_digest, "second-secret")
        .unwrap();
    assert!(directory
        .path()
        .join("desktop-setup-recovery.json")
        .exists());
    std::fs::remove_file(directory.path().join("desktop-setup-receipt.json")).unwrap();

    DesktopSetupService::new(directory.path().to_owned(), credentials.clone())
        .recover(true)
        .unwrap();
    assert!(directory.path().join("desktop-setup-receipt.json").exists());
    assert!(!directory
        .path()
        .join("desktop-setup-recovery.json")
        .exists());
    assert!(credentials
        .0
        .lock()
        .unwrap()
        .contains(&(old_ref, "<deleted>".into())));
}
