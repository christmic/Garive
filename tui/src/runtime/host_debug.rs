use std::fmt;

use super::{HostMessage, HostOperation};

impl fmt::Debug for HostMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SnapshotLoaded {
                request_id,
                items,
                follow_position,
                ..
            } => formatter
                .debug_struct("HostMessage::SnapshotLoaded")
                .field("request_id", request_id)
                .field("item_count", &items.len())
                .field("follow_position", follow_position)
                .finish(),
            Self::SessionCreated { response, .. } => formatter
                .debug_struct("HostMessage::SessionCreated")
                .field("committed_position", &response.committed_position)
                .finish(),
            Self::TurnAccepted { response, .. } => formatter
                .debug_struct("HostMessage::TurnAccepted")
                .field("committed_position", &response.committed_position)
                .finish(),
            Self::Event(event) => formatter
                .debug_struct("HostMessage::Event")
                .field("position", &event.position)
                .finish(),
            Self::LiveOutput(event) => formatter
                .debug_struct("HostMessage::LiveOutput")
                .field("sequence", &event.sequence)
                .field("kind", &live_output_kind_name(&event.kind))
                .finish(),
            Self::FollowEnded { code, .. } => formatter
                .debug_struct("HostMessage::FollowEnded")
                .field("code", &code.wire_name())
                .finish(),
            Self::LiveFollowEnded { code, .. } => formatter
                .debug_struct("HostMessage::LiveFollowEnded")
                .field("code", &code.wire_name())
                .finish(),
            Self::ReconnectDue { attempt, .. } => formatter
                .debug_struct("HostMessage::ReconnectDue")
                .field("attempt", attempt)
                .finish(),
            Self::LiveReconnectDue { attempt, .. } => formatter
                .debug_struct("HostMessage::LiveReconnectDue")
                .field("attempt", attempt)
                .finish(),
            Self::Failed { operation, error } => {
                let mut debug = formatter.debug_struct("HostMessage::Failed");
                match operation {
                    HostOperation::Snapshot { request_id } => {
                        debug
                            .field("operation", &"snapshot")
                            .field("request_id", request_id);
                    }
                    HostOperation::Mutation { .. } => {
                        debug.field("operation", &"mutation");
                    }
                }
                debug.field("code", &error.code.wire_name()).finish()
            }
        }
    }
}

const fn live_output_kind_name(kind: &garive_host_client::LiveOutputEventKind) -> &'static str {
    use garive_host_client::LiveOutputEventKind;

    match kind {
        LiveOutputEventKind::Snapshot { .. } => "snapshot",
        LiveOutputEventKind::TextDelta { .. } => "text_delta",
        LiveOutputEventKind::PhaseChanged { .. } => "phase_changed",
        LiveOutputEventKind::PreviewUnavailable => "preview_unavailable",
        LiveOutputEventKind::Ended { .. } => "ended",
    }
}
