#![cfg(unix)]

use std::os::unix::fs::symlink;

use garive_runtime::{BuiltinWorkspaceExecutor, ExecutorDispatch, ExecutorPort};
use garive_tools::{
    BuiltinT1Catalogue, ExecutionFact, GrantId, InvocationGrant, TerminalClassification,
    ToolIntent, ToolInvocationId, T1_APPLY_PATCH, T1_LIST, T1_READ_TEXT, T1_SEARCH_TEXT,
    T1_WRITE_TEXT,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[tokio::test]
async fn read_text_returns_the_exact_bounded_envelope_and_receipt() {
    let directory = tempdir().unwrap();
    std::fs::create_dir(directory.path().join("src")).unwrap();
    std::fs::write(directory.path().join("src/lib.rs"), "hello").unwrap();
    let prepared = prepare(T1_READ_TEXT, r#"{"path":"src/lib.rs","max_bytes":5}"#);
    let invocation = ToolInvocationId::new("read-call").unwrap();
    let grant = grant(&invocation, &prepared);
    let mut executor =
        BuiltinWorkspaceExecutor::new(directory.path(), "unix-v1", catalogue()).unwrap();
    let execution = executor.prepare(&invocation, &prepared, &grant).unwrap();
    let result = executor
        .dispatch(ExecutorDispatch {
            invocation_id: &invocation,
            prepared: &prepared,
            grant: &grant,
            execution: &execution,
            receipt_id: "receipt-read",
        })
        .await
        .unwrap();
    let ExecutionFact::Completed {
        receipt: Some(receipt),
        content,
        truncated,
    } = result
    else {
        panic!("expected completed read")
    };
    assert_eq!(
        content,
        json!({
            "path":"src/lib.rs",
            "text":"hello",
            "byte_count":5,
            "content_digest":format!("{:x}", Sha256::digest(b"hello")),
            "truncated":false
        })
    );
    assert!(!truncated);
    assert_eq!(
        receipt.terminal_classification,
        TerminalClassification::Completed
    );
    assert_eq!(receipt.executor_id, "garive.builtin.workspace");
}

#[tokio::test]
async fn read_text_maps_bounds_encoding_types_and_links_to_safe_failures() {
    let directory = tempdir().unwrap();
    std::fs::write(directory.path().join("large.txt"), "hello").unwrap();
    std::fs::write(directory.path().join("binary"), [0xff]).unwrap();
    std::fs::write(directory.path().join("Résumé.txt"), "ok").unwrap();
    std::fs::create_dir(directory.path().join("folder")).unwrap();
    symlink("large.txt", directory.path().join("link")).unwrap();
    for (path, max_bytes, code) in [
        ("large.txt", 4, "result_bound_exceeded"),
        ("binary", 4, "non_utf8_content"),
        ("folder", 4, "path_type_mismatch"),
        ("link", 4, "access_denied"),
        ("missing", 4, "path_not_found"),
        ("résumé.txt", 4, "path_not_found"),
    ] {
        assert_eq!(
            run_failure(
                directory.path(),
                T1_READ_TEXT,
                &format!(r#"{{"path":"{path}","max_bytes":{max_bytes}}}"#)
            )
            .await,
            code
        );
    }
    assert_eq!(
        run_completed(
            directory.path(),
            T1_READ_TEXT,
            r#"{"path":"Résumé.txt","max_bytes":2}"#,
        )
        .await["text"],
        "ok"
    );
}

#[tokio::test]
async fn write_text_creates_once_without_overwriting_or_following_links() {
    let directory = tempdir().unwrap();
    std::fs::create_dir(directory.path().join("notes")).unwrap();
    let arguments = r#"{"path":"notes/result.txt","text":"first"}"#;
    let result = run_completed(directory.path(), T1_WRITE_TEXT, arguments).await;
    assert_eq!(result["path"], "notes/result.txt");
    assert_eq!(result["byte_count"], 5);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("notes/result.txt")).unwrap(),
        "first"
    );
    assert_eq!(
        run_failure(directory.path(), T1_WRITE_TEXT, arguments).await,
        "path_exists"
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("notes/result.txt")).unwrap(),
        "first"
    );

    symlink("notes", directory.path().join("linked-notes")).unwrap();
    assert_eq!(
        run_failure(
            directory.path(),
            T1_WRITE_TEXT,
            r#"{"path":"linked-notes/other.txt","text":"blocked"}"#,
        )
        .await,
        "access_denied"
    );
}

#[tokio::test]
async fn list_is_raw_name_sorted_bounded_and_never_follows_links() {
    let directory = tempdir().unwrap();
    for name in ["b", "a", ".hidden"] {
        std::fs::write(directory.path().join(name), name).unwrap();
    }
    std::fs::create_dir(directory.path().join("dir")).unwrap();
    symlink("a", directory.path().join("link")).unwrap();
    let result = run_completed(
        directory.path(),
        T1_LIST,
        r#"{"path":".","max_entries":2,"include_hidden":false,"max_nodes":100}"#,
    )
    .await;
    assert_eq!(
        result,
        json!({
            "path":".",
            "entries":[{"name":"a","kind":"file"},{"name":"b","kind":"file"}],
            "truncated":true
        })
    );
    let all = run_completed(
        directory.path(),
        T1_LIST,
        r#"{"path":".","max_entries":10,"include_hidden":true,"max_nodes":100}"#,
    )
    .await;
    assert_eq!(
        all["entries"],
        json!([
            {"name":".hidden","kind":"file"},
            {"name":"a","kind":"file"},
            {"name":"b","kind":"file"},
            {"name":"dir","kind":"directory"},
            {"name":"link","kind":"symlink"}
        ])
    );
    assert_eq!(
        run_failure(
            directory.path(),
            T1_LIST,
            r#"{"path":".","max_entries":10,"include_hidden":false,"max_nodes":1}"#,
        )
        .await,
        "entry_bound_exceeded"
    );
}

#[test]
fn executor_rejects_unimplemented_or_mixed_prepared_bindings_before_started() {
    let directory = tempdir().unwrap();
    let prepared = prepare(
        T1_APPLY_PATCH,
        &format!(
            r#"{{"patch":"*** Begin Patch\n*** Update File: file\n@@\n-a\n+b\n*** End Patch","expected_files":[{{"path":"file","before_digest":"{}"}}]}}"#,
            "a".repeat(64)
        ),
    );
    let invocation = ToolInvocationId::new("search-call").unwrap();
    let search_grant = grant(&invocation, &prepared);
    let mut executor =
        BuiltinWorkspaceExecutor::new(directory.path(), "unix-v1", catalogue()).unwrap();
    assert!(executor
        .prepare(&invocation, &prepared, &search_grant)
        .is_err());

    let read = prepare(T1_READ_TEXT, r#"{"path":"file","max_bytes":1}"#);
    let read_invocation = ToolInvocationId::new("mixed-call").unwrap();
    let read_grant = grant(&read_invocation, &read);
    let mismatched = BuiltinT1Catalogue::new("snapshot-2", ["rust-toolchain"]).unwrap();
    let mut executor =
        BuiltinWorkspaceExecutor::new(directory.path(), "unix-v1", mismatched).unwrap();
    assert!(executor
        .prepare(&read_invocation, &read, &read_grant)
        .is_err());
}

#[tokio::test]
async fn search_walks_in_raw_path_order_with_exact_columns_and_skip_counts() {
    let directory = tempdir().unwrap();
    std::fs::write(directory.path().join("a.txt"), "Needle alpha\né needle z").unwrap();
    std::fs::write(directory.path().join("b.txt"), "needle").unwrap();
    std::fs::write(directory.path().join("binary"), [0xff]).unwrap();
    std::fs::write(directory.path().join("large"), "x".repeat(100)).unwrap();
    std::fs::create_dir(directory.path().join("nested")).unwrap();
    std::fs::write(directory.path().join("nested/c.txt"), "needle").unwrap();
    symlink("a.txt", directory.path().join("link")).unwrap();

    let result = run_completed(
        directory.path(),
        T1_SEARCH_TEXT,
        r#"{"path":".","query":"needle","case_sensitive":false,"max_matches":2,"max_file_bytes":64,"max_nodes":20}"#,
    )
    .await;
    assert_eq!(
        result,
        json!({
            "matches":[
                {"path":"a.txt","line":1,"column":1,"preview":"Needle alpha"},
                {"path":"a.txt","line":2,"column":3,"preview":"é needle z"}
            ],
            "files_scanned":3,
            "skipped":{"access_denied":0,"non_utf8_content":1,"result_bound_exceeded":1},
            "truncated":true
        })
    );
}

#[tokio::test]
async fn search_preview_and_node_bound_are_deterministic() {
    let directory = tempdir().unwrap();
    let line = format!("{}needle{}", "x".repeat(120), "y".repeat(200));
    std::fs::write(directory.path().join("long.txt"), line).unwrap();
    let result = run_completed(
        directory.path(),
        T1_SEARCH_TEXT,
        r#"{"path":".","query":"needle","case_sensitive":true,"max_matches":1,"max_file_bytes":4096,"max_nodes":1}"#,
    )
    .await;
    assert_eq!(result["matches"][0]["column"], 121);
    let preview = result["matches"][0]["preview"].as_str().unwrap();
    assert!(preview.starts_with('…') && preview.ends_with('…'));
    assert!(preview.contains("needle"));

    std::fs::write(directory.path().join("second.txt"), "needle").unwrap();
    assert_eq!(
        run_failure(
            directory.path(),
            T1_SEARCH_TEXT,
            r#"{"path":".","query":"needle","case_sensitive":true,"max_matches":1,"max_file_bytes":4096,"max_nodes":1}"#,
        )
        .await,
        "search_bound_exceeded"
    );
}

fn prepare(name: &str, arguments: &str) -> garive_tools::PreparedToolCall {
    catalogue()
        .prepare(&ToolIntent::new("model-call", name, arguments))
        .unwrap()
}

fn catalogue() -> BuiltinT1Catalogue {
    BuiltinT1Catalogue::new("snapshot-1", ["rust-toolchain"]).unwrap()
}

fn grant(
    invocation: &ToolInvocationId,
    prepared: &garive_tools::PreparedToolCall,
) -> InvocationGrant {
    InvocationGrant::new(
        GrantId::new(format!("grant-{}", invocation.as_str())).unwrap(),
        invocation.clone(),
        prepared.input_digest(),
        prepared.tool_name(),
        prepared.tool_revision(),
        prepared.requirements().clone(),
        "a".repeat(64),
        "snapshot-1",
    )
    .unwrap()
}

async fn run_completed(root: &std::path::Path, name: &str, arguments: &str) -> serde_json::Value {
    let result = run(root, name, arguments).await;
    let ExecutionFact::Completed { content, .. } = result else {
        panic!("expected completion")
    };
    content
}

async fn run_failure(root: &std::path::Path, name: &str, arguments: &str) -> String {
    let result = run(root, name, arguments).await;
    let ExecutionFact::Failed { code, .. } = result else {
        panic!("expected safe failure")
    };
    code
}

async fn run(root: &std::path::Path, name: &str, arguments: &str) -> ExecutionFact {
    let prepared = prepare(name, arguments);
    let invocation = ToolInvocationId::new("invocation").unwrap();
    let grant = grant(&invocation, &prepared);
    let mut executor = BuiltinWorkspaceExecutor::new(root, "unix-v1", catalogue()).unwrap();
    let execution = executor.prepare(&invocation, &prepared, &grant).unwrap();
    executor
        .dispatch(ExecutorDispatch {
            invocation_id: &invocation,
            prepared: &prepared,
            grant: &grant,
            execution: &execution,
            receipt_id: "receipt",
        })
        .await
        .unwrap()
}
