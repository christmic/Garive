use crate::persistence::{PendingCommand, PendingKind};

use super::{ExactRetryOwner, ExactRetryPhase, RuntimeState};

impl RuntimeState {
    pub(in crate::runtime) fn recoverable_pending_for_context(&self) -> Option<&PendingCommand> {
        let selected = self.model.selected_session.as_deref();
        self.pending.iter().find(|pending| {
            pending.session_id.as_deref() == selected
                && self.pending_recovery.contains(&pending.command_id)
        })
    }

    pub(in crate::runtime) fn begin_exact_retry(&mut self, command_id: &str) -> bool {
        if self.exact_retry_owner.is_some() || !self.pending_recovery.contains(command_id) {
            return false;
        }
        self.exact_retry_owner = Some(ExactRetryOwner {
            command_id: command_id.into(),
            phase: ExactRetryPhase::Refreshing,
        });
        true
    }

    pub(in crate::runtime) fn exact_retry_in_progress(&self) -> bool {
        self.exact_retry_owner.is_some()
    }

    pub(in crate::runtime) fn claim_exact_retry_after_refresh(
        &mut self,
        session_id: Option<&str>,
        required_kind: Option<PendingKind>,
    ) -> Option<PendingCommand> {
        let owner = self.exact_retry_owner.as_ref()?;
        if owner.phase != ExactRetryPhase::Refreshing {
            return None;
        }
        let command_id = owner.command_id.clone();
        let mut matches = self.pending.iter().filter(|pending| {
            pending.command_id == command_id
                && pending.session_id.as_deref() == session_id
                && required_kind.is_none_or(|kind| pending.kind == kind)
                && self.pending_recovery.contains(&pending.command_id)
        });
        let pending = matches.next()?.clone();
        if matches.next().is_some() {
            return None;
        }
        self.exact_retry_owner.as_mut()?.phase = ExactRetryPhase::Replayed;
        Some(pending)
    }

    pub(in crate::runtime) fn cancel_exact_retry_refresh(&mut self) -> bool {
        if self
            .exact_retry_owner
            .as_ref()
            .is_some_and(|owner| owner.phase == ExactRetryPhase::Refreshing)
        {
            self.exact_retry_owner = None;
            true
        } else {
            false
        }
    }

    pub(super) fn clear_exact_retry_owner(&mut self, command_id: &str) {
        if self
            .exact_retry_owner
            .as_ref()
            .is_some_and(|owner| owner.command_id == command_id)
        {
            self.exact_retry_owner = None;
        }
    }

    #[cfg(test)]
    pub(in crate::runtime) fn exact_retry_was_replayed(&self) -> bool {
        self.exact_retry_owner
            .as_ref()
            .is_some_and(|owner| owner.phase == ExactRetryPhase::Replayed)
    }
}
