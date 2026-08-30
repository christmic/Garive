use garive_tools::{
    AccessMode, AccessNamespace, BuiltinT1Catalogue, PreparationErrorCode, ReplayClass, ToolIntent,
    T1_APPLY_PATCH, T1_LIST, T1_PROCESS_RUN, T1_READ_TEXT, T1_SEARCH_TEXT, T1_TOOL_REVISION,
};
use serde_json::Value;

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
            r#"{"path":".","max_entries":10,"include_hidden":false,"max_nodes":100}"#,
            ".",
        ),
        (
            T1_SEARCH_TEXT,
            r#"{"path":"src","query":"needle","case_sensitive":true,"max_matches":10,"max_file_bytes":4096,"max_nodes":100}"#,
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

#[test]
fn shared_fixture_matches_exact_preparation_semantics() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/basic-tools-v1.json"
    ))
    .unwrap();
    let catalogue = BuiltinT1Catalogue::new(
        fixture["policy_revision"].as_str().unwrap(),
        fixture["process_lanes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap()),
    )
    .unwrap();
    for case in fixture["valid_cases"].as_array().unwrap() {
        let prepared = catalogue
            .prepare(&ToolIntent::new(
                "call",
                case["tool_name"].as_str().unwrap(),
                case["arguments"].to_string(),
            ))
            .unwrap();
        assert_eq!(prepared.input_digest(), case["prepared_digest"]);
        let actual = prepared
            .invocation_accesses()
            .unwrap()
            .values()
            .iter()
            .map(|access| {
                let namespace = match access.namespace() {
                    AccessNamespace::Filesystem => "filesystem",
                    AccessNamespace::Process => "process",
                    AccessNamespace::Network => "network",
                    AccessNamespace::Runtime => "runtime",
                };
                let mode = match access.mode() {
                    AccessMode::Read => "read",
                    AccessMode::Write => "write",
                    AccessMode::Exclusive => "exclusive",
                };
                (namespace, access.resource_key(), mode)
            })
            .collect::<Vec<_>>();
        let expected = case["accesses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|access| {
                (
                    access["namespace"].as_str().unwrap(),
                    access["resource_key"].as_str().unwrap(),
                    access["mode"].as_str().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
    for case in fixture["invalid_cases"].as_array().unwrap() {
        let error = catalogue
            .prepare(&ToolIntent::new(
                "call",
                case["tool_name"].as_str().unwrap(),
                case["arguments"].to_string(),
            ))
            .unwrap_err();
        let expected = match case["error"].as_str().unwrap() {
            "effect_access_invalid" => PreparationErrorCode::EffectAccessInvalid,
            "arguments_schema_mismatch" => PreparationErrorCode::ArgumentsSchemaMismatch,
            value => panic!("unknown fixture error {value}"),
        };
        assert_eq!(error.code(), expected);
    }
}
