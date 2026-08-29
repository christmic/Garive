use garive_memory::{
    parse_memory_document, HypothesisState, MemoryAuthority, MemoryControlError,
    MemoryDocumentLimits, MemoryScopeClass, MemoryType,
};

fn limits() -> MemoryDocumentLimits {
    MemoryDocumentLimits::new(2_048, 1_024, 64).unwrap()
}

const DOCUMENT: &str = "---\nschema_version: 1\nmemory_id: mem-01\nrevision: 4\nauthority: user_declared\nkind: semantic\nscope: agent_instance\nlifecycle: active\n---\nPrefer concise status updates.\n";

#[test]
fn strict_document_round_trips_and_normalizes_crlf() {
    let parsed =
        parse_memory_document(DOCUMENT.replace('\n', "\r\n").as_bytes(), limits()).unwrap();
    assert_eq!(parsed.memory_id(), "mem-01");
    assert_eq!(parsed.revision(), 4);
    assert_eq!(parsed.authority(), MemoryAuthority::UserDeclared);
    assert_eq!(parsed.memory_type(), MemoryType::Semantic);
    assert_eq!(parsed.scope(), MemoryScopeClass::AgentInstance);
    assert_eq!(parsed.lifecycle(), HypothesisState::Active);
    assert!(!parsed.erase_requested());
    assert_eq!(parsed.render(), DOCUMENT);
    assert_eq!(parsed.content_digest().len(), 64);
}

#[test]
fn optional_erasure_is_exact_and_canonical() {
    let input = DOCUMENT.replace("lifecycle: active\n", "lifecycle: archived\nerase: true\n");
    let parsed = parse_memory_document(input.as_bytes(), limits()).unwrap();
    assert!(parsed.erase_requested());
    assert_eq!(parsed.lifecycle(), HypothesisState::Archived);
    assert_eq!(parsed.render(), input);
}

#[test]
fn rejects_unknown_order_duplicates_tokens_and_bounds() {
    for invalid in [
        DOCUMENT.replacen("memory_id", "unknown", 1),
        DOCUMENT.replacen("revision: 4", "revision: 0", 1),
        DOCUMENT.replacen("semantic", "future", 1),
        DOCUMENT.replacen("mem-01", "mem/01", 1),
        DOCUMENT.replacen(
            "schema_version: 1\n",
            "schema_version: 1\nschema_version: 1\n",
            1,
        ),
    ] {
        assert!(parse_memory_document(invalid.as_bytes(), limits()).is_err());
    }
    let tiny = MemoryDocumentLimits::new(8, 8, 8).unwrap();
    assert_eq!(
        parse_memory_document(DOCUMENT.as_bytes(), tiny),
        Err(MemoryControlError::BoundExceeded)
    );
}
