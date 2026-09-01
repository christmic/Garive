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
    BootState, CancelRequestPhase, ConnectionState, ConversationLandmark, EffectFailure,
    EffectKind, EffectTracker, FocusTarget, HostReadResponse, InspectorVariant, Overlay,
    PendingMutationDraft, PendingMutationKind, PersistedPendingIdentity, PersistenceFailure,
    TerminalSize, TimelineItem, TimelineRole, WIDE_INSPECTOR_MIN_WIDTH,
};
use garive_host_client::SuspensionView;
use serde_json::json;

#[test]
fn boot_transitions_are_explicit_and_complete() {
    let mut model = AppModel::default();
    let effects = reduce(&mut model, AppAction::BootStarted);
    assert_eq!(effects.len(), 2);
    assert_eq!(model.boot, BootState::Loading);
    for effect in effects {
        let outcome = match &effect.kind {
            EffectKind::LoadDefinitions => {
                AppEffectOutcome::HostRead(Ok(HostReadResponse::Definitions(Vec::new())))
            }
            EffectKind::LoadSessionPage { request } => {
                AppEffectOutcome::HostRead(Ok(HostReadResponse::SessionPage {
                    request: request.clone(),
                    sessions: Vec::new(),
                    next_before: None,
                }))
            }
            _ => unreachable!("boot read"),
        };
        reduce(
            &mut model,
            AppAction::EffectFinished(AppEffectResult {
                context: effect.context,
                kind: effect.kind.tag(),
                outcome,
            }),
        );
    }
    assert_eq!(model.boot, BootState::NotConfigured);
    assert_eq!(model.connection, ConnectionState::Online);
    assert_eq!((model.definition_count, model.session_count), (0, 0));
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
    let completed = tracker.take_finished(&result).expect("exact completion");
    let successor = tracker
        .issue_successor(&completed, EffectKind::Exit, None)
        .expect("successor identity");
    assert!(successor.context.effect_id > completed.context.effect_id);
    assert_eq!(successor.context.session_id, completed.context.session_id);
    assert!(tracker.pending.contains_key(&successor.context.effect_id));
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
    assert!(model.effects.pending.is_empty());
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
fn completing_session_a_does_not_unfreeze_session_b_persistence() {
    let mut model = AppModel {
        selected_session: Some("session-a".into()),
        ..Default::default()
    };
    let draft_a = start_turn_draft_for("command-a", "session-a");
    let persist_a = reduce(&mut model, AppAction::StartTurnRequested(draft_a.clone())).remove(0);

    model.selected_session = Some("session-b".into());
    model.composer_is_frozen = false;
    let draft_b = start_turn_draft_for("command-b", "session-b");
    let persist_b = reduce(&mut model, AppAction::StartTurnRequested(draft_b.clone())).remove(0);

    let follow_up = reduce(
        &mut model,
        AppAction::EffectFinished(persisted(&persist_a, &draft_a.command_id)),
    );
    assert!(matches!(
        follow_up.as_slice(),
        [AppEffect {
            kind: EffectKind::StartTurn { .. },
            ..
        }]
    ));
    assert!(model
        .effects
        .pending
        .contains_key(&persist_b.context.effect_id));
    assert!(model.has_pending_command);
    assert!(model.composer_is_frozen);

    let duplicate_b = start_turn_draft_for("command-b-duplicate", "session-b");
    assert!(reduce(&mut model, AppAction::StartTurnRequested(duplicate_b)).is_empty());
}

#[test]
fn failing_one_persistence_keeps_another_context_pending() {
    let mut model = AppModel {
        selected_session: Some("session-a".into()),
        ..Default::default()
    };
    let persist_a = reduce(
        &mut model,
        AppAction::StartTurnRequested(start_turn_draft_for("command-a", "session-a")),
    )
    .remove(0);

    model.selected_session = Some("session-b".into());
    model.composer_is_frozen = false;
    let persist_b = reduce(
        &mut model,
        AppAction::StartTurnRequested(start_turn_draft_for("command-b", "session-b")),
    )
    .remove(0);

    let follow_up = reduce(
        &mut model,
        AppAction::EffectFinished(AppEffectResult {
            context: persist_a.context,
            kind: persist_a.kind.tag(),
            outcome: AppEffectOutcome::PendingPersisted(Err(PersistenceFailure::Unavailable)),
        }),
    );
    assert!(follow_up.is_empty());
    assert!(model
        .effects
        .pending
        .contains_key(&persist_b.context.effect_id));
    assert!(model.has_pending_command);
    assert!(model.composer_is_frozen);
    assert_eq!(model.overlay, Some(Overlay::ErrorDetails));
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
    assert!(model.effects.pending.is_empty());
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

#[test]
fn cancel_requires_exact_following_context_and_persistence() {
    let mut model = AppModel {
        selected_session: Some("session-a".into()),
        selected_turn: Some("turn-a".into()),
        execution: application::ExecutionState::Following,
        ..Default::default()
    };
    let draft = cancel_draft();
    let mut foreign = draft.clone();
    foreign.session_id = Some("session-b".into());
    assert!(reduce(&mut model, AppAction::CancelTurnRequested(foreign)).is_empty());
    let mut malformed = draft.clone();
    malformed.requested_through_position = Some(0);
    assert!(reduce(&mut model, AppAction::CancelTurnRequested(malformed)).is_empty());

    let persist = reduce(&mut model, AppAction::CancelTurnRequested(draft.clone())).remove(0);
    assert_eq!(model.execution, application::ExecutionState::Following);
    assert_eq!(
        model.selected_cancel_request().map(|request| request.phase),
        Some(CancelRequestPhase::Requesting)
    );
    let duplicate = reduce(&mut model, AppAction::CancelTurnRequested(draft.clone()));
    assert!(duplicate.is_empty());
    let follow_up = reduce(
        &mut model,
        AppAction::EffectFinished(persisted(&persist, &draft.command_id)),
    );
    assert!(matches!(
        follow_up.as_slice(),
        [AppEffect {
            kind: EffectKind::CancelTurn { .. },
            ..
        }]
    ));
    assert_eq!(model.execution, application::ExecutionState::Following);
    assert!(model.effects.pending.is_empty());

    let mut failed = AppModel {
        selected_session: Some("session-a".into()),
        selected_turn: Some("turn-a".into()),
        execution: application::ExecutionState::Following,
        ..Default::default()
    };
    let persist = reduce(&mut failed, AppAction::CancelTurnRequested(draft)).remove(0);
    reduce(
        &mut failed,
        AppAction::EffectFinished(AppEffectResult {
            context: persist.context,
            kind: persist.kind.tag(),
            outcome: AppEffectOutcome::PendingPersisted(Err(PersistenceFailure::Unavailable)),
        }),
    );
    assert!(failed.selected_cancel_request().is_none());
}

#[test]
fn continue_blocks_stale_decision_and_persistence_failure() {
    let mut model = suspended_model();
    let draft = continue_draft();
    let mut foreign = draft.clone();
    foreign.session_id = Some("session-b".into());
    assert!(reduce(
        &mut model,
        AppAction::ContinueTurnRequested {
            draft: foreign,
            schema_digest: "schema-a".into(),
        },
    )
    .is_empty());

    let persist = reduce(
        &mut model,
        AppAction::ContinueTurnRequested {
            draft: draft.clone(),
            schema_digest: "schema-a".into(),
        },
    )
    .remove(0);
    model.suspension.as_mut().unwrap().response_schema_digest = Some("schema-b".into());
    let stale = reduce(
        &mut model,
        AppAction::EffectFinished(persisted(&persist, &draft.command_id)),
    );
    assert!(matches!(
        stale.as_slice(),
        [AppEffect {
            kind: EffectKind::ContinueTurn {
                host_allowed: false,
                ..
            },
            ..
        }]
    ));
    assert!(model.effects.pending.is_empty());

    let mut model = suspended_model();
    let exact = reduce(
        &mut model,
        AppAction::ContinueTurnRequested {
            draft: draft.clone(),
            schema_digest: "schema-a".into(),
        },
    )
    .remove(0);
    let follow_up = reduce(
        &mut model,
        AppAction::EffectFinished(persisted(&exact, &draft.command_id)),
    );
    assert!(matches!(
        follow_up.as_slice(),
        [AppEffect {
            kind: EffectKind::ContinueTurn {
                host_allowed: true,
                ..
            },
            ..
        }]
    ));
    assert!(model.effects.pending.is_empty());

    let mut model = suspended_model();
    let persist = reduce(
        &mut model,
        AppAction::ContinueTurnRequested {
            draft,
            schema_digest: "schema-a".into(),
        },
    )
    .remove(0);
    assert!(reduce(
        &mut model,
        AppAction::EffectFinished(AppEffectResult {
            context: persist.context,
            kind: persist.kind.tag(),
            outcome: AppEffectOutcome::PendingPersisted(Err(PersistenceFailure::Unavailable)),
        }),
    )
    .is_empty());
    assert_eq!(model.overlay, Some(Overlay::ErrorDetails));
    assert!(!model.composer_is_frozen);
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

fn cancel_draft() -> PendingMutationDraft {
    PendingMutationDraft {
        command_id: "command-cancel".into(),
        kind: PendingMutationKind::CancelTurn,
        session_id: Some("session-a".into()),
        turn_id: Some("turn-a".into()),
        suspension_id: None,
        expected_session_version: None,
        requested_through_position: Some(7),
        request_payload: json!({"session_id": "session-a", "requested_through_position": 7}),
        created_at: "2026-09-01T00:00:00Z".into(),
    }
}

fn continue_draft() -> PendingMutationDraft {
    PendingMutationDraft {
        command_id: "command-continue".into(),
        kind: PendingMutationKind::ContinueTurn,
        session_id: Some("session-a".into()),
        turn_id: Some("turn-a".into()),
        suspension_id: Some("suspension-a".into()),
        expected_session_version: Some(4),
        requested_through_position: None,
        request_payload: json!({"input_json": true}),
        created_at: "2026-09-01T00:00:00Z".into(),
    }
}

fn suspended_model() -> AppModel {
    AppModel {
        selected_session: Some("session-a".into()),
        selected_turn: Some("turn-a".into()),
        execution: application::ExecutionState::Suspended,
        suspension: Some(SuspensionView {
            suspension_id: "suspension-a".into(),
            session_version: 4,
            kind: "approval_required".into(),
            prompt_schema: "prompt".into(),
            prompt_json: "{}".into(),
            prompt_digest: "prompt-digest".into(),
            response_schema_json: Some(r#"{"type":"boolean"}"#.into()),
            response_schema_digest: Some("schema-a".into()),
        }),
        ..Default::default()
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
    start_turn_draft_for("command-start", "session-a")
}

fn start_turn_draft_for(command_id: &str, session_id: &str) -> PendingMutationDraft {
    PendingMutationDraft {
        command_id: command_id.into(),
        kind: PendingMutationKind::StartTurn,
        session_id: Some(session_id.into()),
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
            width: WIDE_INSPECTOR_MIN_WIDTH,
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
            width: WIDE_INSPECTOR_MIN_WIDTH - 1,
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
            width: WIDE_INSPECTOR_MIN_WIDTH,
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
