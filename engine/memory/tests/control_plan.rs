use garive_memory::{
    parse_memory_document, prepare_memory_import, MemoryAuthority, MemoryAuthorizedScope,
    MemoryControlDocument, MemoryControlError, MemoryCurrentEntry, MemoryDocumentLimits,
    MemoryIdentityAllocation, MemoryImportOperation, MemoryScopeClass,
};

const DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn plans_all_variants_in_canonical_order_with_exact_counts() {
    let originals = [
        document("mem-a", "rev-a", "user_declared", "active", false, "old a"),
        document("mem-b", "rev-b", "agent_learned", "active", false, "old b"),
        document("mem-c", "rev-c", "user_declared", "cold", false, "old c"),
        document("mem-d", "rev-d", "user_declared", "active", false, "old d"),
        document("mem-e", "rev-e", "user_declared", "active", false, "old e"),
    ];
    let current = originals
        .iter()
        .enumerate()
        .map(|(index, value)| {
            current(
                value,
                if index == 1 {
                    MemoryAuthority::AgentLearned
                } else {
                    MemoryAuthority::UserDeclared
                },
            )
        })
        .collect::<Vec<_>>();
    let documents = vec![
        new_document("draft-1", "new value"),
        document("mem-e", "rev-e", "user_declared", "active", false, "old e"),
        document("mem-d", "rev-d", "user_declared", "active", true, "old d"),
        document(
            "mem-c",
            "rev-c",
            "user_declared",
            "archived",
            false,
            "old c",
        ),
        document(
            "mem-b",
            "rev-b",
            "user_declared",
            "active",
            false,
            "edited b",
        ),
        document(
            "mem-a",
            "rev-a",
            "user_declared",
            "active",
            false,
            "edited a",
        ),
    ];
    let allocations = vec![
        MemoryIdentityAllocation::Supersede {
            record_id: "mem-a".into(),
            revision_id: "rev-a2".into(),
        },
        MemoryIdentityAllocation::Supersede {
            record_id: "mem-b".into(),
            revision_id: "rev-b2".into(),
        },
        MemoryIdentityAllocation::Add {
            draft_token: "draft-1".into(),
            record_id: "mem-f".into(),
            revision_id: "rev-f1".into(),
        },
    ];
    let plan = prepare_memory_import(
        "export-1",
        "namespace-1",
        7,
        DIGEST,
        7,
        &documents,
        &current,
        &[MemoryAuthorizedScope {
            scope: MemoryScopeClass::AgentInstance,
            owner_id: "agent-01".into(),
        }],
        &allocations,
    )
    .unwrap();
    assert_eq!(
        (
            plan.add_count,
            plan.supersede_count,
            plan.archive_count,
            plan.erase_count
        ),
        (1, 2, 1, 1)
    );
    assert_eq!(plan.plan_digest.len(), 64);
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/memory-control-plane-v1.json"
    ))
    .unwrap();
    assert_eq!(
        plan.plan_digest,
        fixture["plan_vector"]["plan_digest"].as_str().unwrap()
    );
    plan.verify().unwrap();
    assert!(plan.canonical_operations_json().unwrap().starts_with('['));
    let mut corrupt = plan.clone();
    corrupt.add_count += 1;
    assert_eq!(corrupt.verify(), Err(MemoryControlError::InvalidSnapshot));
    assert!(
        matches!(&plan.operations[0], MemoryImportOperation::Supersede { record_id, .. } if record_id == "mem-a")
    );
    assert!(
        matches!(&plan.operations[1], MemoryImportOperation::Supersede { supersedes_learned_revision_id: Some(value), .. } if value == "rev-b")
    );
    assert!(
        matches!(&plan.operations[2], MemoryImportOperation::Archive { record_id, .. } if record_id == "mem-c")
    );
    assert!(
        matches!(&plan.operations[3], MemoryImportOperation::Erase { record_id, .. } if record_id == "mem-d")
    );
    assert!(
        matches!(&plan.operations[4], MemoryImportOperation::Add { record_id, .. } if record_id == "mem-f")
    );
}

#[test]
fn stale_authority_and_metadata_changes_fail_closed() {
    let original = document("mem-a", "rev-a", "user_declared", "active", false, "old");
    let current = vec![current(&original, MemoryAuthority::UserDeclared)];
    assert_eq!(
        plan(std::slice::from_ref(&original), &current, 8).unwrap_err(),
        MemoryControlError::StaleSnapshot,
    );
    let widened = parse(
        document_text(
            "existing.bWVtLWE.cmV2LWE",
            "user_declared",
            "active",
            false,
            "old",
        )
        .replace("scope: agent_instance", "scope: platform"),
    );
    assert_eq!(
        plan(&[widened], &current, 7).unwrap_err(),
        MemoryControlError::ForbiddenChange,
    );
    assert_eq!(
        plan(&[original.clone(), original], &current, 7).unwrap_err(),
        MemoryControlError::InvalidSnapshot,
    );
}

fn plan(
    documents: &[MemoryControlDocument],
    current: &[MemoryCurrentEntry],
    revision: u64,
) -> Result<garive_memory::MemoryImportPlan, MemoryControlError> {
    prepare_memory_import(
        "export-1",
        "namespace-1",
        7,
        DIGEST,
        revision,
        documents,
        current,
        &[],
        &[],
    )
}

fn current(value: &MemoryControlDocument, authority: MemoryAuthority) -> MemoryCurrentEntry {
    MemoryCurrentEntry {
        record_id: value.record_ref().record_id().unwrap().into(),
        revision_id: value.record_ref().revision_id().unwrap().into(),
        authority,
        memory_type: value.memory_type(),
        memory_role: value.memory_role(),
        scope: value.scope(),
        scope_owner_id: value.scope_owner_id().into(),
        lifecycle: value.lifecycle(),
        sensitivity: value.sensitivity(),
        content_digest: value.content_digest(),
    }
}

fn document(
    record: &str,
    revision: &str,
    authority: &str,
    lifecycle: &str,
    erase: bool,
    content: &str,
) -> MemoryControlDocument {
    let record_ref = format!(
        "existing.{}.{}",
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, record),
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, revision),
    );
    parse(document_text(
        &record_ref,
        authority,
        lifecycle,
        erase,
        content,
    ))
}

fn new_document(token: &str, content: &str) -> MemoryControlDocument {
    parse(document_text(
        &format!("new.{token}"),
        "user_declared",
        "active",
        false,
        content,
    ))
}

fn document_text(
    record_ref: &str,
    authority: &str,
    lifecycle: &str,
    erase: bool,
    content: &str,
) -> String {
    format!(
        "---\nschema_version: 1\nrecord_ref: {record_ref}\nauthority: {authority}\nmemory_type: semantic\nmemory_role: preference\nscope: agent_instance\nscope_owner_b64: YWdlbnQtMDE\nlifecycle: {lifecycle}\nsensitivity: ordinary\n{}---\n{content}\n",
        if erase { "erase: true\n" } else { "" },
    )
}

fn parse(value: String) -> MemoryControlDocument {
    parse_memory_document(
        value.as_bytes(),
        MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
    )
    .unwrap()
}
