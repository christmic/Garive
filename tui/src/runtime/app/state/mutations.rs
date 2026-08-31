use serde_json::Value;

use crate::{
    application::{AppAction, PendingMutationDraft, PendingMutationKind, PersistedPendingIdentity},
    persistence::{now, PendingCommand, PendingKind},
    runtime::host::{ContinuationInput, ContinuationRequest},
};

use super::RuntimeState;

impl RuntimeState {
    pub(in crate::runtime) fn request_cancel_turn(
        &mut self,
        command_id: String,
        session_id: String,
        turn_id: String,
        position: u64,
    ) {
        let draft = PendingMutationDraft {
            command_id,
            kind: PendingMutationKind::CancelTurn,
            session_id: Some(session_id.clone()),
            turn_id: Some(turn_id),
            suspension_id: None,
            expected_session_version: None,
            requested_through_position: Some(position),
            request_payload: serde_json::json!({
                "session_id": session_id,
                "requested_through_position": position
            }),
            created_at: now(),
        };
        if !self.defer_ephemeral_mutation(&draft) {
            self.dispatch(AppAction::CancelTurnRequested(draft));
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::runtime) fn request_continue_turn(
        &mut self,
        command_id: String,
        session_id: String,
        turn_id: String,
        suspension_id: String,
        expected_session_version: u64,
        schema_digest: String,
        input_json: Value,
    ) {
        let draft = PendingMutationDraft {
            command_id,
            kind: PendingMutationKind::ContinueTurn,
            session_id: Some(session_id),
            turn_id: Some(turn_id),
            suspension_id: Some(suspension_id),
            expected_session_version: Some(expected_session_version),
            requested_through_position: None,
            request_payload: serde_json::json!({"input_json": input_json}),
            created_at: now(),
        };
        if !self.defer_ephemeral_mutation(&draft) {
            self.dispatch(AppAction::ContinueTurnRequested {
                draft,
                schema_digest,
            });
        } else if self
            .deferred_ephemeral
            .as_ref()
            .is_some_and(|pending| pending.command_id == draft.command_id)
        {
            self.deferred_continuation_schema_digest = Some(schema_digest);
        }
    }

    pub(super) fn activate_persisted_cancel(
        &mut self,
        draft: PendingMutationDraft,
        identity: PersistedPendingIdentity,
    ) -> Option<(String, String, String, u64)> {
        let pending = self.activate_persisted_mutation(draft, identity)?;
        Some((
            pending.command_id,
            pending.session_id?,
            pending.turn_id?,
            pending.requested_through_position?,
        ))
    }

    pub(super) fn activate_persisted_continue(
        &mut self,
        draft: PendingMutationDraft,
        identity: PersistedPendingIdentity,
        host_allowed: bool,
    ) -> Option<ContinuationRequest> {
        let pending = self.activate_persisted_mutation(draft, identity)?;
        if !host_allowed {
            self.mark_pending_unknown(&pending.command_id);
            return None;
        }
        Some(ContinuationRequest {
            command_id: pending.command_id,
            session_id: pending.session_id?,
            turn_id: pending.turn_id?,
            suspension_id: pending.suspension_id?,
            expected_session_version: pending.expected_session_version?,
            input: ContinuationInput::Json(pending.request_payload.get("input_json")?.clone()),
        })
    }

    fn defer_ephemeral_mutation(&mut self, draft: &PendingMutationDraft) -> bool {
        if !self.store.is_ephemeral() || self.ephemeral_confirmed {
            return false;
        }
        if self.deferred_ephemeral.is_some() {
            self.model.notice = Some("An ephemeral operation is already awaiting consent.".into());
            return true;
        }
        self.deferred_ephemeral = Some(PendingCommand {
            schema_version: 1,
            command_id: draft.command_id.clone(),
            kind: match draft.kind {
                PendingMutationKind::CancelTurn => PendingKind::CancelTurn,
                PendingMutationKind::ContinueTurn => PendingKind::ContinueTurn,
                _ => return true,
            },
            session_id: draft.session_id.clone(),
            turn_id: draft.turn_id.clone(),
            suspension_id: draft.suspension_id.clone(),
            expected_session_version: draft.expected_session_version,
            requested_through_position: draft.requested_through_position,
            request_payload: draft.request_payload.clone(),
            request_digest: String::new(),
            created_at: draft.created_at.clone(),
        });
        self.model.overlay = Some(crate::application::Overlay::EphemeralConfirmation);
        true
    }
}
