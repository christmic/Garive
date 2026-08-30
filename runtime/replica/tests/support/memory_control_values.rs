use garive_memory::{
    parse_memory_document, MemoryAuthority, MemoryAuthorizedScope, MemoryControlDocument,
    MemoryCurrentEntry, MemoryDocumentLimits, MemoryScopeClass,
};
use garive_runtime::{MemoryControlAction, MemoryControlGrant, SqliteLedger};

pub const EMPTY_DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

pub fn originals() -> Vec<MemoryControlDocument> {
    vec![
        document("mem-a", "rev-a", "user_declared", "active", false, "old a"),
        document("mem-b", "rev-b", "user_declared", "active", false, "old b"),
        document("mem-c", "rev-c", "user_declared", "active", false, "old c"),
    ]
}

pub fn current(value: &MemoryControlDocument) -> MemoryCurrentEntry {
    MemoryCurrentEntry {
        record_id: value.record_ref().record_id().unwrap().into(),
        revision_id: value.record_ref().revision_id().unwrap().into(),
        authority: MemoryAuthority::UserDeclared,
        memory_type: value.memory_type(),
        memory_role: value.memory_role(),
        scope: value.scope(),
        scope_owner_id: value.scope_owner_id().into(),
        lifecycle: value.lifecycle(),
        sensitivity: value.sensitivity(),
        content_digest: value.content_digest(),
    }
}

pub fn scope_set() -> Vec<MemoryAuthorizedScope> {
    vec![MemoryAuthorizedScope {
        scope: MemoryScopeClass::AgentInstance,
        owner_id: "agent-01".into(),
    }]
}

pub fn grant(namespace: &str, scopes: Vec<MemoryAuthorizedScope>) -> MemoryControlGrant {
    MemoryControlGrant::new(
        namespace,
        [MemoryControlAction::Import, MemoryControlAction::Export],
        scopes,
    )
    .unwrap()
}

pub fn document(
    record: &str,
    revision: &str,
    authority: &str,
    lifecycle: &str,
    erase: bool,
    content: &str,
) -> MemoryControlDocument {
    let reference = format!(
        "existing.{}.{}",
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, record),
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, revision),
    );
    parse(document_text(
        &reference, authority, lifecycle, erase, content,
    ))
}

pub fn new_document(token: &str, content: &str) -> MemoryControlDocument {
    parse(document_text(
        &format!("new.{token}"),
        "user_declared",
        "active",
        false,
        content,
    ))
}

fn document_text(
    reference: &str,
    authority: &str,
    lifecycle: &str,
    erase: bool,
    content: &str,
) -> String {
    format!("---\nschema_version: 1\nrecord_ref: {reference}\nauthority: {authority}\nmemory_type: semantic\nmemory_role: preference\nscope: agent_instance\nscope_owner_b64: YWdlbnQtMDE\nlifecycle: {lifecycle}\nsensitivity: ordinary\n{}---\n{content}\n", if erase { "erase: true\n" } else { "" })
}

fn parse(value: String) -> MemoryControlDocument {
    parse_memory_document(
        value.as_bytes(),
        MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
    )
    .unwrap()
}

pub fn scalar(connection: &rusqlite::Connection, sql: &str) -> i64 {
    connection.query_row(sql, [], |row| row.get(0)).unwrap()
}

pub fn repository_revision(ledger: &SqliteLedger) -> u64 {
    let bytes: Vec<u8> = ledger
        .connection_for_test()
        .query_row(
            "SELECT repository_revision FROM memory_namespaces",
            [],
            |row| row.get(0),
        )
        .unwrap();
    u64::from_be_bytes(bytes.try_into().unwrap())
}
