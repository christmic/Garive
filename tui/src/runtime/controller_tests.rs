use garive_host_client::SuspensionView;

use super::*;

#[test]
fn overlay_escape_never_falls_through_to_running_turn_cancel() {
    for (overlay, expected) in [
        (Overlay::Help, None),
        (Overlay::UnknownCommand, Some(Overlay::UnknownCommand)),
    ] {
        let mut state = RuntimeState::test_ephemeral(Vec::new());
        state.model.execution = ExecutionState::Following;
        state.model.selected_session = Some("session".into());
        state.model.selected_turn = Some("turn".into());
        state.model.observed_position = 1;
        state.model.overlay = Some(overlay);
        state.model.composer.replace("retained draft").unwrap();

        handle_terminal(
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            &mut state,
        );

        assert_eq!(state.model.overlay, expected);
        assert!(state.deferred_ephemeral.is_none());
        assert_eq!(state.model.composer.text(), "retained draft");
    }
}

#[test]
fn help_overlay_owns_cancel_input_during_a_running_turn() {
    let mut state = RuntimeState::test_ephemeral(Vec::new());
    state.model.execution = ExecutionState::Following;
    state.model.selected_session = Some("session".into());
    state.model.selected_turn = Some("turn".into());
    state.model.observed_position = 1;
    state.model.overlay = Some(Overlay::Help);

    handle_terminal(
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        &mut state,
    );

    assert_eq!(state.model.overlay, Some(Overlay::Help));
    assert!(state.deferred_ephemeral.is_none());
}

#[test]
fn redraw_preempts_overlay_input_without_closing_it() {
    let mut state = RuntimeState::test_ephemeral(Vec::new());
    state.model.overlay = Some(Overlay::Help);

    handle_terminal(
        Event::Key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL)),
        &mut state,
    );

    assert!(state.force_redraw);
    assert_eq!(state.model.overlay, Some(Overlay::Help));
}

#[test]
fn non_suspension_overlays_consume_paste_without_mutating_the_composer() {
    for overlay in [
        Overlay::CommandPalette,
        Overlay::Help,
        Overlay::SessionPicker,
    ] {
        let mut state = RuntimeState::test_ephemeral(Vec::new());
        state.model.composer.replace("retained draft").unwrap();
        state.model.overlay = Some(overlay);

        handle_terminal(Event::Paste("hidden paste".into()), &mut state);

        assert_eq!(state.model.composer.text(), "retained draft", "{overlay:?}");
    }
}

#[test]
fn suspension_overlay_keeps_paste_in_its_response_editor() {
    let mut state = RuntimeState::test_ephemeral(Vec::new());
    state.model.composer.replace("retained draft").unwrap();
    state.model.overlay = Some(Overlay::Suspension);
    state.model.selected_session = Some("session".into());
    state.model.selected_turn = Some("turn".into());
    state.model.suspension = Some(SuspensionView {
        suspension_id: "suspension".into(),
        session_version: 1,
        kind: "external_input_required".into(),
        prompt_schema: "garive.public-suspension-prompt.v1".into(),
        prompt_json: r#"{"schema_version":1,"title_key":"title","message_text":"Respond.","action_label_key":"submit"}"#.into(),
        prompt_digest: "0".repeat(64),
        response_schema_json: Some(r#"{"type":"string"}"#.into()),
        response_schema_digest: Some("1".repeat(64)),
    });
    state.model.reconcile_suspension_response();

    handle_terminal(Event::Paste("response paste".into()), &mut state);

    assert_eq!(state.model.composer.text(), "retained draft");
    assert_eq!(
        state
            .model
            .suspension_response
            .as_ref()
            .unwrap()
            .editor
            .text(),
        "response paste"
    );
}
