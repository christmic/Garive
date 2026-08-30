use std::{
    collections::BTreeMap,
    io::{self, Write},
    time::Instant,
};

use crossterm::event::EventStream;
use futures::StreamExt;
#[cfg(test)]
use garive_host_client::TurnTimelineItem;
use garive_host_client::{ClientLimits, HostClientErrorCode, LiveHostClient};
use ratatui::{backend::CrosstermBackend, Terminal};
#[cfg(test)]
use serde_json::json;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    application::{
        reduce, AppAction, AppModel, ConnectionState, EffectKind, ExecutionState, Overlay,
        TerminalSize,
    },
    persistence::{
        now, DiagnosticEvent, PendingCommand, PendingKind, Preferences, PromptHistoryEntry,
        StateError, StateStore,
    },
    view, LaunchConfig, TuiError,
};

use super::{
    controller::handle_terminal,
    host::{self, HostMessage},
    SystemTerminal, TerminalError, TerminalGuard, TerminalOptions,
};

mod messages;
mod projection;
mod screen_reader;

use messages::handle_host;
#[cfg(test)]
use projection::install_timeline;

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
    crate::args::apply_terminal_environment(
        &mut config,
        std::env::var("TERM").ok().as_deref(),
        std::env::var_os("NO_COLOR").is_some(),
    );
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
        return screen_reader::run(config, client, restored).await;
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
    bell_requested: bool,
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
            bell_requested: false,
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

    pub(super) fn composer_is_frozen(&self) -> bool {
        pending_freezes_composer(&self.pending, self.model.selected_session.as_deref())
    }

    pub(super) fn explain_frozen_composer(&mut self) {
        self.model.notice =
            Some("This draft is frozen until the pending command reaches durable truth.".into());
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

fn pending_freezes_composer(pending: &[PendingCommand], selected: Option<&str>) -> bool {
    pending.iter().any(|pending| {
        pending.session_id.as_deref() == selected
            || (selected.is_none() && pending.kind == PendingKind::CreateSession)
    })
}

fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
    state: &mut RuntimeState,
) -> Result<(), TuiError> {
    if std::mem::take(&mut state.bell_requested) {
        terminal
            .backend_mut()
            .write_all(b"\x07")
            .and_then(|_| terminal.backend_mut().flush())
            .map_err(|_| TuiError::TerminalIo)?;
    }
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

    #[test]
    fn only_the_pending_commands_for_the_active_context_freeze_composition() {
        let pending = vec![PendingCommand {
            schema_version: 1,
            command_id: "command".into(),
            kind: PendingKind::StartTurn,
            session_id: Some("session-a".into()),
            turn_id: None,
            suspension_id: None,
            expected_session_version: None,
            requested_through_position: None,
            request_payload: json!({"text":"private"}),
            request_digest: "digest".into(),
            created_at: "2026-08-30T00:00:00Z".into(),
        }];
        assert!(pending_freezes_composer(&pending, Some("session-a")));
        assert!(!pending_freezes_composer(&pending, Some("session-b")));
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
