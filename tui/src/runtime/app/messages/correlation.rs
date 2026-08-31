use crate::persistence::{PendingCommand, PendingKind};

pub(super) fn matches_session_created(pending: &[PendingCommand], command_id: &str) -> bool {
    unique_pending(pending, command_id).is_some_and(|pending| {
        pending.kind == PendingKind::CreateSession
            && pending.session_id.is_none()
            && pending.turn_id.is_none()
    })
}

pub(super) fn matches_turn_accepted(
    pending: &[PendingCommand],
    command_id: &str,
    requested_session_id: &str,
    submitted_text: &str,
    response_session_id: &str,
    response_turn_id: &str,
) -> Option<PendingKind> {
    let pending = unique_pending(pending, command_id)?;
    if pending.session_id.as_deref() != Some(requested_session_id)
        || response_session_id != requested_session_id
    {
        return None;
    }
    let matches = match pending.kind {
        PendingKind::CreateSession => false,
        PendingKind::StartTurn => {
            pending.turn_id.is_none()
                && pending
                    .request_payload
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    == Some(submitted_text)
                && !submitted_text.is_empty()
        }
        PendingKind::CancelTurn => {
            pending.turn_id.as_deref() == Some(response_turn_id) && submitted_text.is_empty()
        }
        PendingKind::ContinueTurn => {
            pending.turn_id.as_deref() == Some(response_turn_id)
                && matches_continuation_input(pending, submitted_text)
        }
    };
    matches.then_some(pending.kind)
}

fn matches_continuation_input(pending: &PendingCommand, submitted_text: &str) -> bool {
    if let Some(input) = pending
        .request_payload
        .get("input")
        .and_then(serde_json::Value::as_str)
    {
        return !input.is_empty() && input == submitted_text;
    }
    let Some(input) = pending.request_payload.get("input_json") else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(submitted_text)
        .is_ok_and(|submitted| submitted == *input)
}

pub(super) fn unique_pending<'a>(
    pending: &'a [PendingCommand],
    command_id: &str,
) -> Option<&'a PendingCommand> {
    let mut matches = pending.iter().filter(|item| item.command_id == command_id);
    let pending = matches.next()?;
    matches.next().is_none().then_some(pending)
}

pub(super) fn contains_pending(pending: &[PendingCommand], command_id: &str) -> bool {
    pending.iter().any(|item| item.command_id == command_id)
}
