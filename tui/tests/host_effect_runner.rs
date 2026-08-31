#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::{MouseMode, Theme};
#[path = "../src/application/mod.rs"]
mod application;
#[path = "../src/host.rs"]
mod host;
#[path = "../src/runtime/host_effects.rs"]
mod host_effects;
#[path = "../src/input/mod.rs"]
mod input;

use std::{future::Future, pin::Pin};

use application::{
    reduce, AppAction, AppEffect, AppEffectOutcome, AppEffectResult, AppGeneration, AppModel,
    EffectFailure, EffectKind, EffectTracker, HostReadFailure, HostReadResponse,
    SessionPagePurpose, SessionPageRequest, SnapshotRead, SnapshotRequest,
};
use garive_host_client::{
    AgentDefinitionSummary, HostClientErrorCode, SessionSummary, SessionView,
};
use host::{HostReadFuture, HostReadPort};
use host_effects::HostEffectRunner;
use tokio::sync::mpsc;

#[derive(Clone, Copy)]
enum Mode {
    Success,
    Failure,
    Panic,
}

#[derive(Clone)]
struct FakeHostReadPort(Mode);

impl HostReadPort for FakeHostReadPort {
    fn load_definitions(&self) -> HostReadFuture {
        let mode = self.0;
        Box::pin(async move {
            match mode {
                Mode::Success => Ok(HostReadResponse::Definitions(vec![definition()])),
                Mode::Failure => Err(HostReadFailure {
                    code: HostClientErrorCode::TransportFailure,
                    host_rejected: false,
                }),
                Mode::Panic => panic!("injected Host read panic"),
            }
        })
    }

    fn load_session_page(&self, request: SessionPageRequest) -> HostReadFuture {
        let mode = self.0;
        Box::pin(async move {
            match mode {
                Mode::Success => Ok(HostReadResponse::SessionPage {
                    request,
                    sessions: vec![session("private-session")],
                    next_before: Some("private-next-cursor".into()),
                }),
                Mode::Failure => Err(HostReadFailure {
                    code: HostClientErrorCode::TransportFailure,
                    host_rejected: false,
                }),
                Mode::Panic => panic!("injected Host page panic"),
            }
        })
    }

    fn load_snapshot(&self, request: SnapshotRequest) -> HostReadFuture {
        let mode = self.0;
        Box::pin(async move {
            match mode {
                Mode::Success => Ok(HostReadResponse::Snapshot(Box::new(SnapshotRead {
                    view: SessionView {
                        api_version: "v1".into(),
                        session: session(&request.session_id),
                        observed_max_position: 7,
                    },
                    request,
                    items: Vec::new(),
                    follow_position: 7,
                }))),
                Mode::Failure => Err(HostReadFailure {
                    code: HostClientErrorCode::TransportFailure,
                    host_rejected: false,
                }),
                Mode::Panic => panic!("injected Host snapshot panic: {request:?}"),
            }
        })
    }
}

#[tokio::test]
async fn snapshot_runner_preserves_exact_redacted_request_and_failure_identity() {
    let request = SnapshotRequest {
        session_id: "private-snapshot-session".into(),
    };
    let effect = EffectTracker::default()
        .issue(
            EffectKind::LoadSnapshot {
                request: request.clone(),
            },
            Some(request.session_id.clone()),
            Some("snapshot-request-digest".into()),
        )
        .expect("snapshot effect identity");
    let success = execute(effect, Mode::Success).await;
    assert!(matches!(
        success.outcome,
        AppEffectOutcome::HostRead(Ok(HostReadResponse::Snapshot(ref snapshot)))
            if snapshot.follow_position == 7
    ));
    let debug = format!("{success:?}");
    assert!(!debug.contains("private-snapshot-session"));

    let failure_effect = EffectTracker::default()
        .issue(
            EffectKind::LoadSnapshot {
                request: request.clone(),
            },
            Some(request.session_id.clone()),
            Some("snapshot-request-digest".into()),
        )
        .expect("snapshot effect identity");
    assert!(matches!(
        execute(failure_effect, Mode::Failure).await.outcome,
        AppEffectOutcome::HostRead(Err(HostReadFailure {
            code: HostClientErrorCode::TransportFailure,
            host_rejected: false,
        }))
    ));

    let panic_effect = EffectTracker::default()
        .issue(
            EffectKind::LoadSnapshot {
                request: request.clone(),
            },
            Some(request.session_id.clone()),
            Some("snapshot-request-digest".into()),
        )
        .expect("snapshot effect identity");
    assert_eq!(
        execute(panic_effect, Mode::Panic).await.outcome,
        AppEffectOutcome::Failed(EffectFailure::Internal)
    );
}

#[tokio::test]
async fn one_shot_read_reports_redacted_success_failure_and_panic() {
    let success = run(Mode::Success).await;
    assert!(matches!(
        success.outcome,
        AppEffectOutcome::HostRead(Ok(HostReadResponse::Definitions(ref values)))
            if values.len() == 1
    ));
    let debug = format!("{success:?}");
    assert!(debug.contains("count: 1"));
    assert!(!debug.contains("private-definition"));
    assert!(!debug.contains("private-capability"));

    assert!(matches!(
        run(Mode::Failure).await.outcome,
        AppEffectOutcome::HostRead(Err(HostReadFailure {
            code: HostClientErrorCode::TransportFailure,
            host_rejected: false,
        }))
    ));
    assert_eq!(
        run(Mode::Panic).await.outcome,
        AppEffectOutcome::Failed(EffectFailure::Internal)
    );
}

#[tokio::test]
async fn reducer_accepts_only_the_exact_correlated_read_result() {
    let mut model = AppModel::default();
    let effect = model
        .effects
        .issue(EffectKind::LoadDefinitions, None, None)
        .expect("effect identity");
    let result = execute(effect.clone(), Mode::Success).await;

    let mut stale = result.clone();
    stale.context.issued_generation = AppGeneration(stale.context.issued_generation.0 + 1);
    assert!(reduce(&mut model, AppAction::EffectFinished(stale)).is_empty());
    assert!(model
        .effects
        .pending
        .contains_key(&effect.context.effect_id));

    let mut foreign = result.clone();
    foreign.context.session_id = Some("foreign-session".into());
    assert!(reduce(&mut model, AppAction::EffectFinished(foreign)).is_empty());
    assert!(model
        .effects
        .pending
        .contains_key(&effect.context.effect_id));

    assert!(reduce(&mut model, AppAction::EffectFinished(result)).is_empty());
    assert!(!model
        .effects
        .pending
        .contains_key(&effect.context.effect_id));
}

#[tokio::test]
async fn session_page_runner_reports_redacted_success_failure_and_panic() {
    let request = page_request(SessionPagePurpose::Append, Some("private-page-cursor"));
    let success = execute_page(request.clone(), Mode::Success).await;
    assert!(matches!(
        success.outcome,
        AppEffectOutcome::HostRead(Ok(HostReadResponse::SessionPage {
            request: ref returned,
            ref sessions,
            ref next_before,
        })) if returned == &request && sessions.len() == 1 && next_before.is_some()
    ));
    let debug = format!("{success:?}");
    assert!(debug.contains("count: 1"));
    assert!(!debug.contains("private-page-cursor"));
    assert!(!debug.contains("private-session"));
    assert!(!debug.contains("private-next-cursor"));

    assert!(matches!(
        execute_page(request.clone(), Mode::Failure).await.outcome,
        AppEffectOutcome::HostRead(Err(HostReadFailure {
            code: HostClientErrorCode::TransportFailure,
            host_rejected: false,
        }))
    ));
    assert_eq!(
        execute_page(request, Mode::Panic).await.outcome,
        AppEffectOutcome::Failed(EffectFailure::Internal)
    );
}

#[test]
fn page_owner_rejects_catalog_and_purpose_stale_results_and_applies_once() {
    let mut model = AppModel::default();
    replace_catalog(&mut model, vec![session("initial")], Some("cursor-a"));
    let stale_after_refresh =
        request_page(&mut model, SessionPagePurpose::Append, Some("cursor-a"));

    replace_catalog(&mut model, vec![session("refreshed")], Some("cursor-b"));
    assert!(!model.sessions_loading);
    reduce(
        &mut model,
        AppAction::EffectFinished(page_success(
            &stale_after_refresh,
            vec![session("stale-after-refresh")],
            Some("stale-next"),
        )),
    );
    assert_eq!(session_ids(&model), ["refreshed"]);
    assert_eq!(model.sessions_next_before.as_deref(), Some("cursor-b"));

    let stale_append = request_page(&mut model, SessionPagePurpose::Append, Some("cursor-b"));
    let replace = request_page(&mut model, SessionPagePurpose::CatalogRefresh, None);
    reduce(
        &mut model,
        AppAction::EffectFinished(page_success(
            &stale_append,
            vec![session("stale-append")],
            Some("stale-next"),
        )),
    );
    assert!(model.sessions_loading);
    assert_eq!(session_ids(&model), ["refreshed"]);

    let replace_result = page_success(
        &replace,
        vec![session("replacement"), session("replacement")],
        Some("cursor-c"),
    );
    reduce(
        &mut model,
        AppAction::EffectFinished(replace_result.clone()),
    );
    assert!(!model.sessions_loading);
    assert_eq!(session_ids(&model), ["replacement"]);
    assert_eq!(model.sessions_next_before.as_deref(), Some("cursor-c"));
    reduce(&mut model, AppAction::EffectFinished(replace_result));
    assert_eq!(session_ids(&model), ["replacement"]);

    let append = request_page(&mut model, SessionPagePurpose::Append, Some("cursor-c"));
    reduce(
        &mut model,
        AppAction::EffectFinished(page_success(
            &append,
            vec![session("replacement"), session("appended")],
            None,
        )),
    );
    assert_eq!(session_ids(&model), ["replacement", "appended"]);
    assert_eq!(model.sessions_next_before, None);
    assert_eq!(model.session_count, 2);
}

#[test]
fn page_result_rejects_generation_and_request_mismatch() {
    let mut model = AppModel::default();
    replace_catalog(&mut model, vec![session("initial")], Some("cursor-a"));
    let effect = request_page(&mut model, SessionPagePurpose::Append, Some("cursor-a"));
    let mut wrong_generation = page_success(&effect, vec![session("wrong-generation")], None);
    wrong_generation.context.issued_generation =
        AppGeneration(wrong_generation.context.issued_generation.0 + 1);
    reduce(&mut model, AppAction::EffectFinished(wrong_generation));
    assert!(model.sessions_loading);
    assert!(model
        .effects
        .pending
        .contains_key(&effect.context.effect_id));

    let mut wrong_cursor = page_success(&effect, vec![session("wrong-cursor")], None);
    let AppEffectOutcome::HostRead(Ok(HostReadResponse::SessionPage { request, .. })) =
        &mut wrong_cursor.outcome
    else {
        unreachable!("page result")
    };
    request.cursor = Some("foreign-cursor".into());
    reduce(&mut model, AppAction::EffectFinished(wrong_cursor));
    assert!(!model.sessions_loading);
    assert_eq!(session_ids(&model), ["initial"]);
    assert_eq!(model.sessions_next_before.as_deref(), Some("cursor-a"));
    assert_eq!(
        model.notice.as_deref(),
        Some("Ignored an invalid Session page response.")
    );
}

#[test]
fn page_failure_and_panic_only_release_the_active_owner() {
    let mut model = AppModel::default();
    replace_catalog(&mut model, vec![session("initial")], Some("cursor-a"));
    let stale = request_page(&mut model, SessionPagePurpose::Append, Some("cursor-a"));
    let active = request_page(&mut model, SessionPagePurpose::CatalogRefresh, None);
    reduce(
        &mut model,
        AppAction::EffectFinished(page_failure(&stale, false)),
    );
    assert!(model.sessions_loading);
    assert_eq!(model.notice, None);

    reduce(
        &mut model,
        AppAction::EffectFinished(page_failure(&active, false)),
    );
    assert!(!model.sessions_loading);
    assert_eq!(
        model.notice.as_deref(),
        Some("Session page unavailable: transport_failure.")
    );

    let panic = request_page(&mut model, SessionPagePurpose::CatalogRefresh, None);
    reduce(
        &mut model,
        AppAction::EffectFinished(page_failure(&panic, true)),
    );
    assert!(!model.sessions_loading);
    assert_eq!(
        model.notice.as_deref(),
        Some("Ignored an invalid Session page response.")
    );
}

#[test]
fn snapshot_owner_requires_exact_session_generation_digest_and_applies_once() {
    let mut model = AppModel {
        selected_session: Some("session-a".into()),
        ..Default::default()
    };
    let stale = request_snapshot(&mut model, "session-a");
    let active = request_snapshot(&mut model, "session-a");
    reduce(
        &mut model,
        AppAction::EffectFinished(snapshot_success(&stale, "session-a", 11)),
    );
    assert!(model.snapshot_handoff.is_none());

    for mutate in ["generation", "digest", "session"] {
        let mut result = snapshot_success(&active, "session-a", 12);
        match mutate {
            "generation" => {
                result.context.issued_generation =
                    AppGeneration(result.context.issued_generation.0 + 1)
            }
            "digest" => result.context.request_digest = Some("foreign-digest".into()),
            "session" => result.context.session_id = Some("session-b".into()),
            _ => unreachable!(),
        }
        reduce(&mut model, AppAction::EffectFinished(result));
        assert!(model.snapshot_handoff.is_none());
        assert!(model.snapshot_owner.is_some());
    }

    let success = snapshot_success(&active, "session-a", 12);
    reduce(&mut model, AppAction::EffectFinished(success.clone()));
    assert_eq!(model.snapshot_completion_revision, 1);
    assert_eq!(model.snapshot_handoff.as_ref().unwrap().follow_position, 12);
    reduce(&mut model, AppAction::EffectFinished(success));
    assert_eq!(model.snapshot_completion_revision, 1);

    let foreign = reduce(
        &mut model,
        AppAction::LoadSnapshotRequested(SnapshotRequest {
            session_id: "session-b".into(),
        }),
    );
    assert!(foreign.is_empty());

    let selection_bound = request_snapshot(&mut model, "session-a");
    model.selected_session = Some("session-b".into());
    reduce(
        &mut model,
        AppAction::EffectFinished(snapshot_success(&selection_bound, "session-a", 13)),
    );
    assert_eq!(model.snapshot_completion_revision, 1);
    assert_eq!(model.snapshot_handoff.as_ref().unwrap().follow_position, 12);
    assert!(model.snapshot_owner.is_some());
}

#[test]
fn snapshot_failure_releases_only_exact_owner_and_preserves_error_classification() {
    for (code, rejected, expected_connection, failed) in [
        (
            HostClientErrorCode::InvalidEvent,
            false,
            application::ConnectionState::Unavailable {
                safe_code: "invalid_event",
            },
            true,
        ),
        (
            HostClientErrorCode::HostFailure,
            true,
            application::ConnectionState::Online,
            false,
        ),
        (
            HostClientErrorCode::TransportFailure,
            false,
            application::ConnectionState::Disconnected { attempt: 0 },
            false,
        ),
    ] {
        let mut model = AppModel {
            selected_session: Some("session-a".into()),
            ..Default::default()
        };
        let effect = request_snapshot(&mut model, "session-a");
        reduce(
            &mut model,
            AppAction::EffectFinished(snapshot_failure(&effect, code, rejected)),
        );
        assert!(model.snapshot_owner.is_none());
        assert_eq!(model.connection, expected_connection);
        assert_eq!(
            model.execution == application::ExecutionState::Failed,
            failed
        );
        assert_eq!(model.boot == application::BootState::Degraded, failed);
        assert_eq!(model.snapshot_failure.unwrap().host_rejected, rejected);
    }
}

#[test]
fn boot_reads_complete_once_in_either_order_and_reject_stale_generation() {
    let mut model = AppModel::default();
    let effects = reduce(&mut model, AppAction::BootStarted);
    let definitions = effects
        .iter()
        .find(|effect| matches!(effect.kind, EffectKind::LoadDefinitions))
        .unwrap();
    let page = effects
        .iter()
        .find(|effect| matches!(effect.kind, EffectKind::LoadSessionPage { .. }))
        .unwrap();
    let mut stale = boot_definitions(definitions, true, false);
    stale.context.issued_generation = AppGeneration(stale.context.issued_generation.0 + 1);
    reduce(&mut model, AppAction::EffectFinished(stale));
    assert_eq!(model.boot_completion_revision, 0);

    reduce(
        &mut model,
        AppAction::EffectFinished(page_success(page, vec![session("boot-session")], None)),
    );
    assert_eq!(model.boot_completion_revision, 0);
    reduce(
        &mut model,
        AppAction::EffectFinished(boot_definitions(definitions, true, false)),
    );
    assert_eq!(model.boot_completion_revision, 1);
    assert_eq!(model.definition_count, 1);
    assert_eq!(session_ids(&model), ["boot-session"]);
    reduce(
        &mut model,
        AppAction::EffectFinished(boot_definitions(definitions, true, false)),
    );
    assert_eq!(model.boot_completion_revision, 1);
}

#[test]
fn boot_single_double_failure_and_panic_settle_without_partial_ready() {
    for (definitions_fail, page_fail, page_panic) in [
        (true, false, false),
        (true, true, false),
        (false, false, true),
    ] {
        let mut model = AppModel::default();
        let effects = reduce(&mut model, AppAction::BootStarted);
        let definitions = effects
            .iter()
            .find(|effect| matches!(effect.kind, EffectKind::LoadDefinitions))
            .unwrap();
        let page = effects
            .iter()
            .find(|effect| matches!(effect.kind, EffectKind::LoadSessionPage { .. }))
            .unwrap();
        reduce(
            &mut model,
            AppAction::EffectFinished(boot_definitions(definitions, !definitions_fail, false)),
        );
        let page_result = if page_fail || page_panic {
            page_failure(page, page_panic)
        } else {
            page_success(page, vec![session("available")], None)
        };
        reduce(&mut model, AppAction::EffectFinished(page_result));
        assert_eq!(model.boot_completion_revision, 1);
        assert_eq!(model.boot, application::BootState::Degraded);
    }
}

#[test]
fn newer_boot_and_catalog_refresh_own_truth_without_stealing_selection() {
    let mut model = AppModel {
        selected_session: Some("current-session".into()),
        ..Default::default()
    };
    let old = reduce(&mut model, AppAction::BootStarted);
    let new = reduce(&mut model, AppAction::BootStarted);
    for effect in &new {
        let result = match &effect.kind {
            EffectKind::LoadDefinitions => boot_definitions(effect, true, false),
            EffectKind::LoadSessionPage { .. } => {
                page_success(effect, vec![session("new-boot")], None)
            }
            _ => unreachable!(),
        };
        reduce(&mut model, AppAction::EffectFinished(result));
    }
    for effect in &old {
        let result = match &effect.kind {
            EffectKind::LoadDefinitions => boot_definitions(effect, true, false),
            EffectKind::LoadSessionPage { .. } => {
                page_success(effect, vec![session("stale-boot")], None)
            }
            _ => unreachable!(),
        };
        reduce(&mut model, AppAction::EffectFinished(result));
    }
    assert_eq!(session_ids(&model), ["new-boot"]);
    let refresh = request_page(&mut model, SessionPagePurpose::CatalogRefresh, None);
    reduce(
        &mut model,
        AppAction::EffectFinished(page_success(&refresh, vec![session("created")], None)),
    );
    assert_eq!(model.selected_session.as_deref(), Some("current-session"));
    assert_eq!(model.catalog_refresh_revision, 1);
}

async fn run(mode: Mode) -> AppEffectResult {
    execute(
        EffectTracker::default()
            .issue(EffectKind::LoadDefinitions, None, None)
            .expect("effect identity"),
        mode,
    )
    .await
}

async fn execute(effect: AppEffect, mode: Mode) -> AppEffectResult {
    let (sender, mut receiver) = mpsc::channel(1);
    HostEffectRunner::new(FakeHostReadPort(mode), sender).submit(effect);
    match receiver.recv().await.expect("effect result") {
        AppAction::EffectFinished(result) => result,
        _ => panic!("unexpected action"),
    }
}

async fn execute_page(request: SessionPageRequest, mode: Mode) -> AppEffectResult {
    let effect = EffectTracker::default()
        .issue(
            EffectKind::LoadSessionPage {
                request: request.clone(),
            },
            None,
            Some(request.identity_digest()),
        )
        .expect("page effect identity");
    execute(effect, mode).await
}

fn replace_catalog(model: &mut AppModel, sessions: Vec<SessionSummary>, next: Option<&str>) {
    model.sessions = sessions;
    model.sessions_next_before = next.map(str::to_owned);
    model.session_count = model.sessions.len();
    model.sessions_loading = false;
    model.session_page_owner = None;
}

fn request_page(
    model: &mut AppModel,
    purpose: SessionPagePurpose,
    cursor: Option<&str>,
) -> AppEffect {
    reduce(
        model,
        AppAction::LoadSessionPageRequested(page_request(purpose, cursor)),
    )
    .pop()
    .expect("page effect")
}

fn page_request(purpose: SessionPagePurpose, cursor: Option<&str>) -> SessionPageRequest {
    SessionPageRequest {
        cursor: cursor.map(str::to_owned),
        purpose,
    }
}

fn request_snapshot(model: &mut AppModel, session_id: &str) -> AppEffect {
    reduce(
        model,
        AppAction::LoadSnapshotRequested(SnapshotRequest {
            session_id: session_id.into(),
        }),
    )
    .pop()
    .expect("snapshot effect")
}

fn snapshot_success(effect: &AppEffect, session_id: &str, position: u64) -> AppEffectResult {
    let EffectKind::LoadSnapshot { request } = &effect.kind else {
        panic!("snapshot effect")
    };
    AppEffectResult {
        context: effect.context.clone(),
        kind: effect.kind.tag(),
        outcome: AppEffectOutcome::HostRead(Ok(HostReadResponse::Snapshot(Box::new(
            SnapshotRead {
                request: request.clone(),
                view: SessionView {
                    api_version: "v1".into(),
                    session: session(session_id),
                    observed_max_position: position,
                },
                items: Vec::new(),
                follow_position: position,
            },
        )))),
    }
}

fn snapshot_failure(
    effect: &AppEffect,
    code: HostClientErrorCode,
    host_rejected: bool,
) -> AppEffectResult {
    AppEffectResult {
        context: effect.context.clone(),
        kind: effect.kind.tag(),
        outcome: AppEffectOutcome::HostRead(Err(HostReadFailure {
            code,
            host_rejected,
        })),
    }
}

fn page_success(
    effect: &AppEffect,
    sessions: Vec<SessionSummary>,
    next: Option<&str>,
) -> AppEffectResult {
    let EffectKind::LoadSessionPage { request } = &effect.kind else {
        panic!("page effect")
    };
    AppEffectResult {
        context: effect.context.clone(),
        kind: effect.kind.tag(),
        outcome: AppEffectOutcome::HostRead(Ok(HostReadResponse::SessionPage {
            request: request.clone(),
            sessions,
            next_before: next.map(str::to_owned),
        })),
    }
}

fn page_failure(effect: &AppEffect, panic: bool) -> AppEffectResult {
    AppEffectResult {
        context: effect.context.clone(),
        kind: effect.kind.tag(),
        outcome: if panic {
            AppEffectOutcome::Failed(EffectFailure::Internal)
        } else {
            AppEffectOutcome::HostRead(Err(HostReadFailure {
                code: HostClientErrorCode::TransportFailure,
                host_rejected: false,
            }))
        },
    }
}

fn boot_definitions(effect: &AppEffect, success: bool, panic: bool) -> AppEffectResult {
    AppEffectResult {
        context: effect.context.clone(),
        kind: effect.kind.tag(),
        outcome: if panic {
            AppEffectOutcome::Failed(EffectFailure::Internal)
        } else if success {
            AppEffectOutcome::HostRead(Ok(HostReadResponse::Definitions(vec![definition()])))
        } else {
            AppEffectOutcome::HostRead(Err(HostReadFailure {
                code: HostClientErrorCode::TransportFailure,
                host_rejected: false,
            }))
        },
    }
}

fn session_ids(model: &AppModel) -> Vec<&str> {
    model
        .sessions
        .iter()
        .map(|session| session.session_id.as_str())
        .collect()
}

fn definition() -> AgentDefinitionSummary {
    AgentDefinitionSummary {
        api_version: "v1".into(),
        definition_id: "private-definition".into(),
        definition_revision: "revision-a".into(),
        capabilities: vec!["private-capability".into()],
    }
}

fn session(id: &str) -> SessionSummary {
    SessionSummary {
        api_version: "v1".into(),
        session_id: id.into(),
        agent_instance_id: format!("agent-{id}"),
        definition_id: "definition-a".into(),
        definition_revision: "revision-a".into(),
        opened_at: "2026-09-01T00:00:00Z".into(),
        latest_position: 1,
        latest_turn_id: None,
        latest_turn_state: None,
        turn_count: 0,
    }
}
