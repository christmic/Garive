use garive_host_client::{
    AgentDefinitionSummary, CreateSessionResponse, HostClientErrorCode, HostEvent, LiveHostClient,
    SessionSummary, SessionView, TurnCommandResponse, TurnTimelineItem,
};
use serde_json::Value;
use tokio::{sync::mpsc, task::JoinHandle};

pub(crate) const PAGE_LIMIT: usize = 100;

pub(crate) struct ContinuationRequest {
    pub(crate) command_id: String,
    pub(crate) session_id: String,
    pub(crate) turn_id: String,
    pub(crate) suspension_id: String,
    pub(crate) expected_session_version: u64,
    pub(crate) input: ContinuationInput,
}

pub(crate) enum ContinuationInput {
    Text(String),
    Json(Value),
}

#[derive(Debug)]
pub(crate) enum HostMessage {
    Bootstrapped {
        definitions: Vec<AgentDefinitionSummary>,
        sessions: Vec<SessionSummary>,
    },
    SnapshotLoaded {
        request_id: u64,
        session_id: String,
        view: SessionView,
        items: Vec<TurnTimelineItem>,
        follow_position: u64,
    },
    SessionCreated {
        command_id: String,
        response: CreateSessionResponse,
    },
    TurnAccepted {
        command_id: String,
        session_id: String,
        submitted_text: String,
        response: TurnCommandResponse,
    },
    Event(HostEvent),
    FollowEnded {
        session_id: String,
        code: HostClientErrorCode,
    },
    ReconnectDue {
        session_id: String,
        attempt: u32,
    },
    Failed {
        operation: HostOperation,
        error: garive_host_client::HostClientError,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HostOperation {
    Bootstrap,
    Snapshot { request_id: u64 },
    Mutation { command_id: String },
}

pub(crate) fn bootstrap(client: LiveHostClient, sender: mpsc::Sender<HostMessage>) {
    tokio::spawn(async move {
        let result = async {
            let definitions = client.list_agent_definitions().await?.definitions;
            let sessions = client.list_sessions(PAGE_LIMIT, None).await?.sessions;
            Ok::<_, garive_host_client::HostClientError>((definitions, sessions))
        }
        .await;
        let message = match result {
            Ok((definitions, sessions)) => HostMessage::Bootstrapped {
                definitions,
                sessions,
            },
            Err(error) => HostMessage::Failed {
                operation: HostOperation::Bootstrap,
                error,
            },
        };
        let _ = sender.send(message).await;
    });
}

pub(crate) fn load_snapshot(
    client: LiveHostClient,
    request_id: u64,
    session_id: String,
    sender: mpsc::Sender<HostMessage>,
) {
    tokio::spawn(async move {
        let result = async {
            let view = client.get_session(&session_id).await?;
            let mut after = 0;
            let mut items = Vec::new();
            let follow_position = loop {
                let page = client.get_timeline(&session_id, after, PAGE_LIMIT).await?;
                after = page.scanned_through_position;
                items.extend(page.items);
                if !page.has_more {
                    break page.observed_max_position;
                }
            };
            Ok::<_, garive_host_client::HostClientError>((view, items, follow_position))
        }
        .await;
        let message = match result {
            Ok((view, items, follow_position)) => HostMessage::SnapshotLoaded {
                request_id,
                session_id,
                view,
                items,
                follow_position,
            },
            Err(error) => HostMessage::Failed {
                operation: HostOperation::Snapshot { request_id },
                error,
            },
        };
        let _ = sender.send(message).await;
    });
}

pub(crate) fn cancel_turn(
    client: LiveHostClient,
    command_id: String,
    session_id: String,
    turn_id: String,
    requested_through_position: u64,
    sender: mpsc::Sender<HostMessage>,
) {
    tokio::spawn(async move {
        let message = match client
            .cancel_turn(
                &command_id,
                &session_id,
                &turn_id,
                requested_through_position,
            )
            .await
        {
            Ok(response) => HostMessage::TurnAccepted {
                command_id: command_id.clone(),
                session_id,
                submitted_text: String::new(),
                response,
            },
            Err(error) => HostMessage::Failed {
                operation: HostOperation::Mutation { command_id },
                error,
            },
        };
        let _ = sender.send(message).await;
    });
}

pub(crate) fn continue_turn(
    client: LiveHostClient,
    request: ContinuationRequest,
    sender: mpsc::Sender<HostMessage>,
) {
    tokio::spawn(async move {
        let result = match &request.input {
            ContinuationInput::Text(input) => {
                client
                    .continue_turn(
                        &request.command_id,
                        &request.session_id,
                        &request.turn_id,
                        &request.suspension_id,
                        request.expected_session_version,
                        input,
                    )
                    .await
            }
            ContinuationInput::Json(input) => {
                client
                    .continue_turn_json(
                        &request.command_id,
                        &request.session_id,
                        &request.turn_id,
                        &request.suspension_id,
                        request.expected_session_version,
                        input,
                    )
                    .await
            }
        };
        let submitted_text = match request.input {
            ContinuationInput::Text(value) => value,
            ContinuationInput::Json(value) => value.to_string(),
        };
        let message = match result {
            Ok(response) => HostMessage::TurnAccepted {
                command_id: request.command_id.clone(),
                session_id: request.session_id,
                submitted_text,
                response,
            },
            Err(error) => HostMessage::Failed {
                operation: HostOperation::Mutation {
                    command_id: request.command_id,
                },
                error,
            },
        };
        let _ = sender.send(message).await;
    });
}

pub(crate) fn create_session(
    client: LiveHostClient,
    command_id: String,
    definition_id: String,
    sender: mpsc::Sender<HostMessage>,
) {
    tokio::spawn(async move {
        let message = match client.create_session(&command_id, &definition_id).await {
            Ok(response) => HostMessage::SessionCreated {
                command_id: command_id.clone(),
                response,
            },
            Err(error) => HostMessage::Failed {
                operation: HostOperation::Mutation { command_id },
                error,
            },
        };
        let _ = sender.send(message).await;
    });
}

pub(crate) fn start_turn(
    client: LiveHostClient,
    command_id: String,
    session_id: String,
    text: String,
    sender: mpsc::Sender<HostMessage>,
) {
    tokio::spawn(async move {
        let message = match client.start_turn(&command_id, &session_id, &text).await {
            Ok(response) => HostMessage::TurnAccepted {
                command_id: command_id.clone(),
                session_id,
                submitted_text: text,
                response,
            },
            Err(error) => HostMessage::Failed {
                operation: HostOperation::Mutation { command_id },
                error,
            },
        };
        let _ = sender.send(message).await;
    });
}

pub(crate) fn follow(
    client: LiveHostClient,
    session_id: String,
    after_position: u64,
    sender: mpsc::Sender<HostMessage>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let (event_sender, mut events) = mpsc::channel(256);
        let follow = tokio::spawn({
            let client = client.clone();
            let session_id = session_id.clone();
            async move {
                client
                    .follow_events(&session_id, after_position, event_sender)
                    .await
            }
        });
        while let Some(event) = events.recv().await {
            if sender.send(HostMessage::Event(event)).await.is_err() {
                follow.abort();
                return;
            }
        }
        let code = match follow.await {
            Ok(Err(error)) => error.code,
            Ok(Ok(())) | Err(_) => HostClientErrorCode::TransportFailure,
        };
        let _ = sender
            .send(HostMessage::FollowEnded { session_id, code })
            .await;
    })
}

pub(crate) fn schedule_reconnect(
    session_id: String,
    attempt: u32,
    sender: mpsc::Sender<HostMessage>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let delay_ms = match attempt {
            1 => 250,
            2 => 500,
            3 => 1_000,
            4 => 2_000,
            _ => 4_000,
        };
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        let _ = sender
            .send(HostMessage::ReconnectDue {
                session_id,
                attempt,
            })
            .await;
    })
}
