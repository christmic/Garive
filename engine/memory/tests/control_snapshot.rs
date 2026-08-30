use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use garive_memory::{
    parse_memory_document, parse_memory_snapshot, project_memory_snapshot, MemoryControlError,
    MemoryDocumentLimits, MemorySnapshotFile, MemorySnapshotLimits,
};

#[test]
fn projection_and_package_validation_are_canonical() {
    let projected = project_memory_snapshot(
        "export-1",
        "namespace-1",
        7,
        "2026-08-30T12:00:00Z",
        vec![
            document("mem-b", "rev-b", "second"),
            document("mem-a", "rev-a", "first"),
        ],
    )
    .unwrap();
    assert_eq!(projected.manifest.entries[0].record_id, "mem-a");
    assert_eq!(projected.manifest.manifest_digest.len(), 64);
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/memory-control-plane-v1.json"
    ))
    .unwrap();
    assert_eq!(
        projected.manifest.manifest_digest,
        fixture["snapshot_vector"]["manifest_digest"]
            .as_str()
            .unwrap()
    );
    assert_eq!(
        projected.manifest_json,
        serde_jcs::to_vec(&projected.manifest).unwrap()
    );
    let mut files = files(&projected);
    files.push(MemorySnapshotFile {
        file_name: "entries/new-draft_1.md".into(),
        bytes: new_document("draft_1", "third").render().into_bytes(),
        storage_identity: "storage-new".into(),
        regular: true,
    });
    let verified = parse_memory_snapshot(&projected.manifest_json, &files, limits()).unwrap();
    assert_eq!(verified.documents.len(), 3);
    assert_eq!(verified.manifest, projected.manifest);
}

#[test]
fn layout_alias_canonical_digest_and_bounds_fail_closed() {
    let projected = project_memory_snapshot(
        "export-1",
        "namespace-1",
        7,
        "2026-08-30T12:00:00Z",
        vec![document("mem-a", "rev-a", "first")],
    )
    .unwrap();
    let original = files(&projected);

    let pretty = serde_json::to_vec_pretty(&projected.manifest).unwrap();
    assert_invalid(&pretty, &original);

    let mut symlink = original.clone();
    symlink[0].regular = false;
    assert_invalid(&projected.manifest_json, &symlink);

    let mut traversal = original.clone();
    traversal[0].file_name = "entries/../escape.md".into();
    assert_invalid(&projected.manifest_json, &traversal);

    let mut alias = original.clone();
    alias.push(MemorySnapshotFile {
        file_name: "entries/new-draft.md".into(),
        bytes: new_document("draft", "new").render().into_bytes(),
        storage_identity: alias[0].storage_identity.clone(),
        regular: true,
    });
    assert_invalid(&projected.manifest_json, &alias);

    let tiny = MemorySnapshotLimits::new(1, 8, MemoryDocumentLimits::new(4096, 2048, 128).unwrap())
        .unwrap();
    assert_eq!(
        parse_memory_snapshot(&projected.manifest_json, &original, tiny),
        Err(MemoryControlError::BoundExceeded),
    );
}

#[test]
fn duplicate_current_records_and_digests_are_rejected() {
    assert_eq!(
        project_memory_snapshot(
            "export-1",
            "namespace-1",
            7,
            "2026-08-30T12:00:00Z",
            vec![
                document("mem-a", "rev-a", "same"),
                document("mem-b", "rev-b", "same")
            ],
        ),
        Err(MemoryControlError::InvalidSnapshot),
    );
    assert_eq!(
        project_memory_snapshot(
            "export-1",
            "namespace-1",
            7,
            "2026-08-30T12:00:00Z",
            vec![
                document("mem-a", "rev-a", "one"),
                document("mem-a", "rev-b", "two")
            ],
        ),
        Err(MemoryControlError::InvalidSnapshot),
    );
}

fn limits() -> MemorySnapshotLimits {
    MemorySnapshotLimits::new(
        16,
        64 * 1024,
        MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
    )
    .unwrap()
}

fn files(snapshot: &garive_memory::MemorySnapshot) -> Vec<MemorySnapshotFile> {
    snapshot
        .documents
        .iter()
        .enumerate()
        .map(|(index, (name, document))| MemorySnapshotFile {
            file_name: name.clone(),
            bytes: document.render().into_bytes(),
            storage_identity: format!("storage-{index}"),
            regular: true,
        })
        .collect()
}

fn assert_invalid(manifest: &[u8], files: &[MemorySnapshotFile]) {
    assert_eq!(
        parse_memory_snapshot(manifest, files, limits()),
        Err(MemoryControlError::InvalidSnapshot),
    );
}

fn document(record: &str, revision: &str, content: &str) -> garive_memory::MemoryControlDocument {
    parse(
        format!(
            "existing.{}.{}",
            URL_SAFE_NO_PAD.encode(record),
            URL_SAFE_NO_PAD.encode(revision)
        ),
        content,
    )
}

fn new_document(token: &str, content: &str) -> garive_memory::MemoryControlDocument {
    parse(format!("new.{token}"), content)
}

fn parse(reference: String, content: &str) -> garive_memory::MemoryControlDocument {
    let value = format!(
        "---\nschema_version: 1\nrecord_ref: {reference}\nauthority: user_declared\nmemory_type: semantic\nmemory_role: preference\nscope: agent_instance\nscope_owner_b64: YWdlbnQtMDE\nlifecycle: active\nsensitivity: ordinary\n---\n{content}\n",
    );
    parse_memory_document(
        value.as_bytes(),
        MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
    )
    .unwrap()
}
