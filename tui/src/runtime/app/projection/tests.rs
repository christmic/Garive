use super::*;
use garive_host_client::HostActivity;

fn block(session: &str, turn: &str, activity: TimelineItem) -> TurnBlock {
    TurnBlock {
        key: TurnBlockKey {
            session_id: session.into(),
            turn_id: turn.into(),
        },
        user: TimelineItem {
            stable_key: format!("turn:{turn}:user"),
            position: 1,
            role: TimelineRole::User,
            tone: TimelineTone::Neutral,
            text: turn.into(),
        },
        activities: vec![activity],
        committed_answer: None,
        outcome: None,
    }
}

fn activity(turn: &str, id: &str, text: &str, position: u64) -> TimelineItem {
    TimelineItem {
        stable_key: format!("activity:{turn}:{id}"),
        position,
        role: TimelineRole::Status,
        tone: TimelineTone::Active,
        text: text.into(),
    }
}

fn snapshot(turn: &str, activity_id: &str, label: &str) -> TurnTimelineItem {
    TurnTimelineItem {
        turn_id: turn.into(),
        started_position: 1,
        latest_position: 3,
        state: "running".into(),
        cancellation_requested: false,
        user_text: "question".into(),
        completion_text: None,
        suspension: None,
        content_truncated: false,
        activities: vec![HostActivity {
            api_version: "garive.host.v1".into(),
            activity_id: activity_id.into(),
            kind: "tool".into(),
            label_key: label.into(),
            state: "running".into(),
            source_position: 2,
            terminal: false,
            safe_code: None,
        }],
    }
}

#[test]
fn interleaved_activity_updates_replace_only_the_exact_turn_child() {
    let mut model = AppModel {
        turn_blocks: vec![
            block("session", "one", activity("one", "shared", "one-old", 2)),
            block("session", "two", activity("two", "shared", "two-old", 4)),
        ],
        ..Default::default()
    };

    assert!(upsert_activity(
        &mut model,
        "session",
        "one",
        activity("one", "shared", "one-new", 5),
    ));

    assert_eq!(model.turn_blocks[0].activities[0].text, "one-new");
    assert_eq!(model.turn_blocks[1].activities[0].text, "two-old");
    assert!(!upsert_activity(
        &mut model,
        "other-session",
        "one",
        activity("one", "shared", "wrong", 6),
    ));
}

#[test]
fn snapshot_install_replaces_the_keyed_block_children() {
    let mut model = AppModel {
        selected_session: Some("session".into()),
        ..Default::default()
    };
    install_timeline(&mut model, vec![snapshot("turn", "old", "activity.read")]);
    assert_eq!(
        model.turn_blocks[0].activities[0].stable_key,
        "activity:turn:old"
    );

    install_timeline(&mut model, vec![snapshot("turn", "new", "activity.write")]);

    assert_eq!(model.turn_blocks.len(), 1);
    assert_eq!(model.turn_blocks[0].key.session_id, "session");
    assert_eq!(model.turn_blocks[0].key.turn_id, "turn");
    assert_eq!(model.turn_blocks[0].activities.len(), 1);
    assert_eq!(
        model.turn_blocks[0].activities[0].stable_key,
        "activity:turn:new"
    );
    assert!(model.durable_child("activity:turn:old").is_none());
}

#[test]
fn h2_restores_accepted_cancellation_until_terminal_truth_arrives() {
    let mut model = AppModel {
        selected_session: Some("session".into()),
        ..Default::default()
    };
    let mut running = snapshot("turn", "activity", "activity.read");
    running.cancellation_requested = true;
    install_timeline(&mut model, vec![running]);
    assert_eq!(
        model.selected_cancel_request().map(|request| request.phase),
        Some(crate::application::CancelRequestPhase::AwaitingTerminal)
    );

    let mut stopped = snapshot("turn", "activity", "activity.read");
    stopped.state = "stopped".into();
    stopped.cancellation_requested = false;
    install_timeline(&mut model, vec![stopped]);
    assert!(model.selected_cancel_request().is_none());
}

#[test]
fn event_and_snapshot_paths_preserve_admitted_tool_semantics() {
    let mut model = AppModel {
        selected_session: Some("session".into()),
        ..Default::default()
    };
    install_timeline(
        &mut model,
        vec![snapshot("turn", "read", "agent.activity.read_file")],
    );
    assert_eq!(model.turn_blocks[0].activities[0].text, "Reading file");

    let mut state = RuntimeState::test_ephemeral(Vec::new());
    state.model = model;
    apply_event(
        HostEvent {
            api_version: "garive.host.v1".into(),
            session_id: "session".into(),
            position: 4,
            event: "agent.activity.completed".into(),
            turn_id: "turn".into(),
            execution_id: "execution".into(),
            text: String::new(),
            activity: Some(HostActivity {
                api_version: "garive.host.v1".into(),
                activity_id: "read".into(),
                kind: "tool".into(),
                label_key: "agent.activity.read_file".into(),
                state: "completed".into(),
                source_position: 4,
                terminal: true,
                safe_code: None,
            }),
        },
        &mut state,
    );
    assert_eq!(state.model.turn_blocks[0].activities[0].text, "Read file");
    assert_eq!(
        state.model.turn_blocks[0].activities[0].tone,
        TimelineTone::Success
    );
}

#[test]
fn every_detached_durable_activity_update_is_counted() {
    let mut model = AppModel::default();
    model.viewport.follow_latest = false;
    note_detached_durable_update(&mut model);
    note_detached_durable_update(&mut model);
    assert_eq!(model.viewport.newer_updates, 2);

    model.follow_latest();
    note_detached_durable_update(&mut model);
    assert_eq!(model.viewport.newer_updates, 0);
}

#[tokio::test]
async fn exact_terminal_event_clears_the_cancel_request_before_snapshot_takeover() {
    let mut state = RuntimeState::test_ephemeral(Vec::new());
    state.model.selected_session = Some("session".into());
    state.model.selected_turn = Some("turn".into());
    state.model.execution = ExecutionState::Following;
    state
        .model
        .cancel_requests
        .begin("cancel".into(), "session".into(), "turn".into());
    state.model.cancel_requests.mark_accepted("cancel");

    apply_event(
        HostEvent {
            api_version: "garive.host.v1".into(),
            session_id: "session".into(),
            position: 2,
            event: "turn.stopped".into(),
            turn_id: "turn".into(),
            execution_id: "execution".into(),
            text: String::new(),
            activity: None,
        },
        &mut state,
    );

    assert!(state.model.selected_cancel_request().is_none());
}
