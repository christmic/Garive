#![cfg(unix)]

use std::{collections::BTreeSet, fs, os::unix::fs::PermissionsExt};

use garive_runtime::{
    PodmanProcessConfig, ProcessBackendHostConfig, ProcessExecutable, ProcessLane,
    ProcessLaneRegistry, T1HostSystemConfig, T1RuntimeSystemConfig, T1WorkspaceRuntimeConfig,
    T1_PROCESS_EXECUTOR_ID, T1_WORKSPACE_EXECUTOR_ID,
};
use garive_tools::{ToolIntent, T1_PROCESS_RUN, T1_READ_TEXT, T1_WRITE_TEXT};
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
    assert_eq!(execution.capabilities().definitions.len(), 6);
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
            "garive.workspace.write_text",
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
        &patch_recovery,
        lanes(),
        ProcessBackendHostConfig::podman(
            "/opt/garive/bin/podman",
            "unix:///var/run/garive-podman.sock",
            format!("localhost/garive-runner@sha256:{}", "a".repeat(64)),
            &process_recovery,
            5_000,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(host.policy_revision(), "policy.v1");
    assert_eq!(host.executor_revision(), "executor.v1");
    assert_eq!(host.process_lane_names().collect::<Vec<_>>(), ["rust"]);
    assert_eq!(host.tool_capabilities().unwrap().definitions.len(), 6);
    assert_eq!(
        host.bind_workspace(&workspace)
            .unwrap()
            .build()
            .unwrap()
            .capabilities()
            .definitions
            .len(),
        6
    );
    assert!(ProcessBackendHostConfig::podman(
        "/opt/garive/bin/podman",
        "unix:///var/run/garive-podman.sock",
        "localhost/garive-runner:latest",
        &process_recovery,
        5_000,
    )
    .is_err());
}

#[test]
fn workspace_only_config_excludes_process_and_keeps_write() {
    let directory = tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let recovery = directory.path().join("patch-recovery");
    private_directory(&workspace, 0o755);
    private_directory(&recovery, 0o700);
    let execution =
        T1WorkspaceRuntimeConfig::new("policy.v1", "executor.v1", &workspace, &recovery)
            .unwrap()
            .build()
            .unwrap();
    assert_eq!(execution.capabilities().definitions.len(), 5);
    assert!(execution.executor_binding(T1_PROCESS_RUN).is_none());
    assert!(execution.executor_binding(T1_WRITE_TEXT).is_some());
}

#[test]
fn host_process_backend_is_one_closed_value_bound_only_with_a_workspace() {
    let directory = tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let recovery = directory.path().join("process-recovery");
    private_directory(&workspace, 0o755);
    private_directory(&recovery, 0o700);

    let backend = ProcessBackendHostConfig::podman(
        "/opt/garive/bin/podman",
        "unix:///var/run/garive-podman.sock",
        format!("localhost/garive-runner@sha256:{}", "a".repeat(64)),
        &recovery,
        5_000,
    )
    .unwrap();
    assert_eq!(backend.kind(), "podman");
    assert_eq!(backend.bind_workspace(&workspace).unwrap().kind(), "podman");
}

#[test]
fn runtime_owns_backend_bound_executor_revisions() {
    let directory = tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let patch_recovery = directory.path().join("patch-recovery");
    let process_recovery = directory.path().join("process-recovery");
    private_directory(&workspace, 0o755);
    private_directory(&patch_recovery, 0o700);
    private_directory(&process_recovery, 0o700);

    let first = T1RuntimeSystemConfig::new(
        "policy.v1",
        "executor.v1",
        &workspace,
        &patch_recovery,
        lanes(),
        podman_with_digest(&workspace, &process_recovery, 'a'),
    )
    .unwrap()
    .build()
    .unwrap();
    let workspace_binding = first.executor_binding(T1_READ_TEXT).unwrap();
    assert_eq!(workspace_binding.executor_id(), T1_WORKSPACE_EXECUTOR_ID);
    assert_eq!(workspace_binding.executor_revision(), "executor.v1");
    let first_process = first.executor_binding(T1_PROCESS_RUN).unwrap();
    assert_eq!(first_process.executor_id(), T1_PROCESS_EXECUTOR_ID);
    assert!(first_process
        .executor_revision()
        .starts_with("executor.v1+podman-sha256:"));

    let second = T1RuntimeSystemConfig::new(
        "policy.v1",
        "executor.v1",
        &workspace,
        &patch_recovery,
        lanes(),
        podman_with_digest(&workspace, &process_recovery, 'b'),
    )
    .unwrap()
    .build()
    .unwrap();
    assert_ne!(
        first_process.executor_revision(),
        second
            .executor_binding(T1_PROCESS_RUN)
            .unwrap()
            .executor_revision()
    );
    assert!(first.executor_binding("unknown").is_none());
}

#[test]
fn every_podman_identity_field_changes_process_revision() {
    let directory = tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let other_workspace = directory.path().join("other-workspace");
    let patch_recovery = directory.path().join("patch-recovery");
    let process_recovery = directory.path().join("process-recovery");
    let other_recovery = directory.path().join("other-process-recovery");
    for (path, mode) in [
        (&workspace, 0o755),
        (&other_workspace, 0o755),
        (&patch_recovery, 0o700),
        (&process_recovery, 0o700),
        (&other_recovery, 0o700),
    ] {
        private_directory(path, mode);
    }
    let baseline = process_revision(
        &workspace,
        &patch_recovery,
        podman_config(
            "/opt/garive/bin/podman",
            "unix:///var/run/garive-podman.sock",
            &workspace,
            &process_recovery,
            5_000,
            'a',
        ),
    );
    let variants = [
        podman_config(
            "/other/podman",
            "unix:///var/run/garive-podman.sock",
            &workspace,
            &process_recovery,
            5_000,
            'a',
        ),
        podman_config(
            "/opt/garive/bin/podman",
            "unix:///var/run/other.sock",
            &workspace,
            &process_recovery,
            5_000,
            'a',
        ),
        podman_config(
            "/opt/garive/bin/podman",
            "unix:///var/run/garive-podman.sock",
            &workspace,
            &process_recovery,
            5_000,
            'b',
        ),
        podman_config(
            "/opt/garive/bin/podman",
            "unix:///var/run/garive-podman.sock",
            &workspace,
            &other_recovery,
            5_000,
            'a',
        ),
        podman_config(
            "/opt/garive/bin/podman",
            "unix:///var/run/garive-podman.sock",
            &workspace,
            &process_recovery,
            5_001,
            'a',
        ),
    ];
    for variant in variants {
        assert_ne!(
            baseline,
            process_revision(&workspace, &patch_recovery, variant)
        );
    }
    assert_ne!(
        baseline,
        process_revision(
            &other_workspace,
            &patch_recovery,
            podman_config(
                "/opt/garive/bin/podman",
                "unix:///var/run/garive-podman.sock",
                &other_workspace,
                &process_recovery,
                5_000,
                'a',
            )
        )
    );
}

fn process_revision(
    workspace: &std::path::Path,
    patch_recovery: &std::path::Path,
    podman: PodmanProcessConfig,
) -> String {
    T1RuntimeSystemConfig::new(
        "policy.v1",
        "executor.v1",
        workspace,
        patch_recovery,
        lanes(),
        podman,
    )
    .unwrap()
    .build()
    .unwrap()
    .executor_binding(T1_PROCESS_RUN)
    .unwrap()
    .executor_revision()
    .to_owned()
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
    podman_with_digest(workspace, process_recovery, 'a')
}

fn podman_with_digest(
    workspace: &std::path::Path,
    process_recovery: &std::path::Path,
    digest: char,
) -> PodmanProcessConfig {
    podman_config(
        "/opt/garive/bin/podman",
        "unix:///var/run/garive-podman.sock",
        workspace,
        process_recovery,
        5_000,
        digest,
    )
}

fn podman_config(
    executable: &str,
    socket: &str,
    workspace: &std::path::Path,
    process_recovery: &std::path::Path,
    timeout_ms: u64,
    digest: char,
) -> PodmanProcessConfig {
    PodmanProcessConfig::new(
        executable,
        socket,
        format!(
            "localhost/garive-runner@sha256:{}",
            digest.to_string().repeat(64)
        ),
        workspace,
        process_recovery,
        timeout_ms,
    )
    .unwrap()
}

fn private_directory(path: &std::path::Path, mode: u32) {
    fs::create_dir(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
