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
};
use garive_host_client::{AgentDefinitionSummary, HostClientErrorCode};
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
                }),
                Mode::Panic => panic!("injected Host read panic"),
            }
        })
    }
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
            code: HostClientErrorCode::TransportFailure
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

fn definition() -> AgentDefinitionSummary {
    AgentDefinitionSummary {
        api_version: "v1".into(),
        definition_id: "private-definition".into(),
        definition_revision: "revision-a".into(),
        capabilities: vec!["private-capability".into()],
    }
}
