use garive_tools::{
    ExecutionCapability, PreparationErrorCode, SandboxControl, SandboxRequirementsV1,
};
use serde_json::Value;

#[test]
fn filesystem_profile_requires_scope_symlink_and_limits() {
    let error = SandboxRequirementsV1::new(
        [ExecutionCapability::FilesystemRead],
        [
            SandboxControl::FilesystemScope,
            SandboxControl::ResourceLimits,
        ],
        None,
        8,
    )
    .unwrap_err();
    assert_eq!(
        error.code(),
        PreparationErrorCode::SandboxRequirementInvalid
    );
}

#[test]
fn process_profile_requires_a_process_ceiling() {
    let error = SandboxRequirementsV1::new(
        [ExecutionCapability::Process],
        [
            SandboxControl::ProcessContainment,
            SandboxControl::StructuredArguments,
            SandboxControl::EnvironmentAllowlist,
            SandboxControl::ResourceLimits,
        ],
        None,
        8,
    )
    .unwrap_err();
    assert_eq!(
        error.code(),
        PreparationErrorCode::SandboxRequirementInvalid
    );
}

#[test]
fn stricter_executor_profile_covers_requested_profile() {
    let requested = filesystem_profile(16);
    let executor = filesystem_profile(8);
    assert!(requested.is_covered_by(&executor));
    assert!(!executor.is_covered_by(&requested));
}

#[test]
fn digest_is_canonical_and_limit_sensitive() {
    let profile = filesystem_profile(8);
    assert_eq!(
        profile.digest().unwrap(),
        "ee3658a7b9788d184f0f97b9b611826416cf546b0786a775f9ba339c18d9e611"
    );
    assert_ne!(
        profile.digest().unwrap(),
        filesystem_profile(9).digest().unwrap()
    );
}

#[test]
fn shared_fixture_has_cross_language_profiles_and_failures() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/sandbox-safety-v1.json"
    ))
    .unwrap();
    assert_eq!(fixture["schema_version"], 1);
    for profile in fixture["profiles"].as_array().unwrap() {
        let value = profile_from_json(profile).unwrap();
        assert_eq!(
            value
                .controls()
                .map(SandboxControl::wire_name)
                .collect::<Vec<_>>(),
            profile["canonical_controls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(value.digest().unwrap(), profile["digest"]);
    }
    for invalid in fixture["invalid_profiles"].as_array().unwrap() {
        assert_eq!(
            profile_from_json(invalid).unwrap_err().code(),
            PreparationErrorCode::SandboxRequirementInvalid
        );
    }
}

fn filesystem_profile(max_open_files: u32) -> SandboxRequirementsV1 {
    SandboxRequirementsV1::new(
        [ExecutionCapability::FilesystemRead],
        [
            SandboxControl::ResourceLimits,
            SandboxControl::SymlinkContainment,
            SandboxControl::FilesystemScope,
        ],
        None,
        max_open_files,
    )
    .unwrap()
}

fn profile_from_json(
    value: &Value,
) -> Result<SandboxRequirementsV1, garive_tools::PreparationError> {
    SandboxRequirementsV1::new(
        value["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| match item.as_str().unwrap() {
                "filesystem_read" => ExecutionCapability::FilesystemRead,
                "process" => ExecutionCapability::Process,
                "browser_observe" => ExecutionCapability::BrowserObserve,
                "computer_act" => ExecutionCapability::ComputerAct,
                _ => panic!("unknown fixture capability"),
            }),
        value["controls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| match item.as_str().unwrap() {
                "filesystem_scope" => SandboxControl::FilesystemScope,
                "symlink_containment" => SandboxControl::SymlinkContainment,
                "process_containment" => SandboxControl::ProcessContainment,
                "structured_arguments" => SandboxControl::StructuredArguments,
                "environment_allowlist" => SandboxControl::EnvironmentAllowlist,
                "resource_limits" => SandboxControl::ResourceLimits,
                "browser_session_scope" => SandboxControl::BrowserSessionScope,
                "native_target_scope" => SandboxControl::NativeTargetScope,
                "snapshot_binding" => SandboxControl::SnapshotBinding,
                "focus_revalidation" => SandboxControl::FocusRevalidation,
                "screen_capture_scope" => SandboxControl::ScreenCaptureScope,
                _ => panic!("unknown fixture control"),
            }),
        value["max_processes"].as_u64().map(|number| number as u32),
        value["max_open_files"].as_u64().unwrap() as u32,
    )
}
