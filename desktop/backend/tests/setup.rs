use std::sync::{Arc, Mutex};

use garive_desktop::{
    DesktopSetupError, DesktopSetupInput, DesktopSetupService, DesktopSystemConfiguration,
    SetupCredentialStore, OPENAI_RESPONSES_PROFILE_ID,
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
fn invalid_input_secret_and_replayed_plan_fail_with_stable_codes() {
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
    service.commit(&plan.plan_digest, "secret").unwrap();
    assert_eq!(
        service
            .commit(&plan.plan_digest, "secret")
            .unwrap_err()
            .code(),
        "setup_plan_stale"
    );
}
