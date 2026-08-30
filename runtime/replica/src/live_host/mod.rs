mod activity_projection;
mod http;
mod projection;
mod read_cursor;
mod read_model;
mod service;
mod timeline_projection;
mod timeline_prompt;
mod types;

pub use http::{LiveHostServer, LiveHostServerError};
pub use service::LiveHost;
pub use types::{
    ActivityProjectionLimits, AgentDefinitionPageV1, AgentDefinitionSummary,
    AgentDefinitionSummaryV1, CommittedTurn, CreateSessionResponse, HostActivity, HostArtifact,
    HostArtifactPage, HostClock, HostContinuationInput, HostEventPage, HostReadLimits,
    HostWorkspaceAttachment, HostWorkspaceContextEntry, HostWorkspaceDetachment,
    InstalledActivityCatalogue, InstalledActivityDescriptor, InstalledAgent, LiveHostError,
    LiveHostEvent, LiveHostLimits, SessionPageV1, SessionSummary, SessionSummaryV1, SessionViewV1,
    SuspensionViewV1, TurnCommandResponse, TurnDispatchError, TurnDispatcher, TurnSuspensionView,
    TurnTimelineItem, TurnTimelineItemV1, TurnTimelinePage, TurnTimelinePageV1,
};

pub(crate) use activity_projection::project_activities;
pub(crate) use projection::{completion_text, project_fact};
pub(crate) use service::validate_key;
pub(crate) use types::{
    CancelTurnBody, ContinueTurnBody, CreateSessionBody, ErrorBody, LiveHostState, StartTurnBody,
};
