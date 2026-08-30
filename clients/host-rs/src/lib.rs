//! Explicit loopback H1 client and ephemeral event reduction for Rust Apps.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod client;
mod reducer;
mod values;

pub use client::LiveHostClient;
pub use reducer::reduce_host_events;
pub use values::{
    AgentDefinitionPage, AgentDefinitionSummary, ClientLimits, CreateSessionResponse,
    HostClientError, HostClientErrorCode, HostEvent, HostTerminal, HostView, SessionPage,
    SessionSummary, SessionView, SuspensionView, TurnCommandResponse, TurnTimelineItem,
    TurnTimelinePage, HOST_CLIENT_FAILURES,
};
