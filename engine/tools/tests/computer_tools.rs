use garive_tools::{
    AccessMode, AccessNamespace, BuiltinT2ComputerCatalogue, ComputerTargetScope,
    PreparationErrorCode, ReplayClass, ToolIntent, T2_COMPUTER_ACT, T2_COMPUTER_OBSERVE,
};

fn catalogue() -> BuiltinT2ComputerCatalogue {
    BuiltinT2ComputerCatalogue::new(
        "computer-policy-1",
        [ComputerTargetScope::new("desktop-1", "app-1", "window-1").unwrap()],
    )
    .unwrap()
}

#[test]
fn catalogue_freezes_read_only_observe_and_never_replay_act() {
    let definitions = catalogue().definitions().to_vec();
    assert_eq!(
        definitions
            .iter()
            .map(|definition| definition.name())
            .collect::<Vec<_>>(),
        [T2_COMPUTER_ACT, T2_COMPUTER_OBSERVE]
    );
    assert_eq!(definitions[0].replay_class(), ReplayClass::NeverReplay);
    assert_eq!(definitions[1].replay_class(), ReplayClass::ReadOnly);
}

#[test]
fn observe_binds_one_exact_native_target_read() {
    let prepared = catalogue()
        .prepare(&ToolIntent::new(
            "observe",
            T2_COMPUTER_OBSERVE,
            r#"{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","max_nodes":100,"max_text_bytes":4096,"capture":"none","max_capture_bytes":1024,"max_capture_pixels":1000}"#,
        ))
        .unwrap();
    let access = &prepared.invocation_accesses().unwrap().values()[0];
    assert_eq!(access.namespace(), AccessNamespace::Runtime);
    assert_eq!(access.resource_key(), "computer:desktop-1:app-1:window-1");
    assert_eq!(access.mode(), AccessMode::Read);
}

#[test]
fn semantic_actions_never_accept_coordinate_or_mixed_fallbacks() {
    let catalogue = catalogue();
    let prepared = catalogue
        .prepare(&ToolIntent::new(
            "press",
            T2_COMPUTER_ACT,
            r#"{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"press","node_ref":"node-1"}"#,
        ))
        .unwrap();
    assert_eq!(
        prepared.invocation_accesses().unwrap().values()[0].mode(),
        AccessMode::Write
    );
    for arguments in [
        r#"{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"press"}"#,
        r#"{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"press","node_ref":"node-1","point_x":10}"#,
        r#"{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"scroll","node_ref":"node-1","delta_x":0,"delta_y":0}"#,
    ] {
        assert_eq!(
            catalogue
                .prepare(&ToolIntent::new("bad", T2_COMPUTER_ACT, arguments))
                .unwrap_err()
                .code(),
            PreparationErrorCode::EffectAccessInvalid
        );
    }
}

#[test]
fn coordinate_actions_bind_complete_geometry_and_visible_points() {
    let valid = r#"{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"click_point","display_id":"display-1","point_x":200,"point_y":200,"snapshot_pixel_width":1000,"snapshot_pixel_height":800,"scale_milli":2000,"visible_frame_x":100,"visible_frame_y":100,"visible_frame_width":500,"visible_frame_height":400}"#;
    catalogue()
        .prepare(&ToolIntent::new("click", T2_COMPUTER_ACT, valid))
        .unwrap();

    for arguments in [
        valid.replace(r#""point_x":200"#, r#""point_x":600"#),
        valid.replace(
            r#""visible_frame_width":500"#,
            r#""visible_frame_width":901"#,
        ),
        valid.replace(r#""display_id":"display-1""#, r#""display_id":"bad:id""#),
    ] {
        assert_eq!(
            catalogue()
                .prepare(&ToolIntent::new("bad-point", T2_COMPUTER_ACT, arguments))
                .unwrap_err()
                .code(),
            PreparationErrorCode::EffectAccessInvalid
        );
    }
}

#[test]
fn unadmitted_target_and_zero_length_drag_fail_closed() {
    for arguments in [
        r#"{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"other","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"press","node_ref":"node-1"}"#,
        r#"{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"drag","display_id":"display-1","start_x":200,"start_y":200,"end_x":200,"end_y":200,"snapshot_pixel_width":1000,"snapshot_pixel_height":800,"scale_milli":2000,"visible_frame_x":100,"visible_frame_y":100,"visible_frame_width":500,"visible_frame_height":400}"#,
    ] {
        assert_eq!(
            catalogue()
                .prepare(&ToolIntent::new("bad-target", T2_COMPUTER_ACT, arguments))
                .unwrap_err()
                .code(),
            PreparationErrorCode::EffectAccessInvalid
        );
    }
}
