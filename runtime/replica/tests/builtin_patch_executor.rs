#![cfg(unix)]

use std::os::unix::fs::symlink;

use garive_runtime::{BuiltinPatchExecutor, ExecutorDispatch, ExecutorDispatchError, ExecutorPort};
use garive_tools::{
    BuiltinT1Catalogue, ExecutionFact, GrantId, InvocationGrant, ToolIntent, ToolInvocationId,
    T1_APPLY_PATCH,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[tokio::test]
async fn patch_is_digest_bound_atomic_and_receipted() {
    let directory = tempdir().unwrap();
    let recovery = tempdir().unwrap();
    std::fs::create_dir(directory.path().join("src")).unwrap();
    std::fs::write(directory.path().join("src/lib.rs"), "before\n").unwrap();
    let before_digest = digest(b"before\n");
    let arguments = json!({
        "patch":"*** Begin Patch\n*** Update File: src/lib.rs\n@@\n-before\n+after\n*** End Patch",
        "expected_files":[{"path":"src/lib.rs","before_digest":before_digest}]
    })
    .to_string();

    let (result, mut executor, invocation) =
        dispatch(directory.path(), recovery.path(), &arguments).await;
    let ExecutionFact::Completed { ref content, .. } = result else {
        panic!("expected patch completion")
    };
    assert_eq!(
        std::fs::read(directory.path().join("src/lib.rs")).unwrap(),
        b"after\n"
    );
    assert_eq!(
        content["files"],
        json!([{
            "path":"src/lib.rs",
            "before_digest":digest(b"before\n"),
            "after_digest":digest(b"after\n")
        }])
    );
    assert_eq!(content["receipt_digest"].as_str().unwrap().len(), 64);
    assert_eq!(std::fs::read_dir(recovery.path()).unwrap().count(), 1);
    let receipt = result_receipt(&result);
    let mut mismatched_receipt = receipt.clone();
    mismatched_receipt.result_digest = "b".repeat(64);
    assert_eq!(
        executor.acknowledge_receipt(&invocation, &mismatched_receipt),
        Err(ExecutorDispatchError::ExecutorStateUnknown)
    );
    assert_eq!(std::fs::read_dir(recovery.path()).unwrap().count(), 1);
    executor.acknowledge_receipt(&invocation, receipt).unwrap();
    assert_eq!(std::fs::read_dir(recovery.path()).unwrap().count(), 0);
    assert!(std::fs::read_dir(directory.path())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".garive-patch-")));
}

#[tokio::test]
async fn patch_rejects_changed_content_and_symlink_without_mutation() {
    let directory = tempdir().unwrap();
    std::fs::write(directory.path().join("real"), "before\n").unwrap();
    symlink("real", directory.path().join("link")).unwrap();
    for (path, before_digest, code) in [
        ("real", digest(b"stale\n"), "content_changed"),
        ("link", digest(b"before\n"), "access_denied"),
    ] {
        let arguments = json!({
            "patch":format!("*** Begin Patch\n*** Update File: {path}\n@@\n-before\n+after\n*** End Patch"),
            "expected_files":[{"path":path,"before_digest":before_digest}]
        })
        .to_string();
        let recovery = tempdir().unwrap();
        let ExecutionFact::Failed { code: actual, .. } =
            run(directory.path(), recovery.path(), &arguments).await
        else {
            panic!("expected safe patch failure")
        };
        assert_eq!(actual, code);
        assert_eq!(
            std::fs::read(directory.path().join("real")).unwrap(),
            b"before\n"
        );
    }
}

#[tokio::test]
async fn patch_replays_the_same_journal_instead_of_reapplying_hunks() {
    let directory = tempdir().unwrap();
    let recovery = tempdir().unwrap();
    std::fs::write(directory.path().join("a"), "before-a\n").unwrap();
    std::fs::write(directory.path().join("b"), "before-b\n").unwrap();
    let arguments = json!({
        "patch":"*** Begin Patch\n*** Update File: a\n@@\n-before-a\n+after-a\n*** Update File: b\n@@\n-before-b\n+after-b\n*** End Patch",
        "expected_files":[
            {"path":"a","before_digest":digest(b"before-a\n")},
            {"path":"b","before_digest":digest(b"before-b\n")}
        ]
    })
    .to_string();
    let _ = run(directory.path(), recovery.path(), &arguments).await;

    let journal = std::fs::read_dir(recovery.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let journal_temporary = journal.with_file_name(format!(
        "{}.tmp",
        journal.file_name().unwrap().to_string_lossy()
    ));
    std::fs::rename(&journal, &journal_temporary).unwrap();
    std::fs::write(directory.path().join("b"), "before-b\n").unwrap();
    let key = &digest(b"patch-invocation")[..24];
    std::fs::write(
        directory.path().join(format!(".garive-patch-{key}-1.tmp")),
        "after-b\n",
    )
    .unwrap();
    let result = run(directory.path(), recovery.path(), &arguments).await;

    assert!(matches!(result, ExecutionFact::Completed { .. }));
    assert_eq!(
        std::fs::read(directory.path().join("a")).unwrap(),
        b"after-a\n"
    );
    assert_eq!(
        std::fs::read(directory.path().join("b")).unwrap(),
        b"after-b\n"
    );
}

#[tokio::test]
async fn tampered_recovery_journal_is_uncertain_before_workspace_mutation() {
    let directory = tempdir().unwrap();
    let recovery = tempdir().unwrap();
    std::fs::write(directory.path().join("file"), "before\n").unwrap();
    let arguments = json!({
        "patch":"*** Begin Patch\n*** Update File: file\n@@\n-before\n+after\n*** End Patch",
        "expected_files":[{"path":"file","before_digest":digest(b"before\n")}]
    })
    .to_string();
    let _ = run(directory.path(), recovery.path(), &arguments).await;
    std::fs::write(directory.path().join("file"), "before\n").unwrap();

    let journal_path = std::fs::read_dir(recovery.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
    journal["files"][0]["temporary_name"] = json!("../outside");
    std::fs::write(&journal_path, serde_json::to_vec(&journal).unwrap()).unwrap();

    let (result, _, _) = dispatch_raw(directory.path(), recovery.path(), &arguments).await;
    assert_eq!(result, Err(ExecutorDispatchError::ExecutorStateUnknown));
    assert_eq!(
        std::fs::read(directory.path().join("file")).unwrap(),
        b"before\n"
    );
}

async fn run(root: &std::path::Path, recovery: &std::path::Path, arguments: &str) -> ExecutionFact {
    dispatch(root, recovery, arguments).await.0
}

async fn dispatch(
    root: &std::path::Path,
    recovery: &std::path::Path,
    arguments: &str,
) -> (ExecutionFact, BuiltinPatchExecutor, ToolInvocationId) {
    let (result, executor, invocation) = dispatch_raw(root, recovery, arguments).await;
    (result.unwrap(), executor, invocation)
}

async fn dispatch_raw(
    root: &std::path::Path,
    recovery: &std::path::Path,
    arguments: &str,
) -> (
    Result<ExecutionFact, ExecutorDispatchError>,
    BuiltinPatchExecutor,
    ToolInvocationId,
) {
    let catalogue = BuiltinT1Catalogue::new("snapshot-1", ["rust-toolchain"]).unwrap();
    let prepared = catalogue
        .prepare(&ToolIntent::new("model-call", T1_APPLY_PATCH, arguments))
        .unwrap();
    let invocation = ToolInvocationId::new("patch-invocation").unwrap();
    let grant = InvocationGrant::new(
        GrantId::new("patch-grant").unwrap(),
        invocation.clone(),
        prepared.input_digest(),
        prepared.tool_name(),
        prepared.tool_revision(),
        prepared.requirements().clone(),
        "a".repeat(64),
        "snapshot-1",
    )
    .unwrap();
    let mut executor =
        BuiltinPatchExecutor::new(root, recovery, "unix-patch-v1", catalogue).unwrap();
    let execution = executor.prepare(&invocation, &prepared, &grant).unwrap();
    let result = executor
        .dispatch(ExecutorDispatch {
            invocation_id: &invocation,
            prepared: &prepared,
            grant: &grant,
            execution: &execution,
            receipt_id: "patch-receipt",
        })
        .await;
    (result, executor, invocation)
}

fn result_receipt(result: &ExecutionFact) -> &garive_tools::EffectReceipt {
    match result {
        ExecutionFact::Completed {
            receipt: Some(receipt),
            ..
        }
        | ExecutionFact::Failed {
            receipt: Some(receipt),
            ..
        } => receipt,
        _ => panic!("expected receipt"),
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
