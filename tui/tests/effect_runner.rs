#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::{MouseMode, Theme};
#[path = "../src/application/mod.rs"]
mod application;
#[path = "../src/runtime/effects.rs"]
mod effects;
#[path = "../src/input/mod.rs"]
mod input;
#[path = "../src/persistence/mod.rs"]
mod persistence;

use std::{future::Future, pin::Pin, sync::Arc};

use application::{
    reduce, AppAction, AppEffectOutcome, AppModel, BootState, EffectFailure, EffectKind,
    EffectTracker, PendingMutationDraft, PendingMutationKind, PersistedPendingIdentity,
    PersistenceFailure, TerminalSize,
};
use effects::EffectRunner;
use persistence::{AsyncStateStore, PersistencePort, StateStore};
use serde_json::json;
use tokio::sync::{mpsc, Notify};

#[derive(Clone)]
enum Mode {
    Success,
    Failure,
    Slow(Arc<Notify>),
    Panic,
}

#[derive(Clone)]
struct FakePort(Mode);

impl PersistencePort for FakePort {
    fn persist_pending(
        &self,
        draft: PendingMutationDraft,
    ) -> Pin<Box<dyn Future<Output = Result<PersistedPendingIdentity, PersistenceFailure>> + Send>>
    {
        let mode = self.0.clone();
        Box::pin(async move {
            match mode {
                Mode::Success => Ok(identity(&draft.command_id)),
                Mode::Failure => Err(PersistenceFailure::Unavailable),
                Mode::Slow(release) => {
                    release.notified().await;
                    Ok(identity(&draft.command_id))
                }
                Mode::Panic => panic!("injected persistence panic"),
            }
        })
    }
}

#[tokio::test]
async fn runner_reports_success_failure_and_task_panic() {
    assert!(matches!(
        run(Mode::Success).await,
        AppEffectOutcome::PendingPersisted(Ok(_))
    ));
    assert_eq!(
        run(Mode::Failure).await,
        AppEffectOutcome::PendingPersisted(Err(PersistenceFailure::Unavailable))
    );
    assert_eq!(
        run(Mode::Panic).await,
        AppEffectOutcome::Failed(EffectFailure::Internal)
    );
}

#[tokio::test]
async fn slow_write_does_not_block_reducer_actions() {
    let release = Arc::new(Notify::new());
    let (sender, mut receiver) = mpsc::channel(1);
    EffectRunner::new(FakePort(Mode::Slow(Arc::clone(&release))), sender).submit(effect());

    let mut model = AppModel::default();
    reduce(
        &mut model,
        AppAction::TerminalResized(TerminalSize {
            width: 100,
            height: 40,
        }),
    );
    reduce(&mut model, AppAction::BootStarted);
    assert_eq!(model.terminal_size.width, 100);
    assert_eq!(model.boot, BootState::Loading);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), receiver.recv())
            .await
            .is_err()
    );

    release.notify_one();
    assert!(matches!(
        outcome(receiver.recv().await),
        AppEffectOutcome::PendingPersisted(Ok(_))
    ));
}

#[tokio::test]
async fn state_store_adapter_seals_and_persists_the_draft() {
    let temporary = tempfile::tempdir().unwrap();
    let store = StateStore::open(Some(temporary.path().join("state")), false).unwrap();
    let identity = AsyncStateStore::new(store.clone())
        .persist_pending(draft())
        .await
        .unwrap();
    let (pending, quarantined) = store.load_pending().unwrap();

    assert_eq!(quarantined, 0);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].command_id, identity.command_id);
    assert_eq!(pending[0].request_digest, identity.request_digest);
    assert_eq!(identity.request_digest.len(), 64);
}

async fn run(mode: Mode) -> AppEffectOutcome {
    let (sender, mut receiver) = mpsc::channel(1);
    EffectRunner::new(FakePort(mode), sender).submit(effect());
    outcome(receiver.recv().await)
}

fn outcome(action: Option<AppAction>) -> AppEffectOutcome {
    match action.expect("effect result") {
        AppAction::EffectFinished(result) => result.outcome,
        _ => panic!("unexpected action"),
    }
}

fn effect() -> application::AppEffect {
    EffectTracker::default()
        .issue(
            EffectKind::PersistPending { draft: draft() },
            Some("session-effect".into()),
            None,
        )
        .unwrap()
}

fn draft() -> PendingMutationDraft {
    PendingMutationDraft {
        command_id: "command-effect".into(),
        kind: PendingMutationKind::StartTurn,
        session_id: Some("session-effect".into()),
        turn_id: None,
        suspension_id: None,
        expected_session_version: Some(2),
        requested_through_position: None,
        request_payload: json!({"text":"private"}),
        created_at: "2026-09-01T00:00:00Z".into(),
    }
}

fn identity(command_id: &str) -> PersistedPendingIdentity {
    PersistedPendingIdentity {
        command_id: command_id.into(),
        request_digest: "a".repeat(64),
    }
}
