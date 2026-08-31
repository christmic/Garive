use std::collections::BTreeSet;

use crate::{
    application::{AppAction, EffectKind, EffectTracker, Overlay},
    persistence::{PendingCommand, PendingKind},
};

use super::RuntimeState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GracefulQuitSafety {
    Ready,
    WaitingForPersistence,
    WaitingForHost,
    BlockedUnknown,
}

impl RuntimeState {
    pub(in crate::runtime) fn confirm_graceful_quit(&mut self) {
        if self.model.overlay != Some(Overlay::QuitConfirmation) {
            return;
        }
        self.graceful_quit_armed = true;
        self.advance_graceful_quit();
    }

    pub(in crate::runtime) fn advance_graceful_quit(&mut self) {
        if !self.graceful_quit_armed {
            return;
        }
        if self.model.overlay != Some(Overlay::QuitConfirmation) {
            self.graceful_quit_armed = false;
            return;
        }
        match graceful_quit_safety(
            self.store.is_ephemeral(),
            &self.model.effects,
            &self.pending,
            &self.pending_recovery,
            self.queued_prompt.is_some()
                && (!self.store.is_ephemeral() || self.ephemeral_confirmed),
        ) {
            GracefulQuitSafety::Ready => {
                self.graceful_quit_armed = false;
                self.dispatch(AppAction::QuitConfirmed);
            }
            GracefulQuitSafety::WaitingForPersistence => {
                self.model.notice =
                    Some("Finishing the accepted local commit before safe exit…".into());
            }
            GracefulQuitSafety::WaitingForHost => {
                self.model.notice = Some(
                    "Waiting for accepted work to reach a recoverable boundary before safe exit…"
                        .into(),
                );
            }
            GracefulQuitSafety::BlockedUnknown => {
                self.graceful_quit_armed = false;
                self.model.notice = Some(
                    "Safe exit paused because a command outcome is unknown. Review exact recovery before leaving."
                        .into(),
                );
                self.model.overlay = Some(Overlay::UnknownCommand);
            }
        }
    }
}

fn graceful_quit_safety(
    ephemeral: bool,
    effects: &EffectTracker,
    pending: &[PendingCommand],
    pending_recovery: &BTreeSet<String>,
    accepted_queued_prompt: bool,
) -> GracefulQuitSafety {
    if pending
        .iter()
        .any(|command| pending_recovery.contains(&command.command_id))
    {
        return GracefulQuitSafety::BlockedUnknown;
    }
    if effects.pending.values().any(|effect| {
        matches!(
            effect.kind,
            EffectKind::PersistPending { .. } | EffectKind::PersistContinuation { .. }
        )
    }) {
        return GracefulQuitSafety::WaitingForPersistence;
    }
    let queued_prompt_owner = effects.pending.values().any(|effect| {
        matches!(
            &effect.kind,
            EffectKind::PersistPending { draft }
                if matches!(
                    draft.kind,
                    crate::application::PendingMutationKind::CreateSession
                        | crate::application::PendingMutationKind::StartTurn
                )
        )
    }) || pending.iter().any(|command| {
        matches!(
            command.kind,
            PendingKind::CreateSession | PendingKind::StartTurn
        )
    });
    if accepted_queued_prompt && !queued_prompt_owner {
        return GracefulQuitSafety::BlockedUnknown;
    }
    if accepted_queued_prompt || (ephemeral && !pending.is_empty()) {
        GracefulQuitSafety::WaitingForHost
    } else {
        GracefulQuitSafety::Ready
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{
        application::{EffectKind, EffectTracker, PendingMutationDraft, PendingMutationKind},
        persistence::{PendingCommand, PendingKind, Preferences, StateStore},
        runtime::app::LIMITS,
    };

    use super::{graceful_quit_safety, GracefulQuitSafety, RuntimeState};
    use crate::runtime::app::state::RestoredState;

    #[test]
    fn persistence_must_settle_before_every_clean_exit() {
        let mut effects = EffectTracker::default();
        effects
            .issue(
                EffectKind::PersistPending {
                    draft: draft(PendingMutationKind::StartTurn),
                },
                Some("session-a".into()),
                None,
            )
            .expect("effect identity");

        for ephemeral in [false, true] {
            assert_eq!(
                graceful_quit_safety(ephemeral, &effects, &[], &Default::default(), false,),
                GracefulQuitSafety::WaitingForPersistence
            );
        }
    }

    #[test]
    fn durable_pending_is_recoverable_but_ephemeral_pending_waits_for_host() {
        let pending = vec![pending(PendingKind::StartTurn)];
        assert_eq!(
            graceful_quit_safety(
                false,
                &EffectTracker::default(),
                &pending,
                &Default::default(),
                false
            ),
            GracefulQuitSafety::Ready
        );
        assert_eq!(
            graceful_quit_safety(
                true,
                &EffectTracker::default(),
                &pending,
                &Default::default(),
                false
            ),
            GracefulQuitSafety::WaitingForHost
        );
        assert_eq!(
            graceful_quit_safety(
                true,
                &EffectTracker::default(),
                &[],
                &Default::default(),
                false
            ),
            GracefulQuitSafety::Ready
        );
    }

    #[test]
    fn accepted_queued_prompt_waits_for_create_and_unknown_cancels_exit() {
        let create = vec![pending(PendingKind::CreateSession)];
        assert_eq!(
            graceful_quit_safety(
                false,
                &EffectTracker::default(),
                &create,
                &Default::default(),
                true
            ),
            GracefulQuitSafety::WaitingForHost
        );
        assert_eq!(
            graceful_quit_safety(
                false,
                &EffectTracker::default(),
                &create,
                &Default::default(),
                false
            ),
            GracefulQuitSafety::Ready
        );
        let recovery = [create[0].command_id.clone()].into_iter().collect();
        assert_eq!(
            graceful_quit_safety(true, &EffectTracker::default(), &create, &recovery, true),
            GracefulQuitSafety::BlockedUnknown
        );
        assert_eq!(
            graceful_quit_safety(
                false,
                &EffectTracker::default(),
                &[],
                &Default::default(),
                false
            ),
            GracefulQuitSafety::Ready,
            "an unaccepted ephemeral queued prompt is not a shutdown barrier"
        );
        assert_eq!(
            graceful_quit_safety(
                false,
                &EffectTracker::default(),
                &[],
                &Default::default(),
                true
            ),
            GracefulQuitSafety::BlockedUnknown,
            "accepted queued work without a mutation owner is unsafe"
        );
    }

    #[test]
    fn runtime_confirmation_waits_advances_and_cancels_on_the_exact_boundary() {
        let mut persisting = runtime(false);
        persisting.model.overlay = Some(crate::application::Overlay::QuitConfirmation);
        persisting
            .model
            .effects
            .issue(
                EffectKind::PersistPending {
                    draft: draft(PendingMutationKind::StartTurn),
                },
                Some("session-a".into()),
                None,
            )
            .expect("effect identity");
        persisting.confirm_graceful_quit();
        assert!(persisting.graceful_quit_armed);
        assert!(!persisting.model.quit_requested);

        let mut durable = runtime(false);
        durable.model.overlay = Some(crate::application::Overlay::QuitConfirmation);
        durable.pending.push(pending(PendingKind::StartTurn));
        durable.confirm_graceful_quit();
        assert!(durable.model.quit_requested);

        let mut ephemeral = runtime(true);
        ephemeral.model.overlay = Some(crate::application::Overlay::QuitConfirmation);
        ephemeral.pending.push(pending(PendingKind::StartTurn));
        ephemeral.confirm_graceful_quit();
        assert!(ephemeral.graceful_quit_armed);
        assert!(!ephemeral.model.quit_requested);
        ephemeral.pending.clear();
        ephemeral.advance_graceful_quit();
        assert!(ephemeral.model.quit_requested);

        let mut queued = runtime(false);
        queued.model.overlay = Some(crate::application::Overlay::QuitConfirmation);
        queued.queued_prompt = Some("private".into());
        queued.pending.push(pending(PendingKind::CreateSession));
        queued.confirm_graceful_quit();
        assert!(queued.graceful_quit_armed);
        assert!(!queued.model.quit_requested);

        let mut unknown = runtime(true);
        unknown.model.overlay = Some(crate::application::Overlay::QuitConfirmation);
        unknown.pending.push(pending(PendingKind::StartTurn));
        unknown.pending_recovery.insert("command-a".into());
        unknown.confirm_graceful_quit();
        assert!(!unknown.graceful_quit_armed);
        assert!(!unknown.model.quit_requested);
        assert_eq!(
            unknown.model.overlay,
            Some(crate::application::Overlay::UnknownCommand)
        );
    }

    fn draft(kind: PendingMutationKind) -> PendingMutationDraft {
        PendingMutationDraft {
            command_id: "command-a".into(),
            kind,
            session_id: Some("session-a".into()),
            turn_id: None,
            suspension_id: None,
            expected_session_version: None,
            requested_through_position: None,
            request_payload: json!({"text":"private"}),
            created_at: "2026-09-01T00:00:00Z".into(),
        }
    }

    fn pending(kind: PendingKind) -> PendingCommand {
        PendingCommand {
            schema_version: 1,
            command_id: "command-a".into(),
            kind,
            session_id: (kind != PendingKind::CreateSession).then(|| "session-a".into()),
            turn_id: None,
            suspension_id: None,
            expected_session_version: None,
            requested_through_position: None,
            request_payload: match kind {
                PendingKind::CreateSession => json!({"agent_definition_id":"agent-a"}),
                _ => json!({"text":"private"}),
            },
            request_digest: "a".repeat(64),
            created_at: "2026-09-01T00:00:00Z".into(),
        }
    }

    fn runtime(ephemeral: bool) -> RuntimeState {
        let mut arguments = vec!["garive-tui", "--host", "http://127.0.0.1:4317/"];
        if ephemeral {
            arguments.push("--ephemeral");
        }
        let config = crate::parse_launch_config(arguments).expect("launch config");
        let store = if ephemeral {
            StateStore::open(None, true).expect("ephemeral store")
        } else {
            let root = tempfile::tempdir().expect("temporary state");
            StateStore::open(Some(root.path().join("state")), false).expect("durable store")
        };
        let client =
            garive_host_client::LiveHostClient::new(&config.host, LIMITS).expect("loopback client");
        let (host_sender, _) = tokio::sync::mpsc::channel(8);
        let (action_sender, _) = tokio::sync::mpsc::channel(8);
        RuntimeState::new(
            config,
            client,
            host_sender,
            action_sender,
            RestoredState {
                store,
                preferences: Preferences::default(),
                pending: Vec::new(),
                pending_quarantined: 0,
                history: Vec::new(),
                history_error: false,
            },
        )
    }
}
