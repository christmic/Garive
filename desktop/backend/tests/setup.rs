use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc, Mutex,
};

use garive_desktop::{
    DesktopSetupCancellation, DesktopSetupError, DesktopSetupInput, DesktopSetupService,
    DesktopSystemConfiguration, SetupClock, SetupCredentialStore, OPENAI_RESPONSES_PROFILE_ID,
};

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

fn input(revision: &str) -> DesktopSetupInput {
    DesktopSetupInput {
        schema_version: 1,
        caller_nonce: format!("nonce-{revision}"),
        catalogue_revision: "desktop-setup-catalogue-1".into(),
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

    let plan = service.prepare(input("1")).unwrap();
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
    let plan = service.prepare(input("valid")).unwrap();
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
