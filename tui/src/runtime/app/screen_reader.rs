use std::io::{self, Write};

use crossterm::event::EventStream;
use futures::StreamExt;
use garive_host_client::LiveHostClient;
use tokio::sync::mpsc;

use crate::{
    application::{AppModel, ConnectionState, ExecutionState, Overlay, TimelineRole},
    persistence::DiagnosticEvent,
    LaunchConfig, TuiError,
};

use super::{
    super::{controller::handle_terminal, host, SystemTerminal, TerminalGuard, TerminalOptions},
    handle_host, map_terminal_error, shutdown_signal, RestoredState, RuntimeState,
};

pub(super) async fn run(
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
    #[cfg(feature = "test-hooks")]
    if config.test_crash_hook == Some(crate::args::TestCrashHook::TerminalAcquiredPanic) {
        panic!("injected panic after terminal acquisition");
    }
    let (sender, mut receiver) = mpsc::channel(256);
    let mut state = RuntimeState::new(config, client, sender, restored);
    host::bootstrap(state.client.clone(), state.sender.clone());
    let mut events = EventStream::new();
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    let mut interrupted = None;
    let mut emitted = 0;
    let mut last_status = String::new();
    let mut last_overlay = String::new();
    write_linear("Garive. Connecting to durable workspace.")?;
    loop {
        if std::mem::take(&mut state.bell_requested) {
            write_linear_bell()?;
        }
        emit_linear_changes(&state, &mut emitted, &mut last_status, &mut last_overlay)?;
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

fn emit_linear_changes(
    state: &RuntimeState,
    emitted: &mut usize,
    last_status: &mut String,
    last_overlay: &mut String,
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
    let overlay = linear_overlay(&state.model);
    if *last_overlay != overlay {
        if !overlay.is_empty() {
            write_linear(&overlay)?;
        }
        *last_overlay = overlay;
    }
    Ok(())
}

fn linear_overlay(model: &AppModel) -> String {
    let Some(overlay) = model.overlay else {
        return String::new();
    };
    let value = match overlay {
        Overlay::CommandPalette => {
            let rows = crate::input::COMMAND_PALETTE
                .iter()
                .enumerate()
                .map(|(index, (name, help))| format!("{}. {name}: {help}", index + 1))
                .collect::<Vec<_>>()
                .join("\n");
            format!("Command palette.\n{rows}\nUse arrows and Enter, or Escape to close.")
        }
        Overlay::Help => "Keyboard guide. Enter sends. Control J inserts a newline. Control S opens Sessions. Control P opens commands. Control C cancels a running Turn. Control Q asks to quit. Escape closes a nonblocking prompt.".into(),
        Overlay::SessionPicker => {
            let rows = model
                .sessions
                .iter()
                .enumerate()
                .map(|(index, session)| {
                    format!(
                        "{}. Session ending {}, {}.",
                        index + 1,
                        session
                            .session_id
                            .get(session.session_id.len().saturating_sub(6)..)
                            .unwrap_or(&session.session_id),
                        session.latest_turn_state.as_deref().unwrap_or("new")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("Switch Session.\n{rows}\nUse arrows and Enter, or Escape to close.")
        }
        Overlay::PromptHistory => {
            let rows = model
                .prompt_history
                .iter()
                .take(10)
                .enumerate()
                .map(|(index, text)| {
                    format!(
                        "{}. {}",
                        index + 1,
                        text.lines().next().unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("Prompt history.\n{rows}\nUse arrows and Enter, or Escape to close.")
        }
        Overlay::Suspension => {
            let prompt = model
                .suspension
                .as_ref()
                .map(|value| value.prompt_json.as_str())
                .unwrap_or("Action required");
            let guidance = model
                .suspension
                .as_ref()
                .and_then(|value| value.response_schema_json.as_deref())
                .map(crate::input::describe_schema)
                .unwrap_or("Enter a text response.");
            format!("Action required. {prompt}\n{guidance}\nPress Enter to reply now.")
        }
        Overlay::UnknownCommand => "Command result unknown. Press Enter for exact retry, or A to abandon the local recovery record.".into(),
        Overlay::ErrorDetails => format!(
            "Status details. {} Press Escape to close.",
            model.notice.as_deref().unwrap_or("No safe details available.")
        ),
        Overlay::EphemeralConfirmation => "Ephemeral mode cannot recover a lost mutation response. Press Enter to accept for this run, or Escape to cancel.".into(),
        Overlay::QuitConfirmation => "Quit Garive? Press Enter to quit, or Escape to keep working.".into(),
    };
    linear_safe(&value)
}

fn write_linear(value: &str) -> Result<(), TuiError> {
    let mut stderr = io::stderr().lock();
    writeln!(stderr, "{value}")
        .and_then(|_| stderr.flush())
        .map_err(|_| TuiError::TerminalIo)
}

fn write_linear_bell() -> Result<(), TuiError> {
    let mut stderr = io::stderr().lock();
    stderr
        .write_all(b"\x07")
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
