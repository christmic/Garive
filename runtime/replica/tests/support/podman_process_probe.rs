use std::{collections::BTreeMap, path::PathBuf};

use garive_runtime::{
    PodmanProcessBackend, PodmanProcessConfig, ProcessExecutionRequest, ProcessExit,
    ProcessIsolationBackend, ProcessWorkspaceMode,
};
use garive_tools::ToolInvocationId;

fn main() {
    let [_, mode, podman, socket, image, workspace, recovery] = std::env::args()
        .collect::<Vec<_>>()
        .try_into()
        .expect("usage: probe MODE PODMAN SOCKET IMAGE WORKSPACE RECOVERY");
    let backend = PodmanProcessBackend::new(
        PodmanProcessConfig::new(podman, socket, image, workspace, recovery, 5_000).unwrap(),
    );
    if mode == "recover-long" {
        recover_twice(&backend, "crash");
        return;
    }
    if mode == "start-long" {
        execute(
            &backend,
            "crash",
            "/bin/sleep",
            ["sleep", "30"],
            ProcessWorkspaceMode::Read,
            4096,
            60_000,
            BTreeMap::new(),
        );
        return;
    }
    assert_eq!(mode, "matrix");

    let read = execute(
        &backend,
        "read",
        "/bin/cat",
        ["cat", "/workspace/input"],
        ProcessWorkspaceMode::Read,
        4096,
        5000,
        BTreeMap::new(),
    );
    assert_eq!(read.exit, ProcessExit::Code(0));
    assert_eq!(read.stdout, b"fixture\n");
    acknowledge(&backend, "read");

    let denied_write = execute(
        &backend,
        "denied-write",
        "/bin/touch",
        ["touch", "/workspace/denied"],
        ProcessWorkspaceMode::Read,
        4096,
        5000,
        BTreeMap::new(),
    );
    assert_ne!(denied_write.exit, ProcessExit::Code(0));
    acknowledge(&backend, "denied-write");

    let write = execute(
        &backend,
        "write",
        "/bin/touch",
        ["touch", "/workspace/created"],
        ProcessWorkspaceMode::Write,
        4096,
        5000,
        BTreeMap::new(),
    );
    assert_eq!(write.exit, ProcessExit::Code(0));
    acknowledge(&backend, "write");

    let environment = execute(
        &backend,
        "environment",
        "/usr/bin/env",
        ["env"],
        ProcessWorkspaceMode::Read,
        4096,
        5000,
        BTreeMap::from([("GARIVE_PROBE".into(), "exact".into())]),
    );
    assert_eq!(environment.exit, ProcessExit::Code(0));
    let output = String::from_utf8(environment.stdout).unwrap();
    let lines = output.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3);
    assert!(lines.contains(&"GARIVE_PROBE=exact"));
    assert!(lines.contains(&"HOME=/tmp"));
    assert!(lines
        .iter()
        .any(|line| line.starts_with("HOSTNAME=garive-process-")));
    acknowledge(&backend, "environment");

    let reserved_environment = ProcessExecutionRequest {
        invocation_id: invocation("reserved-environment"),
        dispatch_attempt_id: "attempt-reserved-environment".into(),
        lane: "probe".into(),
        executable: PathBuf::from("/usr/bin/env"),
        argv: vec!["env".into()],
        working_directory: ".".into(),
        workspace_mode: ProcessWorkspaceMode::Read,
        environment: BTreeMap::from([("HOME".into(), "/root".into())]),
        max_output_bytes: 4096,
        timeout_ms: 5000,
        max_processes: 4,
        max_open_files: 32,
    };
    assert!(backend.preflight(&reserved_environment).is_err());

    let network = execute(
        &backend,
        "network",
        "/bin/busybox",
        [
            "busybox",
            "wget",
            "-T",
            "2",
            "-O",
            "/tmp/network-output",
            "http://example.com",
        ],
        ProcessWorkspaceMode::Read,
        4096,
        2000,
        BTreeMap::new(),
    );
    assert_ne!(network.exit, ProcessExit::Code(0));
    acknowledge(&backend, "network");

    let truncated = execute(
        &backend,
        "truncated",
        "/usr/bin/head",
        ["head", "-c", "10000", "/dev/zero"],
        ProcessWorkspaceMode::Read,
        257,
        5000,
        BTreeMap::new(),
    );
    assert!(truncated.truncated);
    assert_eq!(truncated.stdout.len() + truncated.stderr.len(), 257);
    acknowledge(&backend, "truncated");

    let timeout = execute(
        &backend,
        "timeout",
        "/bin/sleep",
        ["sleep", "30"],
        ProcessWorkspaceMode::Read,
        4096,
        100,
        BTreeMap::new(),
    );
    assert_eq!(timeout.exit, ProcessExit::Timeout);
    acknowledge(&backend, "timeout");
    recover_twice(&backend, "timeout");

    let process_tree = execute(
        &backend,
        "process-tree",
        "/bin/sh",
        ["sh", "-c", "/bin/sleep 30 & wait"],
        ProcessWorkspaceMode::Read,
        4096,
        100,
        BTreeMap::new(),
    );
    assert_eq!(process_tree.exit, ProcessExit::Timeout);
    assert!(process_tree.process_tree_terminated);
    acknowledge(&backend, "process-tree");
    recover_twice(&backend, "process-tree");

    println!("podman process probe passed");
}

#[allow(clippy::too_many_arguments)]
fn execute<const N: usize>(
    backend: &PodmanProcessBackend,
    identity: &str,
    executable: &str,
    argv: [&str; N],
    workspace_mode: ProcessWorkspaceMode,
    output: u64,
    timeout: u64,
    environment: BTreeMap<String, String>,
) -> garive_runtime::ProcessExecutionResult {
    let request = ProcessExecutionRequest {
        invocation_id: invocation(identity),
        dispatch_attempt_id: format!("attempt-{identity}"),
        lane: "probe".into(),
        executable: PathBuf::from(executable),
        argv: argv.into_iter().map(str::to_owned).collect(),
        working_directory: ".".into(),
        workspace_mode,
        environment,
        max_output_bytes: output,
        timeout_ms: timeout,
        max_processes: 4,
        max_open_files: 32,
    };
    backend.preflight(&request).unwrap();
    backend.execute(request).unwrap()
}

fn acknowledge(backend: &PodmanProcessBackend, identity: &str) {
    backend
        .acknowledge_terminal(&invocation(identity), &format!("attempt-{identity}"))
        .unwrap();
}

fn recover_twice(backend: &PodmanProcessBackend, identity: &str) {
    for _ in 0..2 {
        backend
            .terminate_or_prove_absent(&invocation(identity), &format!("attempt-{identity}"))
            .unwrap();
    }
}

fn invocation(identity: &str) -> ToolInvocationId {
    ToolInvocationId::new(format!("probe-{identity}")).unwrap()
}
