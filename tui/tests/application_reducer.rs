#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::{MouseMode, Theme};
#[path = "../src/application/mod.rs"]
mod application;
#[path = "../src/input/mod.rs"]
mod input;

use application::{
    reduce, AppAction, AppEffect, AppEffectOutcome, AppEffectResult, AppGeneration, AppModel,
    BootState, ConnectionState, ConversationLandmark, EffectFailure, EffectKind, EffectTracker,
    FocusTarget, InspectorVariant, Overlay, PendingMutationDraft, PendingMutationKind,
    PersistedPendingIdentity, PersistenceFailure, TerminalSize, TimelineItem, TimelineRole,
};
use serde_json::json;

#[test]
fn boot_transitions_are_explicit_and_complete() {
    let mut model = AppModel::default();
    assert!(reduce(&mut model, AppAction::BootStarted).is_empty());
    assert_eq!(model.boot, BootState::Loading);
    reduce(
        &mut model,
        AppAction::BootCompleted {
            definition_count: 2,
            session_count: 3,
        },
    );
    assert_eq!(model.boot, BootState::Ready);
    assert_eq!(model.connection, ConnectionState::Online);
    assert_eq!((model.definition_count, model.session_count), (2, 3));
}

#[test]
fn unavailable_host_has_a_safe_public_code() {
    let mut model = AppModel::default();
    reduce(
        &mut model,
        AppAction::HostUnavailable {
            safe_code: "protocol_error",
        },
    );
    assert_eq!(model.boot, BootState::Degraded);
    assert_eq!(
        model.connection,
        ConnectionState::Unavailable {
            safe_code: "protocol_error"
        }
    );
}

#[test]
fn switching_sessions_restores_a_bounded_independent_viewport() {
    let mut model = AppModel {
        selected_session: Some("session-a".into()),
        ..Default::default()
    };
    model.viewport.follow_latest = false;
    model.viewport.anchor_key = Some("a-anchor".into());
    model.switch_viewport("session-b");
    model.selected_session = Some("session-b".into());
    assert!(model.viewport.follow_latest);
    model.viewport.follow_latest = false;
    model.viewport.anchor_key = Some("b-anchor".into());

    model.switch_viewport("session-a");
    assert_eq!(model.viewport.anchor_key.as_deref(), Some("a-anchor"));
    assert!(!model.viewport.follow_latest);

    let mut bounded = AppModel::default();
    for index in 0..80 {
        let session = format!("session-{index}");
        bounded.switch_viewport(&session);
        bounded.selected_session = Some(session);
    }
    assert!(bounded.session_viewports.len() <= 64);
    assert!(bounded.viewport_order.len() <= 64);
}

#[test]
fn blocking_overlays_own_focus_and_quit_requires_confirmation() {
    let mut model = AppModel::default();
    reduce(
        &mut model,
        AppAction::FocusChanged(FocusTarget::Conversation),
    );
    reduce(&mut model, AppAction::OverlayOpened(Overlay::Suspension));
    reduce(&mut model, AppAction::OverlayClosed);
    reduce(&mut model, AppAction::QuitRequested);
    assert_eq!(model.overlay, Some(Overlay::Suspension));
    assert!(reduce(&mut model, AppAction::QuitConfirmed).is_empty());

    model.overlay = None;
    reduce(&mut model, AppAction::QuitRequested);
    let exit = reduce(&mut model, AppAction::QuitConfirmed);
    assert!(model.quit_requested);
    assert_eq!(exit.len(), 1);
}

#[test]
fn effects_have_monotonic_identity_and_exact_result_correlation() {
    let mut model = AppModel {
        overlay: Some(Overlay::QuitConfirmation),
        ..Default::default()
    };
    let first = reduce(&mut model, AppAction::QuitConfirmed).remove(0);
    model.overlay = Some(Overlay::QuitConfirmation);
    let second = reduce(&mut model, AppAction::QuitConfirmed).remove(0);
    assert!(first.context.effect_id < second.context.effect_id);
    assert_eq!(first.context.issued_generation, AppGeneration::initial());

    let mut stale = completed(&first);
    stale.context.issued_generation = AppGeneration(stale.context.issued_generation.0 + 1);
    reduce(&mut model, AppAction::EffectFinished(stale));
    assert!(model.effects.pending.contains_key(&first.context.effect_id));

    let mut foreign = completed(&first);
    foreign.context.session_id = Some("other-session".into());
    reduce(&mut model, AppAction::EffectFinished(foreign));
    assert!(model.effects.pending.contains_key(&first.context.effect_id));

    reduce(&mut model, AppAction::EffectFinished(completed(&first)));
    assert!(!model.effects.pending.contains_key(&first.context.effect_id));
    assert!(model
        .effects
        .pending
        .contains_key(&second.context.effect_id));

    reduce(
        &mut model,
        AppAction::EffectFinished(AppEffectResult {
            context: second.context.clone(),
            kind: second.kind.tag(),
            outcome: AppEffectOutcome::Failed(EffectFailure::Internal),
        }),
    );
    assert!(!model
        .effects
        .pending
        .contains_key(&second.context.effect_id));
}

#[test]
fn pending_mutation_contract_redacts_payload_and_correlates_sealed_identity() {
    let draft = PendingMutationDraft {
        command_id: "command-a".into(),
        kind: PendingMutationKind::StartTurn,
        session_id: Some("session-a".into()),
        turn_id: None,
        suspension_id: None,
        expected_session_version: Some(4),
        requested_through_position: None,
        request_payload: json!({"text": "private prompt"}),
        created_at: "2026-09-01T00:00:00Z".into(),
    };
    let debug = format!("{draft:?}");
    assert!(!debug.contains("private prompt"));
    assert!(!debug.contains("request_payload"));

    let mut tracker = EffectTracker::default();
    let effect = tracker
        .issue(
            EffectKind::PersistPending { draft },
            Some("session-a".into()),
            None,
        )
        .unwrap();
    let result = AppEffectResult {
        context: effect.context.clone(),
        kind: effect.kind.tag(),
        outcome: AppEffectOutcome::PendingPersisted(Ok(PersistedPendingIdentity {
            command_id: "command-a".into(),
            request_digest: "a".repeat(64),
        })),
    };
    assert!(tracker.take_finished(&result).is_some());
}

#[test]
fn start_turn_waits_for_exact_persistence_result_before_host_effect() {
    let mut model = AppModel {
        selected_session: Some("session-a".into()),
        ..Default::default()
    };
    let draft = PendingMutationDraft {
        command_id: "command-start".into(),
        kind: PendingMutationKind::StartTurn,
        session_id: Some("session-a".into()),
        turn_id: None,
        suspension_id: None,
        expected_session_version: None,
        requested_through_position: None,
        request_payload: json!({"text": "hello"}),
        created_at: "2026-09-01T00:00:00Z".into(),
    };
    let persist = reduce(&mut model, AppAction::StartTurnRequested(draft.clone()))
        .pop()
        .expect("persistence effect");
    assert!(matches!(persist.kind, EffectKind::PersistPending { .. }));
    assert!(model.composer_is_frozen);

    let mut stale = AppEffectResult {
        context: persist.context.clone(),
        kind: persist.kind.tag(),
        outcome: AppEffectOutcome::PendingPersisted(Ok(PersistedPendingIdentity {
            command_id: draft.command_id.clone(),
            request_digest: "a".repeat(64),
        })),
    };
    stale.context.session_id = Some("session-b".into());
    assert!(reduce(&mut model, AppAction::EffectFinished(stale)).is_empty());

    let follow_up = reduce(
        &mut model,
        AppAction::EffectFinished(AppEffectResult {
            context: persist.context,
            kind: persist.kind.tag(),
            outcome: AppEffectOutcome::PendingPersisted(Ok(PersistedPendingIdentity {
                command_id: draft.command_id,
                request_digest: "a".repeat(64),
            })),
        }),
    );
    assert!(matches!(
        follow_up.as_slice(),
        [AppEffect {
            kind: EffectKind::StartTurn { .. },
            ..
        }]
    ));
    assert_eq!(follow_up[0].context.request_digest, Some("a".repeat(64)));
}

#[test]
fn start_turn_persistence_failure_unfreezes_without_host_effect() {
    let mut model = AppModel {
        selected_session: Some("session-a".into()),
        ..Default::default()
    };
    let mut draft = start_turn_draft();
    draft.session_id = model.selected_session.clone();
    let persist = reduce(&mut model, AppAction::StartTurnRequested(draft)).remove(0);
    let follow_up = reduce(
        &mut model,
        AppAction::EffectFinished(AppEffectResult {
            context: persist.context,
            kind: persist.kind.tag(),
            outcome: AppEffectOutcome::PendingPersisted(Err(PersistenceFailure::Unavailable)),
        }),
    );
    assert!(follow_up.is_empty());
    assert!(!model.composer_is_frozen);
    assert!(!model.has_pending_command);
}

#[test]
fn create_session_rejects_malformed_and_waits_for_exact_persistence() {
    let mut model = AppModel::default();
    let mut malformed = create_session_draft();
    malformed.session_id = Some("must-be-absent".into());
    assert!(reduce(&mut model, AppAction::CreateSessionRequested(malformed)).is_empty());
    let mut malformed = create_session_draft();
    malformed.request_payload = json!({});
    assert!(reduce(&mut model, AppAction::CreateSessionRequested(malformed)).is_empty());

    let draft = create_session_draft();
    let persist = reduce(&mut model, AppAction::CreateSessionRequested(draft.clone())).remove(0);
    let mut stale = persisted(&persist, &draft.command_id);
    stale.context.issued_generation = AppGeneration(stale.context.issued_generation.0 + 1);
    assert!(reduce(&mut model, AppAction::EffectFinished(stale)).is_empty());

    let follow_up = reduce(
        &mut model,
        AppAction::EffectFinished(persisted(&persist, &draft.command_id)),
    );
    assert!(matches!(
        follow_up.as_slice(),
        [AppEffect {
            kind: EffectKind::CreateSession { .. },
            ..
        }]
    ));
}

#[test]
fn create_session_persistence_failure_recovers_without_host_effect() {
    let mut model = AppModel::default();
    let persist = reduce(
        &mut model,
        AppAction::CreateSessionRequested(create_session_draft()),
    )
    .remove(0);
    let follow_up = reduce(
        &mut model,
        AppAction::EffectFinished(AppEffectResult {
            context: persist.context,
            kind: persist.kind.tag(),
            outcome: AppEffectOutcome::PendingPersisted(Err(PersistenceFailure::Unavailable)),
        }),
    );
    assert!(follow_up.is_empty());
    assert!(!model.composer_is_frozen);
    assert!(!model.has_pending_command);
    assert_eq!(model.overlay, Some(Overlay::ErrorDetails));
}

fn create_session_draft() -> PendingMutationDraft {
    PendingMutationDraft {
        command_id: "command-create".into(),
        kind: PendingMutationKind::CreateSession,
        session_id: None,
        turn_id: None,
        suspension_id: None,
        expected_session_version: None,
        requested_through_position: None,
        request_payload: json!({"agent_definition_id": "definition-main"}),
        created_at: "2026-09-01T00:00:00Z".into(),
    }
}

fn persisted(effect: &AppEffect, command_id: &str) -> AppEffectResult {
    AppEffectResult {
        context: effect.context.clone(),
        kind: effect.kind.tag(),
        outcome: AppEffectOutcome::PendingPersisted(Ok(PersistedPendingIdentity {
            command_id: command_id.into(),
            request_digest: "b".repeat(64),
        })),
    }
}

#[test]
fn malformed_start_turn_never_reaches_persistence() {
    let mut model = AppModel::default();
    let mut missing_session = start_turn_draft();
    missing_session.session_id = None;
    assert!(reduce(&mut model, AppAction::StartTurnRequested(missing_session)).is_empty());

    model.selected_session = Some("session-a".into());
    for payload in [json!({}), json!({"text": 3}), json!({"text": "  "})] {
        let mut malformed = start_turn_draft();
        malformed.request_payload = payload;
        assert!(reduce(&mut model, AppAction::StartTurnRequested(malformed)).is_empty());
    }
    assert!(!model.composer_is_frozen);
    assert!(!model.has_pending_command);
}

fn start_turn_draft() -> PendingMutationDraft {
    PendingMutationDraft {
        command_id: "command-start".into(),
        kind: PendingMutationKind::StartTurn,
        session_id: Some("session-a".into()),
        turn_id: None,
        suspension_id: None,
        expected_session_version: None,
        requested_through_position: None,
        request_payload: json!({"text": "hello"}),
        created_at: "2026-09-01T00:00:00Z".into(),
    }
}

fn completed(effect: &AppEffect) -> AppEffectResult {
    AppEffectResult {
        context: effect.context.clone(),
        kind: effect.kind.tag(),
        outcome: AppEffectOutcome::Completed,
    }
}

#[test]
fn every_terminal_size_is_representable_without_underflow() {
    let mut model = AppModel::default();
    for size in [
        TerminalSize {
            width: 0,
            height: 0,
        },
        TerminalSize {
            width: 19,
            height: 7,
        },
        TerminalSize {
            width: 20,
            height: 8,
        },
        TerminalSize {
            width: u16::MAX,
            height: u16::MAX,
        },
    ] {
        reduce(&mut model, AppAction::TerminalResized(size));
        assert_eq!(
            model.terminal_size.is_supported(),
            size.width >= 20 && size.height >= 8
        );
    }
}

#[test]
fn inspector_restores_its_surface_and_selection_across_resize_and_overlay_stack() {
    let mut model = AppModel {
        terminal_size: TerminalSize {
            width: 120,
            height: 24,
        },
        ..Default::default()
    };
    model.open_inspector(InspectorVariant::Details);
    model.select_inspector_index(2);
    let selected = model.inspector.selected_key.clone();

    reduce(
        &mut model,
        AppAction::TerminalResized(TerminalSize {
            width: 39,
            height: 24,
        }),
    );
    assert_eq!(model.overlay, None);
    reduce(
        &mut model,
        AppAction::TerminalResized(TerminalSize {
            width: 119,
            height: 24,
        }),
    );
    assert_eq!(
        (model.overlay, model.focus),
        (Some(Overlay::Inspector), FocusTarget::Overlay)
    );

    reduce(&mut model, AppAction::OverlayOpened(Overlay::Help));
    reduce(&mut model, AppAction::OverlayClosed);
    assert_eq!(model.overlay, Some(Overlay::Inspector));
    reduce(
        &mut model,
        AppAction::TerminalResized(TerminalSize {
            width: 120,
            height: 24,
        }),
    );
    assert_eq!((model.overlay, model.focus), (None, FocusTarget::Inspector));
    assert_eq!(model.inspector.selected_key, selected);
}

#[test]
fn turn_navigation_uses_public_positions_and_escape_or_focus_loss_is_non_mutating() {
    let mut model = AppModel {
        focus: FocusTarget::Conversation,
        prior_focus: FocusTarget::Conversation,
        overlay: Some(Overlay::TurnNavigator),
        turn_filter: "beta".into(),
        conversation_landmarks: vec![landmark(1, 10, "alpha"), landmark(2, 20, "beta")],
        ..Default::default()
    };
    model.push_test_timeline_item(cell("alpha", 10));
    model.push_test_timeline_item(cell("beta", 20));
    model.viewport.follow_latest = false;
    model.viewport.anchor_key = Some("alpha".into());
    let original = model.viewport.clone();

    reduce(&mut model, AppAction::OverlayClosed);
    assert_eq!(model.viewport, original);
    assert!(model.turn_filter.is_empty());
    assert_eq!(model.overlay, None);

    assert!(model.jump_to_turn_position(20));
    assert!(model.viewport.follow_latest);
    model.overlay = Some(Overlay::TurnNavigator);
    model.turn_filter = "stale".into();
    reduce(&mut model, AppAction::TerminalFocusChanged(false));
    assert_eq!(model.overlay, None);
    assert!(model.turn_filter.is_empty());
}

fn landmark(ordinal: usize, started_position: u64, prompt: &str) -> ConversationLandmark {
    ConversationLandmark {
        ordinal,
        started_position,
        prompt_preview: prompt.into(),
    }
}

fn cell(key: &str, position: u64) -> TimelineItem {
    TimelineItem {
        stable_key: key.into(),
        position,
        role: TimelineRole::User,
        tone: Default::default(),
        text: key.into(),
    }
}
