use std::io::{self, Write};

use crossterm::event::EventStream;
use futures::StreamExt;
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
    controller::handle_terminal, host, SystemTerminal, TerminalError, TerminalGuard,
    TerminalOptions,
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
