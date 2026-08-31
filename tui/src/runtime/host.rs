use garive_host_client::{
    CreateSessionResponse, HostClientErrorCode, HostEvent, LiveHostClient, LiveOutputEvent,
    TurnCommandResponse,
};
use serde_json::Value;
use tokio::{sync::mpsc, task::JoinHandle};

#[path = "host_debug.rs"]
mod host_debug;

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

pub(crate) enum HostMessage {
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
    LiveOutput(LiveOutputEvent),
    FollowEnded {
        session_id: String,
        code: HostClientErrorCode,
    },
    LiveFollowEnded {
        session_id: String,
        code: HostClientErrorCode,
    },
    ReconnectDue {
        session_id: String,
        attempt: u32,
    },
    LiveReconnectDue {
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
    Mutation { command_id: String },
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
        let canonical_json = match &request.input {
            ContinuationInput::Json(input) => {
                Some(serde_jcs::to_string(input).unwrap_or_else(|_| input.to_string()))
            }
            ContinuationInput::Text(_) => None,
        };
        let result = match (&request.input, canonical_json.as_deref()) {
            (ContinuationInput::Text(input), _) => {
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
            (ContinuationInput::Json(_), Some(input)) => {
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
            (ContinuationInput::Json(_), None) => unreachable!("JSON input always has text"),
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

pub(crate) fn follow_live(
    client: LiveHostClient,
    session_id: String,
    sender: mpsc::Sender<HostMessage>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let (event_sender, mut events) = mpsc::channel(256);
        let follow = tokio::spawn({
            let client = client.clone();
            let session_id = session_id.clone();
            async move { client.follow_live_output(&session_id, event_sender).await }
        });
        while let Some(event) = events.recv().await {
            if sender.send(HostMessage::LiveOutput(event)).await.is_err() {
                follow.abort();
                return;
            }
        }
        let code = match follow.await {
            Ok(Err(error)) => error.code,
            Ok(Ok(())) | Err(_) => HostClientErrorCode::TransportFailure,
        };
        let _ = sender
            .send(HostMessage::LiveFollowEnded { session_id, code })
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

pub(crate) fn schedule_live_reconnect(
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
            .send(HostMessage::LiveReconnectDue {
                session_id,
                attempt,
            })
            .await;
    })
}

#[cfg(test)]
#[path = "host_debug_tests.rs"]
mod tests;
