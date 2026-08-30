use garive_host_client::{
    AgentDefinitionSummary, CreateSessionResponse, HostClientErrorCode, HostEvent, LiveHostClient,
    SessionSummary, SessionView, TurnCommandResponse, TurnTimelineItem,
};
use tokio::{sync::mpsc, task::JoinHandle};

pub(crate) const PAGE_LIMIT: usize = 100;

#[derive(Debug)]
pub(crate) enum HostMessage {
    Bootstrapped {
        definitions: Vec<AgentDefinitionSummary>,
        sessions: Vec<SessionSummary>,
    },
    SnapshotLoaded {
        session_id: String,
        view: SessionView,
        items: Vec<TurnTimelineItem>,
        follow_position: u64,
    },
    SessionCreated(CreateSessionResponse),
    TurnAccepted {
        session_id: String,
        submitted_text: String,
        response: TurnCommandResponse,
    },
    Event(HostEvent),
    FollowEnded {
        session_id: String,
        code: HostClientErrorCode,
    },
    Failed(HostClientErrorCode),
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
            Err(error) => HostMessage::Failed(error.code),
        };
        let _ = sender.send(message).await;
    });
}

pub(crate) fn load_snapshot(
    client: LiveHostClient,
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
                session_id,
                view,
                items,
                follow_position,
            },
            Err(error) => HostMessage::Failed(error.code),
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
                session_id,
                submitted_text: String::new(),
                response,
            },
            Err(error) => HostMessage::Failed(error.code),
        };
        let _ = sender.send(message).await;
    });
}

pub(crate) fn continue_turn(
    client: LiveHostClient,
    command_id: String,
    session_id: String,
    turn_id: String,
    suspension_id: String,
    expected_session_version: u64,
    input: String,
    sender: mpsc::Sender<HostMessage>,
) {
    tokio::spawn(async move {
        let message = match client
            .continue_turn(
                &command_id,
                &session_id,
                &turn_id,
                &suspension_id,
                expected_session_version,
                &input,
            )
            .await
        {
            Ok(response) => HostMessage::TurnAccepted {
                session_id,
                submitted_text: input,
                response,
            },
            Err(error) => HostMessage::Failed(error.code),
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
            Ok(value) => HostMessage::SessionCreated(value),
            Err(error) => HostMessage::Failed(error.code),
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
                session_id,
                submitted_text: text,
                response,
            },
            Err(error) => HostMessage::Failed(error.code),
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
