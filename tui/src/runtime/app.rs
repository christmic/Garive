use std::{
    io,
    time::{SystemTime, UNIX_EPOCH},
};

use crossterm::event::EventStream;
use futures::StreamExt;
use garive_host_client::{ClientLimits, HostEvent, LiveHostClient, TurnTimelineItem};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    application::{
        AppModel, BootState, ConnectionState, ExecutionState, Overlay, TerminalSize, TimelineItem,
        TimelineRole,
    },
    view, LaunchConfig, TuiError,
};

use super::{
    controller::handle_terminal,
    host::{self, HostMessage},
    SystemTerminal, TerminalError, TerminalGuard, TerminalOptions,
};

const LIMITS: ClientLimits = ClientLimits {
    max_command_bytes: 4_096,
    max_event_bytes: 32_768,
    max_events: 16_384,
    follow_deadline_ms: 120_000,
};

/// Runs the resident full-screen terminal client until the user confirms exit.
pub async fn run(config: LaunchConfig) -> Result<(), TuiError> {
    let client = LiveHostClient::new(&config.host, LIMITS).map_err(|_| TuiError::InvalidHost)?;
    let mut guard = TerminalGuard::acquire(
        SystemTerminal::default(),
        TerminalOptions {
            screen_reader: config.screen_reader,
        },
    )
    .map_err(map_terminal_error)?;
    let mut terminal =
        Terminal::new(CrosstermBackend::new(io::stderr())).map_err(|_| TuiError::TerminalIo)?;
    terminal.clear().map_err(|_| TuiError::TerminalIo)?;

    let (sender, mut receiver) = mpsc::channel(256);
    let mut state = RuntimeState::new(config, client, sender);
    host::bootstrap(state.client.clone(), state.sender.clone());
    let mut events = EventStream::new();
    loop {
        draw(&mut terminal, &mut state)?;
        if state.model.quit_requested {
            break;
        }
        tokio::select! {
            event = events.next() => match event {
                Some(Ok(event)) => handle_terminal(event, &mut state),
                Some(Err(_)) | None => return Err(TuiError::TerminalIo),
            },
            message = receiver.recv() => match message {
                Some(message) => handle_host(message, &mut state),
                None => break,
            },
        }
    }
    if let Some(task) = state.follow.take() {
        task.abort();
    }
    guard.restore().map_err(map_terminal_error)
}

pub(super) struct RuntimeState {
    pub(super) config: LaunchConfig,
    pub(super) client: LiveHostClient,
    pub(super) sender: mpsc::Sender<HostMessage>,
    pub(super) model: AppModel,
    pub(super) follow: Option<JoinHandle<()>>,
    pub(super) serial: u64,
}

impl RuntimeState {
    fn new(
        config: LaunchConfig,
        client: LiveHostClient,
        sender: mpsc::Sender<HostMessage>,
    ) -> Self {
        let mut model = AppModel::default();
        model.boot = BootState::Loading;
        model.connection = ConnectionState::Connecting;
        Self {
            config,
            client,
            sender,
            model,
            follow: None,
            serial: 0,
        }
    }

    pub(super) fn command_id(&mut self, operation: &str) -> String {
        self.serial = self.serial.saturating_add(1);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!(
            "tui-{operation}-{}-{nanos}-{}",
            std::process::id(),
            self.serial
        )
    }

    pub(super) fn load(&mut self, session_id: String) {
        if let Some(task) = self.follow.take() {
            task.abort();
        }
        self.model.selected_session = Some(session_id.clone());
        self.model.connection = ConnectionState::Connecting;
        host::load_snapshot(self.client.clone(), session_id, self.sender.clone());
    }
}

fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
    state: &mut RuntimeState,
) -> Result<(), TuiError> {
    terminal
        .draw(|frame| {
            let area = frame.area();
            state.model.terminal_size = TerminalSize {
                width: area.width,
                height: area.height,
            };
            view::render(&state.model, state.config.theme, area, frame.buffer_mut());
        })
        .map(|_| ())
        .map_err(|_| TuiError::TerminalIo)
}

fn handle_host(message: HostMessage, state: &mut RuntimeState) {
    match message {
        HostMessage::Bootstrapped {
            definitions,
            sessions,
        } => {
            state.model.definition_count = definitions.len();
            state.model.definitions = definitions;
            state.model.session_count = sessions.len();
            state.model.sessions = sessions;
            state.model.boot = if state.model.definition_count == 0 {
                BootState::NotConfigured
            } else {
                BootState::Ready
            };
            state.model.connection = ConnectionState::Online;
            let selected = state.config.session.clone().or_else(|| {
                state
                    .model
                    .sessions
                    .first()
                    .map(|item| item.session_id.clone())
            });
            if let Some(id) = selected {
                state.load(id);
            }
        }
        HostMessage::SnapshotLoaded {
            session_id,
            view,
            items,
            follow_position,
        } if state.model.selected_session.as_deref() == Some(&session_id) => {
            install_timeline(&mut state.model, items);
            state.model.observed_position = follow_position;
            state.model.connection = ConnectionState::Online;
            state.model.session_count = state.model.sessions.len();
            if let Some(summary) = state
                .model
                .sessions
                .iter_mut()
                .find(|item| item.session_id == session_id)
            {
                *summary = view.session;
            }
            state.follow = Some(host::follow(
                state.client.clone(),
                session_id,
                follow_position,
                state.sender.clone(),
            ));
        }
        HostMessage::SnapshotLoaded { .. } => {}
        HostMessage::SessionCreated(response) => {
            state.model.composer.clear();
            state.load(response.session_id);
            host::bootstrap(state.client.clone(), state.sender.clone());
        }
        HostMessage::TurnAccepted {
            session_id,
            submitted_text,
            response,
        } => {
            if !submitted_text.is_empty() {
                state.model.composer.clear();
            }
            state.model.selected_turn = Some(response.turn_id);
            state.model.execution = ExecutionState::Following;
            state.load(session_id);
        }
        HostMessage::Event(event) => apply_event(event, state),
        HostMessage::FollowEnded {
            session_id,
            code: _,
        } if state.model.selected_session.as_deref() == Some(&session_id) => {
            state.model.connection = ConnectionState::Disconnected { attempt: 1 };
        }
        HostMessage::FollowEnded { .. } => {}
        HostMessage::Failed(code) => {
            state.model.connection = ConnectionState::Unavailable {
                safe_code: code.wire_name(),
            };
            state.model.boot = BootState::Degraded;
            state.model.execution = ExecutionState::Failed;
        }
    }
}

fn apply_event(event: HostEvent, state: &mut RuntimeState) {
    if state.model.selected_session.as_deref() != Some(&event.session_id)
        || event.position <= state.model.observed_position
    {
        return;
    }
    state.model.observed_position = event.position;
    state.model.connection = ConnectionState::Online;
    if !event.turn_id.is_empty() {
        state.model.selected_turn = Some(event.turn_id.clone());
    }
    if matches!(
        event.event.as_str(),
        "turn.completed" | "turn.failed" | "turn.stopped" | "turn.suspended"
    ) {
        let session = event.session_id;
        state.load(session);
    }
}

fn install_timeline(model: &mut AppModel, mut turns: Vec<TurnTimelineItem>) {
    turns.sort_by_key(|turn| turn.started_position);
    model.timeline.clear();
    model.suspension = None;
    model.selected_turn = None;
    model.execution = ExecutionState::Idle;
    for turn in turns {
        model.timeline.push(TimelineItem {
            position: turn.started_position,
            role: TimelineRole::User,
            text: turn.user_text,
        });
        for activity in turn.activities {
            model.timeline.push(TimelineItem {
                position: activity.source_position,
                role: TimelineRole::Status,
                text: format!("{} · {}", activity.label_key, activity.state),
            });
        }
        if let Some(text) = turn.completion_text {
            model.timeline.push(TimelineItem {
                position: turn.latest_position,
                role: TimelineRole::Agent,
                text,
            });
        }
        model.selected_turn = Some(turn.turn_id);
        model.execution = match turn.state.as_str() {
            "started" | "running" => ExecutionState::Following,
            "suspended" => ExecutionState::Suspended,
            "failed" => ExecutionState::Failed,
            _ => ExecutionState::Idle,
        };
        if turn.suspension.is_some() {
            model.suspension = turn.suspension;
            model.overlay = Some(Overlay::Suspension);
        }
    }
}

fn map_terminal_error(error: TerminalError) -> TuiError {
    match error {
        TerminalError::NotATerminal => TuiError::TerminalUnavailable,
        TerminalError::Setup | TerminalError::Restore => TuiError::TerminalIo,
    }
}
