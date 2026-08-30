mod activity_projection;
mod http;
mod projection;
mod service;
mod types;

pub use http::{LiveHostServer, LiveHostServerError};
pub use service::LiveHost;
pub use types::{
    ActivityProjectionLimits, AgentDefinitionPage, AgentDefinitionSummary, CommittedTurn,
    CreateSessionResponse, HostActivity, HostClock, HostContinuationInput, HostEventPage,
    InstalledActivityCatalogue, InstalledActivityDescriptor, InstalledAgent, LiveHostError,
    LiveHostEvent, LiveHostLimits, SessionPage, SessionSummary, SessionView, TurnCommandResponse,
    TurnDispatchError, TurnDispatcher, TurnSuspensionView, TurnTimelineItem, TurnTimelinePage,
};

pub(crate) use activity_projection::project_activities;
pub(crate) use projection::{completion_text, project_fact};
pub(crate) use service::validate_key;
pub(crate) use types::{
    CancelTurnBody, ContinueTurnBody, CreateSessionBody, ErrorBody, LiveHostState,
    MobileWakeObservation, MobileWakePage, StartTurnBody,
};
