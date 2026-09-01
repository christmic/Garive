use garive_host_client::HostClientErrorCode;

use crate::{
    application::{
        AppAction, Overlay, PendingMutationDraft, PendingMutationKind, PersistedPendingIdentity,
    },
    persistence::{now, PendingCommand, PendingKind, PromptHistoryEntry},
};

use super::RuntimeState;
use crate::runtime::app::state_error_name;

impl RuntimeState {
    pub(in crate::runtime::app::state) fn activate_persisted_mutation(
        &mut self,
        draft: PendingMutationDraft,
        identity: PersistedPendingIdentity,
    ) -> Option<PendingCommand> {
        let kind = match draft.kind {
            PendingMutationKind::CancelTurn => PendingKind::CancelTurn,
            PendingMutationKind::ContinueTurn => PendingKind::ContinueTurn,
            _ => return None,
        };
        let pending = PendingCommand {
            schema_version: 1,
            command_id: draft.command_id,
            kind,
            session_id: draft.session_id,
            turn_id: draft.turn_id,
            suspension_id: draft.suspension_id,
            expected_session_version: draft.expected_session_version,
            requested_through_position: draft.requested_through_position,
            request_payload: draft.request_payload,
            request_digest: identity.request_digest,
            created_at: draft.created_at,
        };
        if pending.command_id != identity.command_id || pending.validate().is_err() {
            self.local_state_failure("invalid_persisted_mutation");
            return None;
        }
        if pending.kind == PendingKind::CancelTurn {
            if let (Some(session_id), Some(turn_id)) =
                (pending.session_id.clone(), pending.turn_id.clone())
            {
                self.model
                    .cancel_requests
                    .begin(pending.command_id.clone(), session_id, turn_id);
            }
        }
        self.pending.push(pending.clone());
        self.sync_pending_projection();
        #[cfg(feature = "test-hooks")]
        self.crash_if(crate::args::TestCrashHook::PendingPersisted);
        Some(pending)
    }

    pub(in crate::runtime::app::state) fn mark_pending_unknown(&mut self, command_id: &str) {
        self.pending_recovery.insert(command_id.into());
        self.model.cancel_requests.mark_unknown(command_id);
        self.sync_pending_projection();
        self.model.notice = Some("The command context changed before it could be sent.".into());
        self.model.overlay = Some(Overlay::UnknownCommand);
    }

    pub(in crate::runtime) fn request_create_session(
        &mut self,
        command_id: String,
        definition_id: String,
    ) -> bool {
        if self
            .pending
            .iter()
            .any(|pending| pending.session_id.is_none())
        {
            self.model.notice =
                Some("A Session creation already has an unknown outcome. Use /retry first.".into());
            self.model.overlay = Some(Overlay::UnknownCommand);
            return false;
        }
        if self.store.is_ephemeral() && !self.ephemeral_confirmed {
            if self.deferred_ephemeral.is_some() {
                self.model.notice =
                    Some("An ephemeral operation is already awaiting consent.".into());
                return false;
            }
            self.deferred_ephemeral = Some(PendingCommand {
                schema_version: 1,
                command_id,
                kind: PendingKind::CreateSession,
                session_id: None,
                turn_id: None,
                suspension_id: None,
                expected_session_version: None,
                requested_through_position: None,
                request_payload: serde_json::json!({"agent_definition_id": definition_id}),
                request_digest: String::new(),
                created_at: now(),
            });
            self.model.overlay = Some(Overlay::EphemeralConfirmation);
            return true;
        }
        self.dispatch(AppAction::CreateSessionRequested(PendingMutationDraft {
            command_id,
            kind: PendingMutationKind::CreateSession,
            session_id: None,
            turn_id: None,
            suspension_id: None,
            expected_session_version: None,
            requested_through_position: None,
            request_payload: serde_json::json!({"agent_definition_id": definition_id}),
            created_at: now(),
        }));
        true
    }

    pub(super) fn activate_persisted_create(
        &mut self,
        draft: PendingMutationDraft,
        identity: PersistedPendingIdentity,
    ) -> Option<(String, String)> {
        let definition_id = draft
            .request_payload
            .get("agent_definition_id")?
            .as_str()?
            .to_owned();
        let pending = PendingCommand {
            schema_version: 1,
            command_id: draft.command_id,
            kind: PendingKind::CreateSession,
            session_id: None,
            turn_id: None,
            suspension_id: None,
            expected_session_version: None,
            requested_through_position: None,
            request_payload: draft.request_payload,
            request_digest: identity.request_digest,
            created_at: draft.created_at,
        };
        if pending.command_id != identity.command_id || pending.validate().is_err() {
            self.local_state_failure("invalid_persisted_create");
            return None;
        }
        let command_id = pending.command_id.clone();
        self.pending.push(pending);
        self.sync_pending_projection();
        #[cfg(feature = "test-hooks")]
        self.crash_if(crate::args::TestCrashHook::PendingPersisted);
        Some((command_id, definition_id))
    }

    pub(in crate::runtime) fn request_start_turn(
        &mut self,
        command_id: String,
        session_id: String,
        text: String,
    ) {
        if self.store.is_ephemeral() && !self.ephemeral_confirmed {
            if self.deferred_ephemeral.is_some() {
                self.model.notice =
                    Some("An ephemeral operation is already awaiting consent.".into());
                return;
            }
            self.deferred_ephemeral = Some(PendingCommand {
                schema_version: 1,
                command_id,
                kind: PendingKind::StartTurn,
                session_id: Some(session_id),
                turn_id: None,
                suspension_id: None,
                expected_session_version: None,
                requested_through_position: None,
                request_payload: serde_json::json!({"text": text}),
                request_digest: String::new(),
                created_at: now(),
            });
            self.model.overlay = Some(Overlay::EphemeralConfirmation);
            return;
        }
        self.dispatch(AppAction::StartTurnRequested(PendingMutationDraft {
            command_id,
            kind: PendingMutationKind::StartTurn,
            session_id: Some(session_id),
            turn_id: None,
            suspension_id: None,
            expected_session_version: None,
            requested_through_position: None,
            request_payload: serde_json::json!({"text": text}),
            created_at: now(),
        }));
    }

    pub(super) fn activate_persisted_start(
        &mut self,
        draft: PendingMutationDraft,
        identity: PersistedPendingIdentity,
    ) -> Option<(String, String, String)> {
        let session_id = draft.session_id.clone()?;
        let text = draft.request_payload.get("text")?.as_str()?.to_owned();
        let pending = PendingCommand {
            schema_version: 1,
            command_id: draft.command_id,
            kind: PendingKind::StartTurn,
            session_id: draft.session_id,
            turn_id: None,
            suspension_id: None,
            expected_session_version: None,
            requested_through_position: None,
            request_payload: draft.request_payload,
            request_digest: identity.request_digest,
            created_at: draft.created_at,
        };
        if pending.command_id != identity.command_id || pending.validate().is_err() {
            self.local_state_failure("invalid_persisted_start");
            return None;
        }
        let command_id = pending.command_id.clone();
        self.pending.push(pending);
        self.sync_pending_projection();
        #[cfg(feature = "test-hooks")]
        self.crash_if(crate::args::TestCrashHook::PendingPersisted);
        Some((command_id, session_id, text))
    }

    pub(in crate::runtime) fn persist_presentation(&mut self) {
        if let Some(session) = self.model.selected_session.as_deref() {
            self.preferences
                .set_draft(session, self.model.composer.text());
            self.preferences.selected_session_id = Some(session.into());
        }
        self.preferences.theme = self.config.theme;
        self.preferences.mouse = self.config.mouse;
        self.preferences.reduced_motion = self.config.reduced_motion;
        self.preferences.activity_inspector = self.model.inspector.open;
        if let Err(error) = self
            .store
            .save_preferences_merged(&mut self.preferences, &mut self.persisted_preferences)
        {
            self.model.notice = Some(format!("Local state: {}", state_error_name(error)));
        }
    }

    pub(in crate::runtime) fn admit_pending(&mut self, pending: PendingCommand) -> bool {
        if self.store.is_ephemeral() && !self.ephemeral_confirmed {
            if self.deferred_ephemeral.is_some() {
                self.model.notice =
                    Some("An ephemeral operation is already awaiting consent.".into());
                return false;
            }
            self.deferred_ephemeral = Some(pending);
            self.model.overlay = Some(Overlay::EphemeralConfirmation);
            return false;
        }
        if self
            .pending
            .iter()
            .any(|value| value.session_id == pending.session_id)
        {
            self.model.notice = Some(
                "This Session already has an unknown command outcome. Use /retry first.".into(),
            );
            self.model.overlay = Some(Overlay::UnknownCommand);
            return false;
        }
        let Ok(pending) = pending.seal() else {
            self.local_state_failure("invalid_pending_command");
            return false;
        };
        if self.store.save_pending(&pending).is_err() {
            self.local_state_failure("pending_write_failed");
            return false;
        }
        self.pending.push(pending);
        self.sync_pending_projection();
        #[cfg(feature = "test-hooks")]
        self.crash_if(crate::args::TestCrashHook::PendingPersisted);
        true
    }

    pub(in crate::runtime) fn pending_for_context(&self) -> Option<&PendingCommand> {
        let selected = self.model.selected_session.as_deref();
        self.pending
            .iter()
            .find(|value| value.session_id.as_deref() == selected)
            .or_else(|| self.pending.first())
    }

    pub(super) fn sync_pending_projection(&mut self) {
        let (has_pending_command, composer_is_frozen) = pending_command_projection(
            &self.pending,
            &self.model.effects,
            self.model.selected_session.as_deref(),
        );
        self.model.has_pending_command = has_pending_command;
        let selected = self.model.selected_session.as_deref();
        self.model.pending_recovery.current_session = self.pending.iter().any(|pending| {
            self.pending_recovery.contains(&pending.command_id)
                && pending.session_id.as_deref() == selected
        });
        self.model.pending_recovery.other_session = self.pending.iter().any(|pending| {
            self.pending_recovery.contains(&pending.command_id)
                && pending.session_id.as_deref() != selected
        });
        self.model.composer_is_frozen = composer_is_frozen;
    }

    pub(in crate::runtime) fn composer_is_frozen(&self) -> bool {
        self.model.composer_is_frozen
    }

    pub(in crate::runtime) fn explain_frozen_composer(&mut self) {
        self.model.notice =
            Some("This draft is frozen until the pending command reaches durable truth.".into());
    }

    pub(in crate::runtime) fn abandon_pending(&mut self) {
        let Some(command_id) = self
            .pending_for_context()
            .map(|value| value.command_id.clone())
        else {
            self.model.overlay = None;
            return;
        };
        let index = self
            .pending
            .iter()
            .position(|value| value.command_id == command_id)
            .expect("pending command remains present");
        if self
            .store
            .remove_pending(self.pending[index].session_id.as_deref())
            .is_err()
        {
            self.local_state_failure("pending_abandon_failed");
            return;
        }
        self.pending_recovery.remove(&command_id);
        self.model.cancel_requests.clear_command(&command_id);
        self.clear_exact_retry_owner(&command_id);
        self.pending.remove(index);
        self.sync_pending_projection();
        self.model.overlay = None;
        self.model.reconcile_inspector_surface();
        if let Some(session) = self.model.selected_session.clone() {
            self.load(session);
        }
    }

    pub(in crate::runtime) fn finish_pending(&mut self, command_id: &str, submitted_text: &str) {
        let Some(index) = self
            .pending
            .iter()
            .position(|value| value.command_id == command_id)
        else {
            return;
        };
        #[cfg(feature = "test-hooks")]
        self.crash_if(crate::args::TestCrashHook::ResponseAccepted);
        let pending = self.pending[index].clone();
        if self
            .store
            .remove_pending(pending.session_id.as_deref())
            .is_err()
        {
            self.local_state_failure("pending_remove_failed");
            return;
        }
        self.pending_recovery.remove(command_id);
        self.clear_exact_retry_owner(command_id);
        #[cfg(feature = "test-hooks")]
        self.crash_if(crate::args::TestCrashHook::PendingRemoved);
        if !self.config.no_prompt_history
            && !submitted_text.is_empty()
            && matches!(
                pending.kind,
                PendingKind::StartTurn | PendingKind::ContinueTurn
            )
        {
            if let Some(session_id) = pending.session_id.clone() {
                let entry = PromptHistoryEntry {
                    schema_version: 1,
                    entry_id: uuid::Uuid::new_v4().to_string(),
                    session_id,
                    submitted_text: submitted_text.into(),
                    submitted_at: now(),
                };
                if self.store.append_history(&entry).is_err() {
                    self.model.notice = Some("Prompt history could not be saved.".into());
                } else {
                    self.model
                        .prompt_history
                        .retain(|value| value != submitted_text);
                    self.model.prompt_history.insert(0, submitted_text.into());
                    self.model.prompt_history.truncate(500);
                }
            }
        }
        self.pending.remove(index);
        self.sync_pending_projection();
        if matches!(
            self.model.overlay,
            Some(Overlay::UnknownCommand | Overlay::AbandonConfirmation)
        ) {
            self.model.overlay = None;
            self.model.reconcile_inspector_surface();
        }
    }

    #[cfg(feature = "test-hooks")]
    fn crash_if(&self, point: crate::args::TestCrashHook) {
        if self.config.test_crash_hook == Some(point) {
            let name = match point {
                crate::args::TestCrashHook::TerminalAcquiredPanic => "terminal-acquired-panic",
                crate::args::TestCrashHook::PendingPersisted => "pending-persisted",
                crate::args::TestCrashHook::ResponseAccepted => "response-accepted",
                crate::args::TestCrashHook::PendingRemoved => "pending-removed",
            };
            eprintln!("GARIVE_TEST_CRASH_HOOK={name}");
            loop {
                std::thread::park();
            }
        }
    }

    pub(in crate::runtime) fn reject_pending(
        &mut self,
        command_id: &str,
        code: HostClientErrorCode,
    ) {
        self.clear_exact_retry_owner(command_id);
        let pending_index = self
            .pending
            .iter()
            .position(|value| value.command_id == command_id);
        if matches!(
            code,
            HostClientErrorCode::HostFailure | HostClientErrorCode::InvalidCommand
        ) {
            if let Some(index) = pending_index {
                let pending = self.pending.remove(index);
                self.pending_recovery.remove(command_id);
                let _ = self.store.remove_pending(pending.session_id.as_deref());
            }
            self.model.cancel_requests.clear_command(command_id);
            self.sync_pending_projection();
        } else if pending_index.is_some() {
            self.pending_recovery.insert(command_id.into());
            self.model.cancel_requests.mark_unknown(command_id);
            self.sync_pending_projection();
            self.model.notice =
                Some("The command outcome is unknown. Review /status or use exact /retry.".into());
            self.model.overlay = Some(Overlay::UnknownCommand);
        }
    }

    fn local_state_failure(&mut self, code: &str) {
        self.model.notice = Some(format!("Local recovery state: {code}"));
        self.model.overlay = Some(Overlay::ErrorDetails);
    }
}

pub(in crate::runtime::app) fn pending_freezes_composer(
    pending: &[PendingCommand],
    selected: Option<&str>,
) -> bool {
    pending.iter().any(|pending| {
        pending.session_id.as_deref() == selected
            || (selected.is_none() && pending.kind == PendingKind::CreateSession)
    })
}

pub(in crate::runtime::app) fn pending_command_projection(
    pending: &[PendingCommand],
    effects: &crate::application::EffectTracker,
    selected: Option<&str>,
) -> (bool, bool) {
    (
        !pending.is_empty() || effects.has_pending_mutation(),
        pending_freezes_composer(pending, selected)
            || effects.has_pending_mutation_for_context(selected),
    )
}
