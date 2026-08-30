use garive_tools::{
    AccessMode, AccessNamespace, BuiltinT1Catalogue, PreparationErrorCode, ReplayClass, ToolIntent,
    T1_APPLY_PATCH, T1_LIST, T1_PROCESS_RUN, T1_READ_TEXT, T1_SEARCH_TEXT, T1_TOOL_REVISION,
};

#[test]
fn catalogue_freezes_all_five_v3_definitions() {
    let catalogue = BuiltinT1Catalogue::new("snapshot-1", ["rust-toolchain"]).unwrap();
    let definitions = catalogue.definitions();
    assert_eq!(definitions.len(), 5);
    assert_eq!(
        definitions
            .iter()
            .map(|value| value.name())
            .collect::<Vec<_>>(),
        [
            T1_PROCESS_RUN,
            T1_APPLY_PATCH,
            T1_LIST,
            T1_READ_TEXT,
            T1_SEARCH_TEXT
        ]
    );
    assert!(definitions.iter().all(|definition| {
        definition.revision() == T1_TOOL_REVISION && definition.prepared_contract_version() == 3
    }));
    assert_eq!(definitions[0].replay_class(), ReplayClass::NeverReplay);
    assert_eq!(
        definitions[1].replay_class(),
        ReplayClass::ReceiptRecoverable
    );
}

#[test]
fn read_list_and_search_resolve_exact_workspace_access() {
    let catalogue = BuiltinT1Catalogue::new("snapshot-1", ["rust-toolchain"]).unwrap();
    for (name, arguments, key) in [
        (
            T1_READ_TEXT,
            r#"{"path":"src/lib.rs","max_bytes":4096}"#,
            "src/lib.rs",
        ),
        (
            T1_LIST,
            r#"{"path":".","max_entries":10,"include_hidden":false}"#,
            ".",
        ),
        (
            T1_SEARCH_TEXT,
            r#"{"path":"src","query":"needle","case_sensitive":true,"max_matches":10,"max_file_bytes":4096}"#,
            "src",
        ),
    ] {
        let prepared = catalogue
            .prepare(&ToolIntent::new("call", name, arguments))
            .unwrap();
        let accesses = prepared.invocation_accesses().unwrap().values();
        assert_eq!(accesses.len(), 1);
        assert_eq!(accesses[0].namespace(), AccessNamespace::Filesystem);
        assert_eq!(accesses[0].resource_key(), key);
        assert_eq!(accesses[0].mode(), AccessMode::Read);
    }
}

#[test]
fn patch_binds_only_declared_canonical_targets() {
    let catalogue = BuiltinT1Catalogue::new("snapshot-1", ["rust-toolchain"]).unwrap();
    let digest = "a".repeat(64);
    let arguments = format!(
        r#"{{"patch":"*** Begin Patch\n*** Update File: src/lib.rs\n@@\n-old\n+new\n*** End Patch","expected_files":[{{"path":"src/lib.rs","before_digest":"{digest}"}}]}}"#
    );
    let prepared = catalogue
        .prepare(&ToolIntent::new("call", T1_APPLY_PATCH, arguments))
        .unwrap();
    let accesses = prepared.invocation_accesses().unwrap().values();
    assert_eq!(accesses.len(), 1);
    assert_eq!(accesses[0].resource_key(), "src/lib.rs");
    assert_eq!(accesses[0].mode(), AccessMode::Write);

    let mismatch = format!(
        r#"{{"patch":"*** Begin Patch\n*** Update File: other.rs\n@@\n-old\n+new\n*** End Patch","expected_files":[{{"path":"src/lib.rs","before_digest":"{digest}"}}]}}"#
    );
    assert_eq!(
        catalogue
            .prepare(&ToolIntent::new("call", T1_APPLY_PATCH, mismatch))
            .unwrap_err()
            .code(),
        PreparationErrorCode::EffectAccessInvalid
    );
}

#[test]
fn process_resolves_configured_lane_and_explicit_working_directory() {
    let catalogue = BuiltinT1Catalogue::new("snapshot-1", ["rust-toolchain"]).unwrap();
    let prepared = catalogue
        .prepare(&ToolIntent::new(
            "call",
            T1_PROCESS_RUN,
            r#"{"lane":"rust-toolchain","argv":["cargo","test"],"working_directory":".","max_output_bytes":4096,"timeout_ms":30000}"#,
        ))
        .unwrap();
    let accesses = prepared.invocation_accesses().unwrap().values();
    assert_eq!(accesses.len(), 2);
    assert_eq!(accesses[0].namespace(), AccessNamespace::Filesystem);
    assert_eq!(accesses[1].namespace(), AccessNamespace::Process);
    assert_eq!(accesses[1].mode(), AccessMode::Exclusive);

    assert_eq!(
        catalogue
            .prepare(&ToolIntent::new(
                "call",
                T1_PROCESS_RUN,
                r#"{"lane":"unknown","argv":["cargo"],"working_directory":".","max_output_bytes":4096,"timeout_ms":30000}"#,
            ))
            .unwrap_err()
            .code(),
        PreparationErrorCode::EffectAccessInvalid
    );
}

#[test]
fn directory_root_never_becomes_a_file_or_patch_target() {
    let catalogue = BuiltinT1Catalogue::new("snapshot-1", ["rust-toolchain"]).unwrap();
    assert_eq!(
        catalogue
            .prepare(&ToolIntent::new(
                "call",
                T1_READ_TEXT,
                r#"{"path":".","max_bytes":1}"#,
            ))
            .unwrap_err()
            .code(),
        PreparationErrorCode::EffectAccessInvalid
    );
}
