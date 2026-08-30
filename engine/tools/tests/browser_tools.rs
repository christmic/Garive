use garive_tools::{
    AccessMode, AccessNamespace, BrowserPageScope, BuiltinT2BrowserCatalogue, PreparationErrorCode,
    ReplayClass, ToolIntent, T2_BROWSER_ACT, T2_BROWSER_NAVIGATE, T2_BROWSER_OBSERVE,
};
use serde_json::Value;

fn catalogue() -> BuiltinT2BrowserCatalogue {
    BuiltinT2BrowserCatalogue::new(
        "browser-policy-1",
        [BrowserPageScope::new("session-1", "page-1").unwrap()],
        ["https://example.com:443", "https://next.example:443"],
    )
    .unwrap()
}

#[test]
fn catalogue_freezes_observe_and_never_replay_actions() {
    let catalogue = catalogue();
    assert_eq!(
        catalogue
            .definitions()
            .iter()
            .map(|definition| definition.name())
            .collect::<Vec<_>>(),
        [T2_BROWSER_ACT, T2_BROWSER_NAVIGATE, T2_BROWSER_OBSERVE]
    );
    assert_eq!(
        catalogue.definitions()[0].replay_class(),
        ReplayClass::NeverReplay
    );
    assert_eq!(
        catalogue.definitions()[2].replay_class(),
        ReplayClass::ReadOnly
    );
}

#[test]
fn observe_binds_exact_page_read_and_navigation_binds_page_and_origin_writes() {
    let catalogue = catalogue();
    let observe = catalogue
        .prepare(&ToolIntent::new(
            "call-observe",
            T2_BROWSER_OBSERVE,
            r#"{"session_id":"session-1","page_id":"page-1","max_nodes":100,"max_text_bytes":4096}"#,
        ))
        .unwrap();
    let access = &observe.invocation_accesses().unwrap().values()[0];
    assert_eq!(access.namespace(), AccessNamespace::Runtime);
    assert_eq!(access.resource_key(), "browser:session-1:page-1");
    assert_eq!(access.mode(), AccessMode::Read);

    let navigate = catalogue
        .prepare(&ToolIntent::new(
            "call-nav",
            T2_BROWSER_NAVIGATE,
            r#"{"session_id":"session-1","page_id":"page-1","expected_snapshot_id":"snapshot-1","target_revision":"nav-1","destination_url":"https://example.com:443/path","destination_origin":"https://example.com:443","wait_until":"load","timeout_ms":1000,"max_nodes":100,"max_text_bytes":4096}"#,
        ))
        .unwrap();
    let accesses = navigate.invocation_accesses().unwrap().values();
    assert_eq!(accesses.len(), 2);
    assert_eq!(accesses[0].namespace(), AccessNamespace::Network);
    assert_eq!(accesses[1].namespace(), AccessNamespace::Runtime);
    assert!(accesses
        .iter()
        .all(|access| access.mode() == AccessMode::Write));
}

#[test]
fn action_shapes_origins_and_scope_fail_closed() {
    let catalogue = catalogue();
    let valid = catalogue
        .prepare(&ToolIntent::new(
            "call-act",
            T2_BROWSER_ACT,
            r#"{"session_id":"session-1","page_id":"page-1","expected_snapshot_id":"snapshot-1","target_revision":"nav-1","action":"click","node_ref":"node-7","allowed_navigation_origins":["https://next.example:443"]}"#,
        ))
        .unwrap();
    assert_eq!(valid.invocation_accesses().unwrap().values().len(), 2);

    for arguments in [
        r#"{"session_id":"session-1","page_id":"page-1","expected_snapshot_id":"snapshot-1","target_revision":"nav-1","action":"click","allowed_navigation_origins":[]}"#,
        r#"{"session_id":"session-1","page_id":"other","expected_snapshot_id":"snapshot-1","target_revision":"nav-1","action":"reload","allowed_navigation_origins":[]}"#,
        r#"{"session_id":"session-1","page_id":"page-1","expected_snapshot_id":"snapshot-1","target_revision":"nav-1","action":"press_key","key":"enter","text":"extra","allowed_navigation_origins":[]}"#,
        r#"{"session_id":"session-1","page_id":"page-1","expected_snapshot_id":"snapshot-1","target_revision":"nav-1","action":"scroll","delta_x":0,"delta_y":0,"allowed_navigation_origins":[]}"#,
    ] {
        assert_eq!(
            catalogue
                .prepare(&ToolIntent::new("bad", T2_BROWSER_ACT, arguments))
                .unwrap_err()
                .code(),
            PreparationErrorCode::EffectAccessInvalid
        );
    }
}

#[test]
fn destination_url_must_match_exact_canonical_origin() {
    assert_eq!(
        catalogue()
            .prepare(&ToolIntent::new(
                "bad-nav",
                T2_BROWSER_NAVIGATE,
                r#"{"session_id":"session-1","page_id":"page-1","expected_snapshot_id":"snapshot-1","target_revision":"nav-1","destination_url":"https://other.example:443/path","destination_origin":"https://example.com:443","wait_until":"load","timeout_ms":1000,"max_nodes":100,"max_text_bytes":4096}"#,
            ))
            .unwrap_err()
            .code(),
        PreparationErrorCode::EffectAccessInvalid
    );
}

#[test]
fn shared_fixture_matches_exact_browser_preparation() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/browser-tools-v1.json"
    ))
    .unwrap();
    let catalogue = BuiltinT2BrowserCatalogue::new(
        fixture["policy_revision"].as_str().unwrap(),
        fixture["pages"].as_array().unwrap().iter().map(|page| {
            BrowserPageScope::new(
                page["session_id"].as_str().unwrap(),
                page["page_id"].as_str().unwrap(),
            )
            .unwrap()
        }),
        fixture["origins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|origin| origin.as_str().unwrap()),
    )
    .unwrap();
    for case in fixture["valid_cases"].as_array().unwrap() {
        let prepared = catalogue
            .prepare(&ToolIntent::new(
                "fixture-call",
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
                (
                    match access.namespace() {
                        AccessNamespace::Filesystem => "filesystem",
                        AccessNamespace::Process => "process",
                        AccessNamespace::Network => "network",
                        AccessNamespace::Runtime => "runtime",
                    },
                    access.resource_key(),
                    match access.mode() {
                        AccessMode::Read => "read",
                        AccessMode::Write => "write",
                        AccessMode::Exclusive => "exclusive",
                    },
                )
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
                "fixture-bad",
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
