use garive_runtime::{
    BrowserPageId, BrowserSessionId, NativeNodeRef, NativeObservationBounds, NativeObservationV1,
    NativeProtocolError, NativeSemanticNode, NativeSensitivity, NativeSnapshotId, NativeTarget,
};

fn node(id: &str, parent: Option<&str>, name: &str) -> NativeSemanticNode {
    NativeSemanticNode {
        node_ref: NativeNodeRef::new(id).expect("node id"),
        parent_ref: parent.map(|value| NativeNodeRef::new(value).expect("parent id")),
        role: "button".into(),
        name: Some(name.into()),
        value_summary: None,
        states: vec!["enabled".into(), "focused".into()],
        actions: vec!["press".into()],
        sensitivity: NativeSensitivity::Public,
    }
}

fn observation(nodes: Vec<NativeSemanticNode>) -> NativeObservationV1 {
    NativeObservationV1 {
        target: NativeTarget::Browser {
            session_id: BrowserSessionId::new("session-1").expect("session"),
            page_id: BrowserPageId::new("page-1").expect("page"),
        },
        snapshot_id: NativeSnapshotId::new("snapshot-1").expect("snapshot"),
        target_revision: "revision-1".into(),
        nodes,
        focused_node: None,
        screenshot_reference: None,
        redacted_field_count: 0,
        bounds: NativeObservationBounds {
            max_nodes: 10,
            max_text_bytes: 100,
        },
    }
}

#[test]
fn observation_accepts_parent_before_child_and_exact_focus_scope() {
    let mut value = observation(vec![
        node("root", None, "Root"),
        node("child", Some("root"), "Go"),
    ]);
    value.focused_node = Some(NativeNodeRef::new("child").expect("focus"));
    assert_eq!(value.validate(), Ok(()));
}

#[test]
fn observation_rejects_forward_parent_duplicate_and_unknown_focus() {
    assert_eq!(
        observation(vec![
            node("child", Some("root"), "Go"),
            node("root", None, "Root")
        ])
        .validate(),
        Err(NativeProtocolError::InvalidBinding)
    );
    assert_eq!(
        observation(vec![node("same", None, "One"), node("same", None, "Two")]).validate(),
        Err(NativeProtocolError::InvalidBinding)
    );
    let mut value = observation(vec![node("root", None, "Root")]);
    value.focused_node = Some(NativeNodeRef::new("missing").expect("focus"));
    assert_eq!(value.validate(), Err(NativeProtocolError::InvalidBinding));
}

#[test]
fn observation_enforces_text_and_token_bounds() {
    let mut value = observation(vec![node("root", None, "too long")]);
    value.bounds.max_text_bytes = 4;
    assert_eq!(
        value.validate(),
        Err(NativeProtocolError::ResultBoundExceeded)
    );
    value.bounds.max_text_bytes = 100;
    value.nodes[0].states = vec!["focused".into(), "enabled".into()];
    assert_eq!(value.validate(), Err(NativeProtocolError::InvalidBinding));
}

#[test]
fn stable_failures_keep_the_accepted_codes() {
    assert_eq!(
        NativeProtocolError::SnapshotStale.code(),
        "native_snapshot_stale"
    );
    assert_eq!(
        NativeProtocolError::FocusChanged.code(),
        "native_focus_changed"
    );
    assert_eq!(
        NativeProtocolError::ActionUncertain.code(),
        "native_action_uncertain"
    );
}
