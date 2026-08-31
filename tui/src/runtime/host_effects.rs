use tokio::sync::mpsc;

use crate::{
    application::{
        AppAction, AppEffect, AppEffectOutcome, AppEffectResult, EffectFailure, EffectKind,
    },
    host::HostReadPort,
};

pub(crate) struct HostEffectRunner<H> {
    host: H,
    sender: mpsc::Sender<AppAction>,
}

impl<H: HostReadPort> HostEffectRunner<H> {
    pub(crate) fn new(host: H, sender: mpsc::Sender<AppAction>) -> Self {
        Self { host, sender }
    }

    pub(crate) fn submit(&self, effect: AppEffect) {
        let host = self.host.clone();
        let sender = self.sender.clone();
        let context = effect.context.clone();
        let kind = effect.kind.tag();
        tokio::spawn(async move {
            let result = match tokio::spawn(execute(effect, host)).await {
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

async fn execute<H: HostReadPort>(effect: AppEffect, host: H) -> AppEffectResult {
    let kind = effect.kind.tag();
    let outcome = match effect.kind {
        EffectKind::LoadDefinitions => AppEffectOutcome::HostRead(host.load_definitions().await),
        EffectKind::LoadSessionPage { request } => {
            AppEffectOutcome::HostRead(host.load_session_page(request).await)
        }
        EffectKind::LoadSnapshot { request } => {
            AppEffectOutcome::HostRead(host.load_snapshot(request).await)
        }
        _ => AppEffectOutcome::Failed(EffectFailure::Internal),
    };
    AppEffectResult {
        context: effect.context,
        kind,
        outcome,
    }
}
