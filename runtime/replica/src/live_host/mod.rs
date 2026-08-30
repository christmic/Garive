mod http;
mod projection;
mod service;
mod types;

pub use http::{LiveHostServer, LiveHostServerError};
pub use service::LiveHost;
pub use types::{
    AgentDefinitionSummary, CommittedTurn, CreateSessionResponse, HostClock, HostContinuationInput,
    HostEventPage, InstalledAgent, LiveHostError, LiveHostEvent, LiveHostLimits, SessionSummary,
    TurnCommandResponse, TurnDispatchError, TurnDispatcher, TurnTimelineItem, TurnTimelinePage,
};

pub(crate) use projection::{completion_text, project_fact};
pub(crate) use service::validate_key;
pub(crate) use types::{
    CancelTurnBody, ContinueTurnBody, CreateSessionBody, ErrorBody, LiveHostState, StartTurnBody,
};
