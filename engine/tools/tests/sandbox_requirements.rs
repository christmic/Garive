use garive_tools::{
    ExecutionCapability, PreparationErrorCode, SandboxControl, SandboxRequirementsV1,
};

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
