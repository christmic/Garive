use std::io::{self, Write};

use crossterm::event::EventStream;
use futures::StreamExt;
use garive_host_client::{
    ClientLimits, HostClientErrorCode, HostEvent, LiveHostClient, TurnTimelineItem,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use serde_json::json;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    application::{
        reduce, AppAction, AppModel, ConnectionState, EffectKind, ExecutionState, Overlay,
        TerminalSize, TimelineItem, TimelineRole,
    },
    persistence::{
        now, PendingCommand, PendingKind, Preferences, PromptHistoryEntry, StateError, StateStore,
    },
    view, LaunchConfig, TuiError,
};

use super::{
    controller::handle_terminal,
    host::{self, HostMessage, HostOperation},
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
    let (pending, pending_quarantined) = store.load_pending().map_err(map_state_error)?;
    let (history, history_error) = if config.no_prompt_history {
        (Vec::new(), false)
    } else {
        match store.load_history() {
            Ok(value) => (value, false),
            Err(_) => (Vec::new(), true),
        }
    };
    let mut config = config;
    if !config.theme_explicit {
        config.theme = preferences.theme;
    }
    if !config.mouse_explicit {
        config.mouse = preferences.mouse;
    }
    if !config.reduced_motion_explicit {
        config.reduced_motion = preferences.reduced_motion;
    }
    let client = LiveHostClient::new(&config.host, LIMITS).map_err(|_| TuiError::InvalidHost)?;
    let restored = RestoredState {
        store,
        preferences,
        pending,
        pending_quarantined,
        history,
        history_error,
    };
    if config.screen_reader {
        return run_screen_reader(config, client, restored).await;
    }
    let mut guard = TerminalGuard::acquire(
        SystemTerminal::default(),
        TerminalOptions {
            screen_reader: config.screen_reader,
            mouse: config.mouse == crate::MouseMode::On,
        },
    )
    .map_err(map_terminal_error)?;
    let mut terminal =
        Terminal::new(CrosstermBackend::new(io::stderr())).map_err(|_| TuiError::TerminalIo)?;
    terminal.clear().map_err(|_| TuiError::TerminalIo)?;

    let (sender, mut receiver) = mpsc::channel(256);
    let mut state = RuntimeState::new(config, client, sender, restored);
    host::bootstrap(state.client.clone(), state.sender.clone());
    let mut events = EventStream::new();
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    let mut interrupted = None;
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
            signal = &mut shutdown => {
                interrupted = Some(signal);
                break;
            }
        }
    }
    if let Some(task) = state.follow.take() {
        task.abort();
    }
    if let Some(task) = state.reconnect.take() {
        task.abort();
    }
    state.persist_presentation();
    guard.restore().map_err(map_terminal_error)?;
    interrupted.map_or(Ok(()), |signal| Err(TuiError::Interrupted(signal)))
}

async fn run_screen_reader(
    config: LaunchConfig,
    client: LiveHostClient,
    restored: RestoredState,
) -> Result<(), TuiError> {
    let mut guard = TerminalGuard::acquire(
        SystemTerminal::default(),
        TerminalOptions {
            screen_reader: true,
            mouse: false,
        },
    )
    .map_err(map_terminal_error)?;
    let (sender, mut receiver) = mpsc::channel(256);
    let mut state = RuntimeState::new(config, client, sender, restored);
    host::bootstrap(state.client.clone(), state.sender.clone());
    let mut events = EventStream::new();
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    let mut interrupted = None;
    let mut emitted = 0;
    let mut last_status = String::new();
    write_linear("Garive. Connecting to durable workspace.")?;
    loop {
        emit_linear_changes(&state, &mut emitted, &mut last_status)?;
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
            signal = &mut shutdown => {
                interrupted = Some(signal);
                break;
            }
        }
    }
    if let Some(task) = state.follow.take() {
        task.abort();
    }
    if let Some(task) = state.reconnect.take() {
        task.abort();
    }
    state.persist_presentation();
    write_linear("Garive exited. Terminal restored.")?;
    guard.restore().map_err(map_terminal_error)?;
    interrupted.map_or(Ok(()), |signal| Err(TuiError::Interrupted(signal)))
}

#[cfg(unix)]
async fn shutdown_signal() -> i32 {
    use tokio::signal::unix::{signal, SignalKind};

    let mut interrupt = signal(SignalKind::interrupt()).expect("SIGINT handler");
    let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM handler");
    tokio::select! {
        _ = interrupt.recv() => 2,
        _ = terminate.recv() => 15,
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() -> i32 {
    let _ = tokio::signal::ctrl_c().await;
    2
}

fn emit_linear_changes(
    state: &RuntimeState,
    emitted: &mut usize,
    last_status: &mut String,
) -> Result<(), TuiError> {
    let status = format!(
        "Connection {}. Turn {}.",
        linear_connection(state.model.connection),
        linear_execution(state.model.execution)
    );
    if *last_status != status {
        write_linear(&status)?;
        *last_status = status;
    }
    for item in state.model.timeline.iter().skip(*emitted) {
        let role = match item.role {
            TimelineRole::User => "You",
            TimelineRole::Agent => "Garive",
            TimelineRole::Status => "Activity",
        };
        write_linear(&format!("{role}: {}", linear_safe(&item.text)))?;
    }
    *emitted = state.model.timeline.len();
    Ok(())
}

fn write_linear(value: &str) -> Result<(), TuiError> {
    let mut stderr = io::stderr().lock();
    writeln!(stderr, "{value}")
        .and_then(|_| stderr.flush())
        .map_err(|_| TuiError::TerminalIo)
}

fn linear_safe(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\n' | '\t' => character,
            value
                if value.is_control()
                    || matches!(value, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
            {
                '�'
            }
            value => value,
        })
        .collect()
}

fn linear_connection(value: ConnectionState) -> &'static str {
    match value {
        ConnectionState::Connecting => "connecting",
        ConnectionState::Online => "online",
        ConnectionState::Disconnected { .. } => "disconnected",
        ConnectionState::Reconnecting { .. } => "reconnecting",
        ConnectionState::Unavailable { .. } => "unavailable",
    }
}

fn linear_execution(value: ExecutionState) -> &'static str {
    match value {
        ExecutionState::Idle => "ready",
        ExecutionState::Following => "running",
        ExecutionState::Suspended => "action required",
        ExecutionState::Failed => "failed",
    }
}

pub(super) struct RuntimeState {
    pub(super) config: LaunchConfig,
    pub(super) client: LiveHostClient,
    pub(super) sender: mpsc::Sender<HostMessage>,
    pub(super) model: AppModel,
    pub(super) follow: Option<JoinHandle<()>>,
    reconnect: Option<JoinHandle<()>>,
    reconnect_attempt: u32,
    pub(super) store: StateStore,
    pub(super) preferences: Preferences,
    persisted_preferences: Preferences,
    pub(super) pending: Vec<PendingCommand>,
    pub(super) ephemeral_confirmed: bool,
    pub(super) queued_prompt: Option<String>,
    pub(super) editing_suspension: Option<String>,
    snapshot_request: u64,
}

struct RestoredState {
    store: StateStore,
    preferences: Preferences,
    pending: Vec<PendingCommand>,
    pending_quarantined: usize,
    history: Vec<PromptHistoryEntry>,
    history_error: bool,
}

impl RuntimeState {
    fn new(
        config: LaunchConfig,
        client: LiveHostClient,
        sender: mpsc::Sender<HostMessage>,
        restored: RestoredState,
    ) -> Self {
        let mut model = AppModel::default();
        reduce(&mut model, AppAction::BootStarted);
        model.prompt_history = restored
            .history
            .into_iter()
            .rev()
            .map(|entry| entry.submitted_text)
            .collect();
        if restored.history_error {
            model.notice = Some("Corrupt prompt history was quarantined.".into());
        }
        if !restored.pending.is_empty() {
            model.notice = Some("A prior command has an unknown durable outcome. Use /retry after reviewing status.".into());
            model.overlay = Some(Overlay::UnknownCommand);
        } else if restored.pending_quarantined != 0 {
            model.notice = Some(format!(
                "Quarantined {} corrupt pending command file(s).",
                restored.pending_quarantined
            ));
        }
        model.has_pending_command = !restored.pending.is_empty();
        let persisted_preferences = restored.preferences.clone();
        Self {
            config,
            client,
            sender,
            model,
            follow: None,
            reconnect: None,
            reconnect_attempt: 0,
            store: restored.store,
            preferences: restored.preferences,
            persisted_preferences,
            pending: restored.pending,
            ephemeral_confirmed: false,
            queued_prompt: None,
            editing_suspension: None,
            snapshot_request: 0,
        }
    }

    pub(super) fn command_id(&mut self, _operation: &str) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    pub(super) fn dispatch(&mut self, action: AppAction) {
        for effect in reduce(&mut self.model, action) {
            match effect.kind {
                EffectKind::Exit => debug_assert!(self.model.quit_requested),
            }
        }
    }

    pub(super) fn load(&mut self, session_id: String) {
        if let Some(task) = self.follow.take() {
            task.abort();
        }
        if let Some(task) = self.reconnect.take() {
            task.abort();
        }
        self.reconnect_attempt = 0;
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
        self.snapshot_request = self.snapshot_request.saturating_add(1);
        host::load_snapshot(
            self.client.clone(),
            self.snapshot_request,
            session_id,
            self.sender.clone(),
        );
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
        if let Err(error) = self
            .store
            .save_preferences_merged(&mut self.preferences, &mut self.persisted_preferences)
        {
            self.model.notice = Some(format!("Local state: {}", state_error_name(error)));
        }
    }

    pub(super) fn admit_pending(&mut self, pending: PendingCommand) -> bool {
        if self.store.is_ephemeral() && !self.ephemeral_confirmed {
            self.model.overlay = Some(Overlay::EphemeralConfirmation);
            return false;
        }
        if self
            .pending
            .iter()
            .any(|value| value.session_id == pending.session_id)
        {
            self.model.notice = Some(
                "This Session already has an unknown command outcome. Use /retry first.".into(),
            );
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
        self.pending.push(pending);
        self.model.has_pending_command = true;
        true
    }

    pub(super) fn pending_for_context(&self) -> Option<&PendingCommand> {
        let selected = self.model.selected_session.as_deref();
        self.pending
            .iter()
            .find(|value| value.session_id.as_deref() == selected)
            .or_else(|| self.pending.first())
    }

    pub(super) fn abandon_pending(&mut self) {
        let Some(command_id) = self
            .pending_for_context()
            .map(|value| value.command_id.clone())
        else {
            self.model.overlay = None;
            return;
        };
        let index = self
            .pending
            .iter()
            .position(|value| value.command_id == command_id)
            .expect("pending command remains present");
        if self
            .store
            .remove_pending(self.pending[index].session_id.as_deref())
            .is_err()
        {
            self.local_state_failure("pending_abandon_failed");
            return;
        }
        self.pending.remove(index);
        self.model.has_pending_command = !self.pending.is_empty();
        self.model.overlay = None;
        if let Some(session) = self.model.selected_session.clone() {
            self.load(session);
        }
    }

    fn finish_pending(&mut self, command_id: &str, submitted_text: &str) {
        let Some(index) = self
            .pending
            .iter()
            .position(|value| value.command_id == command_id)
        else {
            return;
        };
        let pending = self.pending[index].clone();
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
                } else {
                    self.model
                        .prompt_history
                        .retain(|value| value != submitted_text);
                    self.model.prompt_history.insert(0, submitted_text.into());
                    self.model.prompt_history.truncate(500);
                }
            }
        }
        self.pending.remove(index);
        self.model.has_pending_command = !self.pending.is_empty();
    }

    fn reject_pending(&mut self, command_id: &str, code: HostClientErrorCode) {
        let pending_index = self
            .pending
            .iter()
            .position(|value| value.command_id == command_id);
        if matches!(
            code,
            HostClientErrorCode::HostFailure | HostClientErrorCode::InvalidCommand
        ) {
            if let Some(index) = pending_index {
                let pending = self.pending.remove(index);
                let _ = self.store.remove_pending(pending.session_id.as_deref());
            }
            self.model.has_pending_command = !self.pending.is_empty();
        } else if pending_index.is_some() {
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
            reduce(
                &mut state.model,
                AppAction::TerminalResized(TerminalSize {
                    width: area.width,
                    height: area.height,
                }),
            );
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
            state.model.definitions = definitions;
            state.model.sessions = sessions;
            state.dispatch(AppAction::BootCompleted {
                definition_count: state.model.definitions.len(),
                session_count: state.model.sessions.len(),
            });
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
            request_id,
            session_id,
            view,
            items,
            follow_position,
        } if state.model.selected_session.as_deref() == Some(&session_id)
            && request_id == state.snapshot_request =>
        {
            install_timeline(&mut state.model, items);
            match state.model.suspension.as_ref() {
                Some(suspension)
                    if state.editing_suspension.as_deref()
                        == Some(suspension.suspension_id.as_str()) =>
                {
                    state.model.overlay = None;
                }
                Some(_) => state.editing_suspension = None,
                None => state.editing_suspension = None,
            }
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
        HostMessage::SessionCreated {
            command_id,
            response,
        } => {
            state.finish_pending(&command_id, "");
            let session_id = response.session_id;
            state.load(session_id.clone());
            if let Some(text) = state.queued_prompt.take() {
                let command_id = state.command_id("turn");
                let pending = PendingCommand {
                    schema_version: 1,
                    command_id: command_id.clone(),
                    kind: PendingKind::StartTurn,
                    session_id: Some(session_id.clone()),
                    turn_id: None,
                    suspension_id: None,
                    expected_session_version: None,
                    requested_through_position: None,
                    request_payload: json!({"text": text}),
                    request_digest: String::new(),
                    created_at: now(),
                };
                if state.admit_pending(pending) {
                    host::start_turn(
                        state.client.clone(),
                        command_id,
                        session_id,
                        text,
                        state.sender.clone(),
                    );
                }
            } else {
                state.model.composer.clear();
            }
            host::bootstrap(state.client.clone(), state.sender.clone());
        }
        HostMessage::TurnAccepted {
            command_id,
            session_id,
            submitted_text,
            response,
        } => {
            state.finish_pending(&command_id, &submitted_text);
            if !submitted_text.is_empty() {
                state.model.composer.clear();
            }
            state.model.selected_turn = Some(response.turn_id);
            state.model.execution = ExecutionState::Following;
            state.load(session_id);
        }
        HostMessage::Event(event) => apply_event(event, state),
        HostMessage::FollowEnded { session_id, code }
            if state.model.selected_session.as_deref() == Some(&session_id) =>
        {
            state.follow = None;
            if matches!(
                code,
                HostClientErrorCode::InvalidEvent
                    | HostClientErrorCode::EventOrderViolation
                    | HostClientErrorCode::EventLimitExceeded
            ) {
                state.model.connection = ConnectionState::Unavailable {
                    safe_code: code.wire_name(),
                };
            } else if matches!(
                state.model.execution,
                ExecutionState::Following | ExecutionState::Suspended
            ) && state.reconnect_attempt < 5
            {
                state.reconnect_attempt += 1;
                state.model.connection = ConnectionState::Disconnected {
                    attempt: state.reconnect_attempt,
                };
                state.reconnect = Some(host::schedule_reconnect(
                    session_id,
                    state.reconnect_attempt,
                    state.sender.clone(),
                ));
            } else {
                state.model.connection = ConnectionState::Disconnected {
                    attempt: state.reconnect_attempt,
                };
            }
        }
        HostMessage::FollowEnded { .. } => {}
        HostMessage::ReconnectDue {
            session_id,
            attempt,
        } if state.model.selected_session.as_deref() == Some(&session_id)
            && state.reconnect_attempt == attempt =>
        {
            state.reconnect = None;
            state.model.connection = ConnectionState::Reconnecting { attempt };
            state.follow = Some(host::follow(
                state.client.clone(),
                session_id,
                state.model.observed_position,
                state.sender.clone(),
            ));
        }
        HostMessage::ReconnectDue { .. } => {}
        HostMessage::Failed { operation, error } => {
            let code = error.code;
            match operation {
                HostOperation::Bootstrap => {
                    state.dispatch(AppAction::HostUnavailable {
                        safe_code: code.wire_name(),
                    });
                    state.model.execution = ExecutionState::Failed;
                }
                HostOperation::Snapshot { request_id } if request_id != state.snapshot_request => {}
                HostOperation::Snapshot { .. }
                    if matches!(
                        code,
                        HostClientErrorCode::InvalidConfiguration
                            | HostClientErrorCode::InvalidEvent
                            | HostClientErrorCode::EventOrderViolation
                            | HostClientErrorCode::EventLimitExceeded
                    ) =>
                {
                    state.dispatch(AppAction::HostUnavailable {
                        safe_code: code.wire_name(),
                    });
                    state.model.execution = ExecutionState::Failed;
                }
                HostOperation::Snapshot { .. } if error.status.is_some() => {
                    state.model.connection = ConnectionState::Online;
                    state.model.notice =
                        Some(format!("Snapshot refresh deferred: {}.", code.wire_name()));
                }
                HostOperation::Snapshot { .. } => {
                    state.model.connection = ConnectionState::Disconnected {
                        attempt: state.reconnect_attempt,
                    };
                }
                HostOperation::Mutation { command_id } if error.status.is_some() => {
                    state.reject_pending(&command_id, code);
                    state.model.connection = ConnectionState::Online;
                    state.model.notice =
                        Some(format!("Host rejected the command: {}.", code.wire_name()));
                    if state.model.overlay != Some(Overlay::UnknownCommand) {
                        state.model.overlay = Some(Overlay::ErrorDetails);
                    }
                }
                HostOperation::Mutation { command_id } => {
                    state.reject_pending(&command_id, code);
                    state.model.connection = ConnectionState::Disconnected {
                        attempt: state.reconnect_attempt,
                    };
                }
            }
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
    state.reconnect_attempt = 0;
    state.model.connection = ConnectionState::Online;
    if !event.turn_id.is_empty() {
        state.model.selected_turn = Some(event.turn_id.clone());
    }
    if let Some(activity) = event.activity {
        let key = format!("activity:{}:{}", event.turn_id, activity.activity_id);
        let item = TimelineItem {
            stable_key: key.clone(),
            position: activity.source_position,
            role: TimelineRole::Status,
            text: format!("{} · {}", activity.label_key, activity.state),
        };
        if let Some(existing) = state
            .model
            .timeline
            .iter_mut()
            .find(|value| value.stable_key == key)
        {
            *existing = item;
        } else {
            state.model.timeline.push(item);
        }
    }
    if matches!(
        event.event.as_str(),
        "turn.started" | "turn.completed" | "turn.failed" | "turn.stopped" | "turn.suspended"
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
            stable_key: format!("turn:{}:user", turn.turn_id),
            position: turn.started_position,
            role: TimelineRole::User,
            text: turn.user_text,
        });
        for activity in turn.activities {
            model.timeline.push(TimelineItem {
                stable_key: format!("activity:{}:{}", turn.turn_id, activity.activity_id),
                position: activity.source_position,
                role: TimelineRole::Status,
                text: format!("{} · {}", activity.label_key, activity.state),
            });
        }
        if let Some(text) = turn.completion_text {
            model.timeline.push(TimelineItem {
                stable_key: format!("turn:{}:agent", turn.turn_id),
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
