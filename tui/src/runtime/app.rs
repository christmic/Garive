use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    time::Instant,
};

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
        now, DiagnosticEvent, PendingCommand, PendingKind, Preferences, PromptHistoryEntry,
        StateError, StateStore,
    },
    view, LaunchConfig, TuiError,
};

use super::{
    controller::{handle_terminal, replay_pending},
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
    let _ = store.record_diagnostic(DiagnosticEvent::Started);
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
    state.stop_tasks();
    state.persist_presentation();
    guard.restore().map_err(map_terminal_error)?;
    let _ = state
        .store
        .record_diagnostic(DiagnosticEvent::TerminalRestored);
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
    state.stop_tasks();
    state.persist_presentation();
    write_linear("Garive exited. Terminal restored.")?;
    guard.restore().map_err(map_terminal_error)?;
    let _ = state
        .store
        .record_diagnostic(DiagnosticEvent::TerminalRestored);
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
    background_follows: BTreeMap<String, BackgroundFollow>,
    follow_sequence: u64,
    pub(super) force_redraw: bool,
    pub(super) last_empty_ctrl_c: Option<Instant>,
    pub(super) retry_after_refresh: Option<String>,
    render_cache: view::RenderCache,
}

struct BackgroundFollow {
    observed_position: u64,
    attempt: u32,
    sequence: u64,
    follow: Option<JoinHandle<()>>,
    reconnect: Option<JoinHandle<()>>,
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
            background_follows: BTreeMap::new(),
            follow_sequence: 0,
            force_redraw: false,
            last_empty_ctrl_c: None,
            retry_after_refresh: None,
            render_cache: view::RenderCache::default(),
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
        let switching_session = self.model.selected_session.as_deref() != Some(&session_id);
        if switching_session {
            if matches!(
                self.model.execution,
                ExecutionState::Following | ExecutionState::Suspended
            ) {
                if let (Some(previous), Some(task)) =
                    (self.model.selected_session.clone(), self.follow.take())
                {
                    self.add_background_follow(previous, self.model.observed_position, task);
                }
            } else if let Some(task) = self.follow.take() {
                task.abort();
            }
        } else if let Some(task) = self.follow.take() {
            task.abort();
        }
        if let Some(task) = self.reconnect.take() {
            task.abort();
        }
        self.reconnect_attempt = 0;
        if let Some(mut background) = self.background_follows.remove(&session_id) {
            if let Some(task) = background.follow.take() {
                task.abort();
            }
            if let Some(task) = background.reconnect.take() {
                task.abort();
            }
        }
        self.persist_presentation();
        if switching_session {
            self.model.switch_viewport(&session_id);
        }
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

    fn add_background_follow(
        &mut self,
        session_id: String,
        observed_position: u64,
        task: JoinHandle<()>,
    ) {
        self.follow_sequence = self.follow_sequence.saturating_add(1);
        if let Some(mut replaced) = self.background_follows.remove(&session_id) {
            if let Some(previous) = replaced.follow.take() {
                previous.abort();
            }
            if let Some(previous) = replaced.reconnect.take() {
                previous.abort();
            }
        }
        self.background_follows.insert(
            session_id,
            BackgroundFollow {
                observed_position,
                attempt: 0,
                sequence: self.follow_sequence,
                follow: Some(task),
                reconnect: None,
            },
        );
        if self.background_follows.len() > 4 {
            let oldest = self
                .background_follows
                .iter()
                .min_by_key(|(_, value)| value.sequence)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                if let Some(mut evicted) = self.background_follows.remove(&oldest) {
                    if let Some(task) = evicted.follow.take() {
                        task.abort();
                    }
                    if let Some(task) = evicted.reconnect.take() {
                        task.abort();
                    }
                }
            }
        }
    }

    fn stop_tasks(&mut self) {
        if let Some(task) = self.follow.take() {
            task.abort();
        }
        if let Some(task) = self.reconnect.take() {
            task.abort();
        }
        for background in self.background_follows.values_mut() {
            if let Some(task) = background.follow.take() {
                task.abort();
            }
            if let Some(task) = background.reconnect.take() {
                task.abort();
            }
        }
    }

    pub(super) fn load_more_sessions(&mut self) {
        if self.model.sessions_loading {
            return;
        }
        let Some(before) = self.model.sessions_next_before.clone() else {
            return;
        };
        self.model.sessions_loading = true;
        host::load_session_page(self.client.clone(), before, self.sender.clone());
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
    if std::mem::take(&mut state.force_redraw) {
        terminal.clear().map_err(|_| TuiError::TerminalIo)?;
    }
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
            if let Some(cursor) = view::render_cached(
                &state.model,
                state.config.theme,
                area,
                frame.buffer_mut(),
                &mut state.render_cache,
            ) {
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
            next_before,
        } => {
            state.model.definitions = definitions;
            state.model.sessions = sessions;
            state.model.sessions_next_before = next_before;
            state.model.sessions_loading = false;
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
            replay_queued_create(state);
        }
        HostMessage::SessionPageLoaded {
            sessions,
            next_before,
        } => {
            for session in sessions {
                if !state
                    .model
                    .sessions
                    .iter()
                    .any(|existing| existing.session_id == session.session_id)
                {
                    state.model.sessions.push(session);
                }
            }
            state.model.sessions_next_before = next_before;
            state.model.sessions_loading = false;
            state.model.session_count = state.model.sessions.len();
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
                session_id.clone(),
                follow_position,
                state.sender.clone(),
            ));
            replay_queued_for_session(state, &session_id);
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
        HostMessage::FollowEnded { session_id, code } => {
            let Some(background) = state.background_follows.get_mut(&session_id) else {
                return;
            };
            background.follow = None;
            if matches!(
                code,
                HostClientErrorCode::InvalidEvent
                    | HostClientErrorCode::EventOrderViolation
                    | HostClientErrorCode::EventLimitExceeded
            ) {
                state.background_follows.remove(&session_id);
                state.model.notice = Some(format!(
                    "Background Session follow stopped: {}.",
                    code.wire_name()
                ));
            } else if background.attempt < 5 {
                background.attempt += 1;
                background.reconnect = Some(host::schedule_reconnect(
                    session_id,
                    background.attempt,
                    state.sender.clone(),
                ));
            }
        }
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
        HostMessage::ReconnectDue {
            session_id,
            attempt,
        } => {
            let Some(background) = state.background_follows.get_mut(&session_id) else {
                return;
            };
            if background.attempt != attempt {
                return;
            }
            background.reconnect = None;
            background.follow = Some(host::follow(
                state.client.clone(),
                session_id,
                background.observed_position,
                state.sender.clone(),
            ));
        }
        HostMessage::Failed { operation, error } => {
            let refresh_failed = match &operation {
                HostOperation::Bootstrap => true,
                HostOperation::Snapshot { request_id } => *request_id == state.snapshot_request,
                _ => false,
            };
            if refresh_failed && state.retry_after_refresh.take().is_some() {
                state.model.notice =
                    Some("Fresh Host truth could not be loaded; exact retry was not sent.".into());
            }
            let code = error.code;
            let _ = state.store.record_diagnostic(DiagnosticEvent::HostFailure {
                safe_code: code.wire_name(),
            });
            match operation {
                HostOperation::Bootstrap => {
                    state.dispatch(AppAction::HostUnavailable {
                        safe_code: code.wire_name(),
                    });
                    state.model.execution = ExecutionState::Failed;
                }
                HostOperation::Snapshot { request_id } if request_id != state.snapshot_request => {}
                HostOperation::SessionPage => {
                    state.model.sessions_loading = false;
                    state.model.notice =
                        Some(format!("Session page unavailable: {}.", code.wire_name()));
                }
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

fn replay_queued_create(state: &mut RuntimeState) {
    let Some(command_id) = state.retry_after_refresh.clone() else {
        return;
    };
    let Some(pending) = state
        .pending
        .iter()
        .find(|pending| {
            pending.command_id == command_id && pending.kind == PendingKind::CreateSession
        })
        .cloned()
    else {
        return;
    };
    state.retry_after_refresh = None;
    replay_pending(state, pending);
}

fn replay_queued_for_session(state: &mut RuntimeState, session_id: &str) {
    let Some(command_id) = state.retry_after_refresh.clone() else {
        return;
    };
    let Some(pending) = state
        .pending
        .iter()
        .find(|pending| {
            pending.command_id == command_id && pending.session_id.as_deref() == Some(session_id)
        })
        .cloned()
    else {
        return;
    };
    state.retry_after_refresh = None;
    replay_pending(state, pending);
}

fn apply_event(event: HostEvent, state: &mut RuntimeState) {
    if state.model.selected_session.as_deref() != Some(&event.session_id) {
        apply_background_event(event, state);
        return;
    }
    if event.position <= state.model.observed_position {
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

fn apply_background_event(event: HostEvent, state: &mut RuntimeState) {
    let Some(background) = state.background_follows.get_mut(&event.session_id) else {
        return;
    };
    if event.position <= background.observed_position {
        return;
    }
    background.observed_position = event.position;
    background.attempt = 0;
    let lifecycle = match event.event.as_str() {
        "turn.started" => Some("running"),
        "turn.suspended" => Some("suspended"),
        "turn.completed" => Some("completed"),
        "turn.failed" => Some("failed"),
        "turn.stopped" => Some("stopped"),
        _ => None,
    };
    if let Some(summary) = state
        .model
        .sessions
        .iter_mut()
        .find(|value| value.session_id == event.session_id)
    {
        summary.latest_position = summary.latest_position.max(event.position);
        if !event.turn_id.is_empty() {
            summary.latest_turn_id = Some(event.turn_id.clone());
        }
        if event.event == "turn.started" {
            summary.turn_count = summary.turn_count.saturating_add(1);
        }
        if let Some(lifecycle) = lifecycle {
            summary.latest_turn_state = Some(lifecycle.into());
        }
    }
    if matches!(
        event.event.as_str(),
        "turn.suspended" | "turn.completed" | "turn.failed" | "turn.stopped"
    ) {
        let mut finished = state
            .background_follows
            .remove(&event.session_id)
            .expect("background follow remains present");
        if let Some(task) = finished.follow.take() {
            task.abort();
        }
        if let Some(task) = finished.reconnect.take() {
            task.abort();
        }
        state.model.notice = Some(if event.event == "turn.suspended" {
            "A background Session requires action.".into()
        } else {
            "A background Session reached a terminal state.".into()
        });
    }
}

fn install_timeline(model: &mut AppModel, mut turns: Vec<TurnTimelineItem>) {
    let old_keys = model
        .timeline
        .iter()
        .map(|item| item.stable_key.clone())
        .collect::<BTreeSet<_>>();
    let old_max_position = model
        .timeline
        .iter()
        .map(|item| item.position)
        .max()
        .unwrap_or(0);
    let old_anchor = model.viewport.anchor_key.clone();
    let old_anchor_index = old_anchor.as_deref().and_then(|key| {
        model
            .timeline
            .iter()
            .position(|item| item.stable_key == key)
    });
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
    if model.viewport.follow_latest {
        model.follow_latest();
        return;
    }
    let replacement_anchor = old_anchor
        .filter(|key| model.timeline.iter().any(|item| item.stable_key == *key))
        .or_else(|| {
            old_anchor_index.and_then(|index| {
                model
                    .timeline
                    .get(index.min(model.timeline.len().saturating_sub(1)))
                    .map(|item| item.stable_key.clone())
            })
        });
    model.viewport.anchor_key = replacement_anchor;
    model.viewport.newer_updates = model.viewport.newer_updates.saturating_add(
        model
            .timeline
            .iter()
            .filter(|item| item.position > old_max_position && !old_keys.contains(&item.stable_key))
            .count(),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_refresh_preserves_manual_anchor_and_counts_new_cells() {
        let mut model = AppModel::default();
        install_timeline(&mut model, vec![turn("one", 1), turn("two", 4)]);
        model.scroll_conversation_up(1);
        let anchor = model.viewport.anchor_key.clone();

        install_timeline(
            &mut model,
            vec![turn("one", 1), turn("two", 4), turn("three", 7)],
        );

        assert_eq!(model.viewport.anchor_key, anchor);
        assert!(!model.viewport.follow_latest);
        assert_eq!(model.viewport.newer_updates, 2);
        model.follow_latest();
        assert!(model.viewport.follow_latest);
        assert_eq!(model.viewport.newer_updates, 0);
    }

    fn turn(id: &str, position: u64) -> TurnTimelineItem {
        TurnTimelineItem {
            turn_id: id.into(),
            started_position: position,
            latest_position: position + 1,
            state: "completed".into(),
            user_text: format!("question {id}"),
            completion_text: Some(format!("answer {id}")),
            suspension: None,
            content_truncated: false,
            activities: Vec::new(),
        }
    }
}
