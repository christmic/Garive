use std::io::{self, Write};

#[cfg(test)]
use garive_host_client::TurnTimelineItem;
use garive_host_client::{ClientLimits, LiveHostClient};
use ratatui::{backend::CrosstermBackend, Terminal};
#[cfg(test)]
use serde_json::json;
use tokio::sync::mpsc;

#[cfg(test)]
use crate::{
    application::AppModel,
    persistence::{PendingCommand, PendingKind},
};
use crate::{
    application::{reduce, AppAction, TerminalSize},
    persistence::{DiagnosticEvent, StateError, StateStore},
    view, LaunchConfig, TuiError,
};

use super::{
    controller::handle_terminal, external_editor, host, terminal_events::TerminalEventReader,
    SystemTerminal, TerminalError, TerminalGuard, TerminalOptions,
};

mod messages;
mod projection;
mod screen_reader;
mod state;

use messages::handle_host;
#[cfg(test)]
use projection::install_timeline;
#[cfg(test)]
use state::pending_freezes_composer;
use state::RestoredState;
pub(super) use state::RuntimeState;

const LIMITS: ClientLimits = ClientLimits {
    max_command_bytes: 4_096,
    max_event_bytes: 32_768,
    max_events: 16_384,
    follow_deadline_ms: 120_000,
};
const MOTION_INTERVAL: std::time::Duration = std::time::Duration::from_millis(160);

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
    let mut shutdown = ShutdownSignal::new()?;
    if config.screen_reader {
        return screen_reader::run(config, client, restored, shutdown).await;
    }
    let mut guard = TerminalGuard::acquire(
        SystemTerminal::default(),
        TerminalOptions {
            screen_reader: config.screen_reader,
            mouse: config.mouse == crate::MouseMode::On,
        },
    )
    .map_err(map_terminal_error)?;
    #[cfg(feature = "test-hooks")]
    if config.test_crash_hook == Some(crate::args::TestCrashHook::TerminalAcquiredPanic) {
        panic!("injected panic after terminal acquisition");
    }
    let mut terminal = match Terminal::new(CrosstermBackend::new(io::stderr())) {
        Ok(terminal) => terminal,
        Err(_) => return Err(terminal_setup_failure(&mut guard, &mut shutdown).await),
    };
    if terminal.clear().is_err() {
        return Err(terminal_setup_failure(&mut guard, &mut shutdown).await);
    }

    let (sender, mut receiver) = mpsc::channel(256);
    let mut state = RuntimeState::new(config, client, sender, restored);
    host::bootstrap(state.client.clone(), state.sender.clone());
    let mut events = TerminalEventReader::start().map_err(|_| TuiError::TerminalIo)?;
    let mut interrupted = None;
    let mut motion_tick = 0_u64;
    let mut motion_clock = tokio::time::interval(MOTION_INTERVAL);
    motion_clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    motion_clock.tick().await;
    loop {
        if let Some(request) = state.external_editor_request.take() {
            match external_editor::prepare(request) {
                Err((message, request)) => {
                    external_editor::apply(&mut state, request, Err(message))
                }
                Ok(prepared) => {
                    events.pause().map_err(|_| TuiError::TerminalIo)?;
                    guard.restore().map_err(map_terminal_error)?;
                    writeln!(
                        io::stderr(),
                        "Garive paused. The external editor owns this terminal until it exits."
                    )
                    .map_err(|_| TuiError::TerminalIo)?;
                    let (request, result, signal) =
                        wait_for_external_editor(prepared, &mut shutdown).await;
                    guard = TerminalGuard::acquire(
                        SystemTerminal::default(),
                        TerminalOptions {
                            screen_reader: false,
                            mouse: state.config.mouse == crate::MouseMode::On,
                        },
                    )
                    .map_err(map_terminal_error)?;
                    terminal.clear().map_err(|_| TuiError::TerminalIo)?;
                    external_editor::apply(&mut state, request, result);
                    // The paused clear already invalidated Ratatui's back buffer.
                    // Do not issue a second cursor query after input resumes.
                    state.force_redraw = false;
                    events.resume().map_err(|_| TuiError::TerminalIo)?;
                    if let Some(signal) = signal {
                        interrupted = Some(signal);
                        break;
                    }
                }
            }
        }
        guard
            .set_title(&view::terminal_title(&state.model))
            .map_err(map_terminal_error)?;
        draw(&mut terminal, &mut state, motion_tick)?;
        if state.model.quit_requested {
            break;
        }
        tokio::select! {
            event = events.recv() => match event {
                Some(Ok(event)) => handle_terminal(event, &mut state),
                Some(Err(_)) | None => return Err(TuiError::TerminalIo),
            },
            message = receiver.recv() => match message {
                Some(message) => handle_host(message, &mut state),
                None => break,
            },
            signal = shutdown.recv() => {
                interrupted = Some(signal);
                break;
            }
            _ = motion_clock.tick(), if view::status_motion_enabled(&state.model, state.config.reduced_motion) => {
                motion_tick = motion_tick.wrapping_add(1);
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

pub(super) async fn wait_for_external_editor(
    prepared: external_editor::PreparedEditor,
    shutdown: &mut ShutdownSignal,
) -> (
    external_editor::EditorRequest,
    Result<String, &'static str>,
    Option<i32>,
) {
    let mut child = match prepared.spawn() {
        Ok(child) => child,
        Err(_) => {
            let (request, result) = prepared.failed(external_editor::SPAWN_FAILED);
            return (request, result, None);
        }
    };
    tokio::select! {
        status = child.wait() => {
            let (request, result) = match status {
                Ok(status) => prepared.finish(status),
                Err(_) => prepared.failed(external_editor::EXIT_FAILED),
            };
            (request, result, None)
        }
        signal = shutdown.recv() => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            let (request, result) = prepared.failed(external_editor::EXIT_FAILED);
            (request, result, Some(signal))
        }
    }
}

async fn terminal_setup_failure(
    guard: &mut TerminalGuard<SystemTerminal>,
    shutdown: &mut ShutdownSignal,
) -> TuiError {
    let restored = guard.restore();
    let signal = tokio::time::timeout(std::time::Duration::from_millis(50), shutdown.recv())
        .await
        .ok();
    match (restored, signal) {
        (_, Some(signal)) => TuiError::Interrupted(signal),
        (Err(_), None) | (Ok(()), None) => TuiError::TerminalIo,
    }
}

#[cfg(unix)]
pub(super) struct ShutdownSignal {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl ShutdownSignal {
    fn new() -> Result<Self, TuiError> {
        use tokio::signal::unix::{signal, SignalKind};

        Ok(Self {
            interrupt: signal(SignalKind::interrupt()).map_err(|_| TuiError::TerminalIo)?,
            terminate: signal(SignalKind::terminate()).map_err(|_| TuiError::TerminalIo)?,
        })
    }

    async fn recv(&mut self) -> i32 {
        tokio::select! {
            _ = self.interrupt.recv() => 2,
            _ = self.terminate.recv() => 15,
        }
    }
}

#[cfg(windows)]
pub(super) struct ShutdownSignal {
    interrupt: tokio::signal::windows::CtrlC,
}

#[cfg(windows)]
impl ShutdownSignal {
    fn new() -> Result<Self, TuiError> {
        Ok(Self {
            interrupt: tokio::signal::windows::ctrl_c().map_err(|_| TuiError::TerminalIo)?,
        })
    }

    async fn recv(&mut self) -> i32 {
        let _ = self.interrupt.recv().await;
        2
    }
}

fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
    state: &mut RuntimeState,
    motion_tick: u64,
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
            let cursor = if state.config.reduced_motion {
                view::render_cached(
                    &state.model,
                    state.config.theme,
                    area,
                    frame.buffer_mut(),
                    &mut state.render_cache,
                )
            } else {
                view::render_cached_with_motion(
                    &state.model,
                    state.config.theme,
                    view::MotionFrame::animated(motion_tick),
                    area,
                    frame.buffer_mut(),
                    &mut state.render_cache,
                )
            };
            if let Some(cursor) = cursor {
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
    use crate::application::{FocusTarget, Overlay};

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
    fn timeline_install_builds_safe_public_landmarks_and_tears_down_stale_navigation() {
        let mut hostile = turn("private-id", 9);
        hostile.user_text = "  beta\n\u{1b}[31m \u{2066}safe\u{2069}  ".into();
        let mut model = AppModel {
            overlay: Some(Overlay::TurnNavigator),
            focus: FocusTarget::Overlay,
            prior_focus: FocusTarget::Conversation,
            turn_filter: "stale".into(),
            turn_selection: 7,
            ..Default::default()
        };

        install_timeline(&mut model, vec![hostile, turn("older", 1)]);

        assert_eq!(model.overlay, None);
        assert_eq!(model.focus, FocusTarget::Conversation);
        assert!(model.turn_filter.is_empty());
        assert_eq!(model.turn_selection, 0);
        assert_eq!(
            model
                .conversation_landmarks
                .iter()
                .map(|item| (
                    item.ordinal,
                    item.started_position,
                    item.prompt_preview.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![(1, 1, "question older"), (2, 9, "beta �[31m �safe�")]
        );
        assert!(!model.conversation_landmarks[1]
            .prompt_preview
            .contains("private-id"));
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
