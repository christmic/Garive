//! Explicit loopback H1 client and ephemeral event reduction for Rust Apps.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod client;
mod reducer;
mod values;

pub use client::{ApprovalDecision, LiveHostClient};
pub use reducer::reduce_host_events;
pub use values::{
    AgentDefinitionPage, AgentDefinitionSummary, ClientLimits, CreateSessionResponse,
    DeliveredTurnResponse, GoalCommandResponse, GoalPage, GoalSummary, HostActivity,
    HostClientError, HostClientErrorCode, HostEvent, HostTerminal, HostView, LiveOutputEndReason,
    LiveOutputEvent, LiveOutputEventKind, PlanPage, PlanSummary, SessionMember, SessionMembership,
    SessionPage, SessionSummary, SessionView, StartTurnsResponse, SuspensionView,
    TurnCommandResponse, TurnTimelineItem, TurnTimelinePage, HOST_CLIENT_FAILURES,
};
