use garive_browser_cdp::{CdpAxNode, CdpAxProperty, CdpAxTree};
use garive_runtime::{
    map_cdp_ax_tree, map_cdp_ax_tree_with_binding, BrowserPageId, BrowserSessionId,
    CdpObservationContext, NativeNodeRef, NativeObservationBounds, NativeProtocolError,
    NativeSensitivity, NativeSnapshotId, NativeTarget,
};
use serde_json::json;

fn node(id: &str, parent: Option<&str>, role: &str, ignored: bool) -> CdpAxNode {
    CdpAxNode {
        node_id: id.into(),
        ignored,
        role: Some(role.into()),
        name: Some(format!("name-{id}")),
        value_summary: None,
        properties: vec![],
        parent_id: parent.map(str::to_owned),
        child_ids: vec![],
        backend_dom_node_id: None,
        frame_id: Some("frame-1".into()),
    }
}

fn context() -> CdpObservationContext {
    CdpObservationContext {
        target: NativeTarget::Browser {
            session_id: BrowserSessionId::new("browser-1").expect("browser"),
            page_id: BrowserPageId::new("page-1").expect("page"),
        },
        snapshot_id: NativeSnapshotId::new("snapshot-1").expect("snapshot"),
        target_revision: "revision-1".into(),
        bounds: NativeObservationBounds {
            max_nodes: 16,
            max_text_bytes: 4_096,
        },
    }
}

#[test]
fn mapping_topologizes_folds_ignored_nodes_and_redacts_protected_values() {
    let mut button = node("button-cdp", Some("ignored-cdp"), "button", false);
    button.properties.push(CdpAxProperty {
        name: "focusable".into(),
        value: json!({"type":"booleanOrUndefined","value":true}),
    });
    let mut password = node("password-cdp", Some("root-cdp"), "secureTextField", false);
    password.value_summary = Some("secret".into());
    password.properties.push(CdpAxProperty {
        name: "protected".into(),
        value: json!({"type":"boolean","value":true}),
    });
    let tree = CdpAxTree {
        nodes: vec![
            button,
            node("ignored-cdp", Some("root-cdp"), "generic", true),
            password,
            node("root-cdp", None, "RootWebArea", false),
        ],
    };
    let observation = map_cdp_ax_tree(context(), &tree).expect("observation");
    assert_eq!(observation.nodes.len(), 3);
    assert_eq!(observation.nodes[0].role, "root_web_area");
    assert_eq!(observation.nodes[1].role, "button");
    assert_eq!(
        observation.nodes[1].parent_ref,
        Some(observation.nodes[0].node_ref.clone())
    );
    assert_eq!(observation.nodes[1].actions, vec!["click"]);
    assert_eq!(observation.nodes[1].states, vec!["focusable"]);
    assert_ne!(observation.nodes[1].node_ref.as_str(), "button-cdp");
    assert_eq!(
        observation.nodes[2].sensitivity,
        NativeSensitivity::Redacted
    );
    assert_eq!(observation.nodes[2].name.as_deref(), Some("[redacted]"));
    assert_eq!(
        observation.nodes[2].value_summary.as_deref(),
        Some("[redacted]")
    );
    assert_eq!(observation.redacted_field_count, 2);
}

#[test]
fn mapping_rejects_missing_or_cyclic_parent_evidence() {
    assert_eq!(
        map_cdp_ax_tree(
            context(),
            &CdpAxTree {
                nodes: vec![node("child", Some("missing"), "button", false)]
            }
        ),
        Err(NativeProtocolError::ReceiptInvalid)
    );
    assert_eq!(
        map_cdp_ax_tree(
            context(),
            &CdpAxTree {
                nodes: vec![
                    node("one", Some("two"), "button", false),
                    node("two", Some("one"), "button", false),
                ]
            }
        ),
        Err(NativeProtocolError::ReceiptInvalid)
    );
}

#[test]
fn private_binding_resolves_only_the_exact_snapshot_node_revision_and_action() {
    let mut button = node("button-cdp", Some("root-cdp"), "button", false);
    button.backend_dom_node_id = Some(42);
    let mut textbox = node("textbox-cdp", Some("root-cdp"), "textbox", false);
    textbox.backend_dom_node_id = Some(43);
    textbox.properties.push(CdpAxProperty {
        name: "focused".into(),
        value: json!({"type":"booleanOrUndefined","value":true}),
    });
    let mapped = map_cdp_ax_tree_with_binding(
        context(),
        &CdpAxTree {
            nodes: vec![
                button,
                textbox,
                node("root-cdp", None, "RootWebArea", false),
            ],
        },
    )
    .expect("mapped observation");
    let target = mapped.observation.target.clone();
    let snapshot = mapped.observation.snapshot_id.clone();
    let button_ref = mapped
        .observation
        .nodes
        .iter()
        .find(|node| node.role == "button")
        .expect("button")
        .node_ref
        .clone();
    let textbox_ref = mapped
        .observation
        .nodes
        .iter()
        .find(|node| node.role == "textbox")
        .expect("textbox")
        .node_ref
        .clone();
    assert_eq!(mapped.observation.focused_node, Some(textbox_ref.clone()));
    assert_eq!(
        mapped
            .binding
            .resolve_focus(&target, &snapshot, "revision-1")
            .expect("focus target")
            .backend_dom_node_id,
        43
    );
    assert_eq!(
        mapped
            .binding
            .resolve_click(&target, &snapshot, "revision-1", &button_ref)
            .expect("click target")
            .backend_dom_node_id,
        42
    );
    assert_eq!(
        mapped
            .binding
            .resolve_type_text(&target, &snapshot, "revision-1", &textbox_ref)
            .expect("type target")
            .backend_dom_node_id,
        43
    );
    assert_eq!(
        mapped
            .binding
            .resolve_clear(&target, &snapshot, "revision-1", &textbox_ref)
            .expect("clear target")
            .backend_dom_node_id,
        43
    );
    assert_eq!(
        mapped
            .binding
            .resolve_type_text(&target, &snapshot, "revision-1", &button_ref),
        Err(NativeProtocolError::ActionUnsupported)
    );
    assert_eq!(
        mapped.binding.resolve_click(
            &target,
            &NativeSnapshotId::new("snapshot-old").expect("snapshot"),
            "revision-1",
            &button_ref,
        ),
        Err(NativeProtocolError::SnapshotStale)
    );
    assert_eq!(
        mapped
            .binding
            .resolve_click(&target, &snapshot, "revision-old", &button_ref,),
        Err(NativeProtocolError::SnapshotStale)
    );
    assert_eq!(
        mapped.binding.resolve_click(
            &target,
            &snapshot,
            "revision-1",
            &NativeNodeRef::new("node-missing").expect("node"),
        ),
        Err(NativeProtocolError::NodeStale)
    );
}
