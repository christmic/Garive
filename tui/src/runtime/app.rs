use std::io;

use crossterm::event::EventStream;
use futures::StreamExt;
use garive_host_client::{
    ClientLimits, HostClientErrorCode, HostEvent, LiveHostClient, TurnTimelineItem,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    application::{
        AppModel, BootState, ConnectionState, ExecutionState, Overlay, TerminalSize, TimelineItem,
        TimelineRole,
    },
    persistence::{
        now, PendingCommand, PendingKind, Preferences, PromptHistoryEntry, StateError, StateStore,
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
    let store =
        StateStore::open(config.state_dir.clone(), config.ephemeral).map_err(map_state_error)?;
    let preferences = store.load_preferences().map_err(map_state_error)?;
    let pending = store.load_any_pending().map_err(map_state_error)?;
    let mut config = config;
    if config.theme == crate::Theme::System {
        config.theme = preferences.theme;
    }
    if config.mouse == crate::MouseMode::Auto {
        config.mouse = preferences.mouse;
    }
    if !config.reduced_motion {
        config.reduced_motion = preferences.reduced_motion;
    }
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
    let mut state = RuntimeState::new(config, client, sender, store, preferences, pending);
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
    state.persist_presentation();
    guard.restore().map_err(map_terminal_error)
}

pub(super) struct RuntimeState {
    pub(super) config: LaunchConfig,
    pub(super) client: LiveHostClient,
    pub(super) sender: mpsc::Sender<HostMessage>,
    pub(super) model: AppModel,
    pub(super) follow: Option<JoinHandle<()>>,
    pub(super) store: StateStore,
    pub(super) preferences: Preferences,
    pub(super) pending: Option<PendingCommand>,
    pub(super) ephemeral_confirmed: bool,
}

impl RuntimeState {
    fn new(
        config: LaunchConfig,
        client: LiveHostClient,
        sender: mpsc::Sender<HostMessage>,
        store: StateStore,
        preferences: Preferences,
        pending: Option<PendingCommand>,
    ) -> Self {
        let mut model = AppModel::default();
        model.boot = BootState::Loading;
        model.connection = ConnectionState::Connecting;
        if pending.is_some() {
            model.notice = Some("A prior command has an unknown durable outcome. Use /retry after reviewing status.".into());
            model.overlay = Some(Overlay::UnknownCommand);
        }
        Self {
            config,
            client,
            sender,
            model,
            follow: None,
            store,
            preferences,
            pending,
            ephemeral_confirmed: false,
        }
    }

    pub(super) fn command_id(&mut self, _operation: &str) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    pub(super) fn load(&mut self, session_id: String) {
        if let Some(task) = self.follow.take() {
            task.abort();
        }
        self.persist_presentation();
        self.model.selected_session = Some(session_id.clone());
        self.preferences.selected_session_id = Some(session_id.clone());
        let draft = self
            .preferences
            .draft(&session_id)
            .unwrap_or_default()
            .to_owned();
        let _ = self.model.composer.replace(&draft);
        self.model.connection = ConnectionState::Connecting;
        host::load_snapshot(self.client.clone(), session_id, self.sender.clone());
    }

    pub(super) fn persist_presentation(&mut self) {
        if let Some(session) = self.model.selected_session.as_deref() {
            self.preferences
                .set_draft(session, self.model.composer.text());
            self.preferences.selected_session_id = Some(session.into());
        }
        self.preferences.theme = self.config.theme;
        self.preferences.mouse = self.config.mouse;
        self.preferences.reduced_motion = self.config.reduced_motion;
        if let Err(error) = self.store.save_preferences(&self.preferences) {
            self.model.notice = Some(format!("Local state: {}", state_error_name(error)));
        }
    }

    pub(super) fn admit_pending(&mut self, pending: PendingCommand) -> bool {
        if self.store.is_ephemeral() && !self.ephemeral_confirmed {
            self.model.overlay = Some(Overlay::EphemeralConfirmation);
            return false;
        }
        if self.pending.is_some() {
            self.model.notice =
                Some("Another command has an unknown durable outcome. Use /retry first.".into());
            self.model.overlay = Some(Overlay::UnknownCommand);
            return false;
        }
        let Ok(pending) = pending.seal() else {
            self.local_state_failure("invalid_pending_command");
            return false;
        };
        if self.store.save_pending(&pending).is_err() {
            self.local_state_failure("pending_write_failed");
            return false;
        }
        self.pending = Some(pending);
        true
    }

    pub(super) fn abandon_pending(&mut self) {
        if let Some(pending) = self.pending.take() {
            if self
                .store
                .remove_pending(pending.session_id.as_deref())
                .is_err()
            {
                self.pending = Some(pending);
                self.local_state_failure("pending_abandon_failed");
                return;
            }
        }
        self.model.overlay = None;
        if let Some(session) = self.model.selected_session.clone() {
            self.load(session);
        }
    }

    fn finish_pending(&mut self, submitted_text: &str) {
        let Some(pending) = self.pending.clone() else {
            return;
        };
        if self
            .store
            .remove_pending(pending.session_id.as_deref())
            .is_err()
        {
            self.local_state_failure("pending_remove_failed");
            return;
        }
        if !self.config.no_prompt_history
            && !submitted_text.is_empty()
            && matches!(
                pending.kind,
                PendingKind::StartTurn | PendingKind::ContinueTurn
            )
        {
            if let Some(session_id) = pending.session_id.clone() {
                let entry = PromptHistoryEntry {
                    schema_version: 1,
                    entry_id: uuid::Uuid::new_v4().to_string(),
                    session_id,
                    submitted_text: submitted_text.into(),
                    submitted_at: now(),
                };
                if self.store.append_history(&entry).is_err() {
                    self.model.notice = Some("Prompt history could not be saved.".into());
                }
            }
        }
        self.pending = None;
    }

    fn reject_pending(&mut self, code: HostClientErrorCode) {
        if matches!(
            code,
            HostClientErrorCode::HostFailure | HostClientErrorCode::InvalidCommand
        ) {
            if let Some(pending) = self.pending.take() {
                let _ = self.store.remove_pending(pending.session_id.as_deref());
            }
        } else if self.pending.is_some() {
            self.model.notice =
                Some("The command outcome is unknown. Review /status or use exact /retry.".into());
            self.model.overlay = Some(Overlay::UnknownCommand);
        }
    }

    fn local_state_failure(&mut self, code: &str) {
        self.model.notice = Some(format!("Local recovery state: {code}"));
        self.model.overlay = Some(Overlay::ErrorDetails);
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
            if let Some(cursor) =
                view::render(&state.model, state.config.theme, area, frame.buffer_mut())
            {
                frame.set_cursor_position(cursor);
            }
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
            let selected = state
                .config
                .session
                .clone()
                .or_else(|| state.preferences.selected_session_id.clone())
                .or_else(|| {
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
            state.finish_pending("");
            state.model.composer.clear();
            state.load(response.session_id);
            host::bootstrap(state.client.clone(), state.sender.clone());
        }
        HostMessage::TurnAccepted {
            session_id,
            submitted_text,
            response,
        } => {
            state.finish_pending(&submitted_text);
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
        HostMessage::Failed(error) => {
            let code = error.code;
            state.reject_pending(code);
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
    model.scroll_offset = model.timeline.len().saturating_sub(10);
}

fn map_terminal_error(error: TerminalError) -> TuiError {
    match error {
        TerminalError::NotATerminal => TuiError::TerminalUnavailable,
        TerminalError::Setup | TerminalError::Restore => TuiError::TerminalIo,
    }
}

fn map_state_error(_: StateError) -> TuiError {
    TuiError::LocalState
}

fn state_error_name(error: StateError) -> &'static str {
    match error {
        StateError::Unavailable => "unavailable",
        StateError::UnsafePermissions => "unsafe_permissions",
        StateError::InvalidData => "invalid_data",
        StateError::Conflict => "conflict",
    }
}
