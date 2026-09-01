use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CancelRequestPhase {
    Requesting,
    AwaitingTerminal,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CancelRequest {
    pub(crate) command_id: Option<String>,
    pub(crate) session_id: String,
    pub(crate) turn_id: String,
    pub(crate) phase: CancelRequestPhase,
}

#[derive(Debug, Default)]
pub(crate) struct CancelRequests {
    by_session: BTreeMap<String, CancelRequest>,
}

impl CancelRequests {
    pub(crate) fn begin(&mut self, command_id: String, session_id: String, turn_id: String) {
        self.by_session.insert(
            session_id.clone(),
            CancelRequest {
                command_id: Some(command_id),
                session_id,
                turn_id,
                phase: CancelRequestPhase::Requesting,
            },
        );
    }

    pub(crate) fn selected(
        &self,
        session_id: Option<&str>,
        turn_id: Option<&str>,
    ) -> Option<&CancelRequest> {
        let request = self.by_session.get(session_id?)?;
        (Some(request.turn_id.as_str()) == turn_id).then_some(request)
    }

    pub(crate) fn mark_accepted(&mut self, command_id: &str) {
        if let Some(request) = self.by_command_mut(command_id) {
            request.phase = CancelRequestPhase::AwaitingTerminal;
        }
    }

    pub(crate) fn restore_accepted(&mut self, session_id: String, turn_id: String) {
        match self.by_session.entry(session_id.clone()) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                if entry.get().turn_id == turn_id {
                    entry.get_mut().phase = CancelRequestPhase::AwaitingTerminal;
                } else {
                    entry.insert(CancelRequest {
                        command_id: None,
                        session_id,
                        turn_id,
                        phase: CancelRequestPhase::AwaitingTerminal,
                    });
                }
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(CancelRequest {
                    command_id: None,
                    session_id,
                    turn_id,
                    phase: CancelRequestPhase::AwaitingTerminal,
                });
            }
        }
    }

    pub(crate) fn mark_unknown(&mut self, command_id: &str) {
        if let Some(request) = self.by_command_mut(command_id) {
            request.phase = CancelRequestPhase::OutcomeUnknown;
        }
    }

    pub(crate) fn clear_command(&mut self, command_id: &str) {
        self.by_session
            .retain(|_, request| request.command_id.as_deref() != Some(command_id));
    }

    pub(crate) fn clear_terminal(&mut self, session_id: &str, turn_id: &str) {
        if self
            .by_session
            .get(session_id)
            .is_some_and(|request| request.turn_id == turn_id)
        {
            self.by_session.remove(session_id);
        }
    }

    fn by_command_mut(&mut self, command_id: &str) -> Option<&mut CancelRequest> {
        self.by_session
            .values_mut()
            .find(|request| request.command_id.as_deref() == Some(command_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_command_and_terminal_advance_or_clear_only_their_request() {
        let mut requests = CancelRequests::default();
        requests.begin("command-a".into(), "session-a".into(), "turn-a".into());
        requests.begin("command-b".into(), "session-b".into(), "turn-b".into());

        requests.mark_accepted("command-a");
        assert_eq!(
            requests
                .selected(Some("session-a"), Some("turn-a"))
                .map(|request| request.phase),
            Some(CancelRequestPhase::AwaitingTerminal)
        );
        requests.clear_terminal("session-a", "other-turn");
        assert!(requests
            .selected(Some("session-a"), Some("turn-a"))
            .is_some());
        requests.clear_terminal("session-a", "turn-a");
        assert!(requests
            .selected(Some("session-a"), Some("turn-a"))
            .is_none());
        assert!(requests
            .selected(Some("session-b"), Some("turn-b"))
            .is_some());

        requests.restore_accepted("session-b".into(), "turn-new".into());
        assert!(requests
            .selected(Some("session-b"), Some("turn-b"))
            .is_none());
        assert_eq!(
            requests
                .selected(Some("session-b"), Some("turn-new"))
                .map(|request| (&request.command_id, request.phase)),
            Some((&None, CancelRequestPhase::AwaitingTerminal))
        );
    }
}
