use tokio::sync::mpsc;

use crate::{
    application::{
        AppAction, AppEffect, AppEffectOutcome, AppEffectResult, EffectFailure, EffectKind,
    },
    persistence::PersistencePort,
};

pub(crate) struct EffectRunner<P> {
    persistence: P,
    sender: mpsc::Sender<AppAction>,
}

impl<P: PersistencePort> EffectRunner<P> {
    pub(crate) fn new(persistence: P, sender: mpsc::Sender<AppAction>) -> Self {
        Self {
            persistence,
            sender,
        }
    }

    pub(crate) fn submit(&self, effect: AppEffect) {
        let persistence = self.persistence.clone();
        let sender = self.sender.clone();
        let context = effect.context.clone();
        let kind = effect.kind.tag();
        tokio::spawn(async move {
            let result = match tokio::spawn(execute(effect, persistence)).await {
                Ok(result) => result,
                Err(_) => AppEffectResult {
                    context,
                    kind,
                    outcome: AppEffectOutcome::Failed(EffectFailure::Internal),
                },
            };
            let _ = sender.send(AppAction::EffectFinished(result)).await;
        });
    }
}

async fn execute<P: PersistencePort>(effect: AppEffect, persistence: P) -> AppEffectResult {
    let kind = effect.kind.tag();
    let outcome = match effect.kind {
        EffectKind::Exit => AppEffectOutcome::Completed,
        EffectKind::LoadDefinitions
        | EffectKind::LoadSessionPage { .. }
        | EffectKind::LoadSnapshot { .. } => AppEffectOutcome::Failed(EffectFailure::Internal),
        EffectKind::PersistPending { draft } => {
            AppEffectOutcome::PendingPersisted(persistence.persist_pending(draft).await)
        }
        EffectKind::StartTurn { .. } => AppEffectOutcome::Failed(EffectFailure::Internal),
        EffectKind::CreateSession { .. } => AppEffectOutcome::Failed(EffectFailure::Internal),
        EffectKind::CancelTurn { .. } | EffectKind::ContinueTurn { .. } => {
            AppEffectOutcome::Failed(EffectFailure::Internal)
        }
        EffectKind::PersistContinuation { draft, .. } => {
            AppEffectOutcome::PendingPersisted(persistence.persist_pending(draft).await)
        }
    };
    AppEffectResult {
        context: effect.context,
        kind,
        outcome,
    }
}
