use garive_memory::{
    parse_memory_document, HypothesisState, MemoryAuthority, MemoryControlError,
    MemoryDocumentLimits, MemoryKind, MemoryRecordRef, MemoryScopeClass, MemorySensitivity,
    MemoryType,
};

fn limits() -> MemoryDocumentLimits {
    MemoryDocumentLimits::new(2_048, 1_024, 64).unwrap()
}

const DOCUMENT: &str = "---\nschema_version: 1\nrecord_ref: existing.bWVtLTAx.cmV2LTA0\nauthority: user_declared\nmemory_type: semantic\nmemory_role: preference\nscope: agent_instance\nscope_owner_b64: YWdlbnQtMDE\nlifecycle: active\nsensitivity: ordinary\n---\nPrefer concise status updates.\n";

#[test]
fn exact_identities_round_trip_and_crlf_normalizes() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/memory-control-plane-v1.json"
    ))
    .unwrap();
    let canonical = fixture["document_vector"]["canonical_markdown"]
        .as_str()
        .unwrap();
    let parsed =
        parse_memory_document(canonical.replace('\n', "\r\n").as_bytes(), limits()).unwrap();
    assert_eq!(parsed.record_ref().record_id(), Some("mem-01"));
    assert_eq!(parsed.record_ref().revision_id(), Some("rev-04"));
    assert_eq!(parsed.authority(), MemoryAuthority::UserDeclared);
    assert_eq!(parsed.memory_type(), MemoryType::Semantic);
    assert_eq!(parsed.memory_role(), MemoryKind::Preference);
    assert_eq!(parsed.scope(), MemoryScopeClass::AgentInstance);
    assert_eq!(parsed.scope_owner_id(), "agent-01");
    assert_eq!(parsed.lifecycle(), HypothesisState::Active);
    assert_eq!(parsed.sensitivity(), MemorySensitivity::Ordinary);
    assert_eq!(parsed.render(), canonical);
    assert_eq!(
        parsed.content_digest(),
        fixture["document_vector"]["content_digest"]
            .as_str()
            .unwrap()
    );
    assert_eq!(
        parsed.document_digest(),
        fixture["document_vector"]["document_digest"]
            .as_str()
            .unwrap()
    );
}

#[test]
fn repository_fields_build_the_same_canonical_document() {
    let built = garive_memory::MemoryControlDocument::from_repository_record(
        "mem-01",
        "rev-04",
        MemoryAuthority::UserDeclared,
        MemoryType::Semantic,
        MemoryKind::Preference,
        MemoryScopeClass::AgentInstance,
        "agent-01",
        HypothesisState::Active,
        MemorySensitivity::Ordinary,
        "Prefer concise status updates.\r\n\r\n",
        limits(),
    )
    .unwrap();
    assert_eq!(built.render(), DOCUMENT);
    assert_eq!(
        built.with_lifecycle(HypothesisState::Cold).lifecycle(),
        HypothesisState::Cold,
    );
    assert_eq!(
        garive_memory::MemoryControlDocument::from_repository_record(
            "mem-01",
            "rev-04",
            MemoryAuthority::AgentLearned,
            MemoryType::Semantic,
            MemoryKind::Preference,
            MemoryScopeClass::Session,
            "session",
            HypothesisState::Candidate,
            MemorySensitivity::Ordinary,
            "",
            limits(),
        ),
        Err(MemoryControlError::InvalidSnapshot),
    );
}

#[test]
fn new_and_erasure_forms_are_exact() {
    let new = DOCUMENT.replace("existing.bWVtLTAx.cmV2LTA0", "new.draft_1");
    assert_eq!(
        parse_memory_document(new.as_bytes(), limits())
            .unwrap()
            .record_ref(),
        &MemoryRecordRef::New {
            draft_token: "draft_1".into()
        },
    );
    let erased = DOCUMENT.replace(
        "sensitivity: ordinary\n",
        "sensitivity: ordinary\nerase: true\n",
    );
    assert!(parse_memory_document(erased.as_bytes(), limits())
        .unwrap()
        .erase_requested());
}

#[test]
fn rejects_aliases_noncanonical_base64_shape_and_bounds() {
    for invalid in [
        DOCUMENT.replacen("record_ref", "unknown", 1),
        DOCUMENT.replacen("bWVtLTAx", "bWVtLTAx=", 1),
        DOCUMENT.replacen("cmV2LTA0", "", 1),
        DOCUMENT.replacen("preference", "future", 1),
        DOCUMENT.replacen("YWdlbnQtMDE", "YWdlbnQtMDE=", 1),
        DOCUMENT.replacen(
            "sensitivity: ordinary\n",
            "sensitivity: ordinary\nerase: false\n",
            1,
        ),
        DOCUMENT.replacen(
            "schema_version: 1\n",
            "schema_version: 1\nschema_version: 1\n",
            1,
        ),
    ] {
        assert_eq!(
            parse_memory_document(invalid.as_bytes(), limits())
                .unwrap_err()
                .wire_name(),
            "memory_snapshot_invalid",
        );
    }
    let tiny = MemoryDocumentLimits::new(8, 8, 8).unwrap();
    assert_eq!(
        parse_memory_document(DOCUMENT.as_bytes(), tiny),
        Err(MemoryControlError::BoundExceeded),
    );
}
