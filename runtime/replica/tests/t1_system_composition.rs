#![cfg(unix)]

use std::{collections::BTreeSet, fs, os::unix::fs::PermissionsExt};

use garive_runtime::{
    PodmanProcessConfig, ProcessExecutable, ProcessLane, ProcessLaneRegistry, T1HostSystemConfig,
    T1RuntimeSystemConfig,
};
use garive_tools::{ToolIntent, T1_READ_TEXT};
use tempfile::tempdir;

#[test]
fn one_explicit_system_config_builds_the_exact_five_tool_execution_surface() {
    let directory = tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let patch_recovery = directory.path().join("patch-recovery");
    let process_recovery = directory.path().join("process-recovery");
    private_directory(&workspace, 0o755);
    private_directory(&patch_recovery, 0o700);
    private_directory(&process_recovery, 0o700);
    let podman = podman(&workspace, &process_recovery);
    let config = T1RuntimeSystemConfig::new(
        "t1.policy.v1",
        "t1.executor.v1",
        &workspace,
        &patch_recovery,
        lanes(),
        podman,
    )
    .unwrap();
    let execution = config.build().unwrap();
    assert_eq!(execution.capabilities().definitions.len(), 5);
    assert_eq!(
        execution
            .capabilities()
            .definitions
            .iter()
            .map(|definition| definition.name())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "garive.process.run",
            "garive.workspace.apply_patch",
            "garive.workspace.list",
            "garive.workspace.read_text",
            "garive.workspace.search_text",
        ])
    );
    let (_, preparation, _) = execution.into_parts();
    let prepared = preparation
        .prepare(&ToolIntent::new(
            "call-1",
            T1_READ_TEXT,
            r#"{"path":"README.md","max_bytes":4096}"#,
        ))
        .unwrap();
    assert_eq!(prepared.tool_name(), T1_READ_TEXT);
}

#[test]
fn mismatched_workspace_and_non_private_recovery_fail_before_executor_creation() {
    let directory = tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let other_workspace = directory.path().join("other-workspace");
    let patch_recovery = directory.path().join("patch-recovery");
    let process_recovery = directory.path().join("process-recovery");
    private_directory(&workspace, 0o755);
    private_directory(&other_workspace, 0o755);
    private_directory(&patch_recovery, 0o700);
    private_directory(&process_recovery, 0o700);
    assert!(T1RuntimeSystemConfig::new(
        "policy",
        "executor",
        &other_workspace,
        &patch_recovery,
        lanes(),
        podman(&workspace, &process_recovery),
    )
    .is_err());

    fs::set_permissions(&patch_recovery, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(T1RuntimeSystemConfig::new(
        "policy",
        "executor",
        &workspace,
        &patch_recovery,
        lanes(),
        podman(&workspace, &process_recovery),
    )
    .is_err());
}

#[test]
fn persistent_host_values_bind_an_explicit_workspace_without_discovery() {
    let directory = tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let patch_recovery = directory.path().join("patch-recovery");
    let process_recovery = directory.path().join("process-recovery");
    private_directory(&workspace, 0o755);
    private_directory(&patch_recovery, 0o700);
    private_directory(&process_recovery, 0o700);
    let host = T1HostSystemConfig::new(
        "policy.v1",
        "executor.v1",
        "/opt/garive/bin/podman",
        "unix:///var/run/garive-podman.sock",
        format!("localhost/garive-runner@sha256:{}", "a".repeat(64)),
        &patch_recovery,
        &process_recovery,
        5_000,
        lanes(),
    )
    .unwrap();
    assert_eq!(host.policy_revision(), "policy.v1");
    assert_eq!(host.executor_revision(), "executor.v1");
    assert_eq!(host.process_lane_names().collect::<Vec<_>>(), ["rust"]);
    assert_eq!(host.tool_capabilities().unwrap().definitions.len(), 5);
    assert_eq!(
        host.bind_workspace(&workspace)
            .unwrap()
            .build()
            .unwrap()
            .capabilities()
            .definitions
            .len(),
        5
    );
    assert!(T1HostSystemConfig::new(
        "policy.v1",
        "executor.v1",
        "/opt/garive/bin/podman",
        "unix:///var/run/garive-podman.sock",
        "localhost/garive-runner:latest",
        &patch_recovery,
        &process_recovery,
        5_000,
        lanes(),
    )
    .is_err());
}

fn lanes() -> ProcessLaneRegistry {
    ProcessLaneRegistry::new([ProcessLane::new(
        "rust",
        [ProcessExecutable::new("cargo", "/opt/garive/bin/cargo").unwrap()],
        [("LANG".into(), "C.UTF-8".into())],
    )
    .unwrap()])
    .unwrap()
}

fn podman(workspace: &std::path::Path, process_recovery: &std::path::Path) -> PodmanProcessConfig {
    PodmanProcessConfig::new(
        "/opt/garive/bin/podman",
        "unix:///var/run/garive-podman.sock",
        format!("localhost/garive-runner@sha256:{}", "a".repeat(64)),
        workspace,
        process_recovery,
        5_000,
    )
    .unwrap()
}

fn private_directory(path: &std::path::Path, mode: u32) {
    fs::create_dir(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
