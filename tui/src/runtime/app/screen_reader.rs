use std::io::{self, Write};

use crossterm::event::EventStream;
use futures::StreamExt;
use garive_host_client::LiveHostClient;
use tokio::sync::mpsc;

use crate::{
    application::{ConnectionState, ExecutionState, TimelineRole},
    persistence::DiagnosticEvent,
    LaunchConfig, TuiError,
};

use super::{
    super::{controller::handle_terminal, host, SystemTerminal, TerminalGuard, TerminalOptions},
    handle_host, map_terminal_error, RestoredState, RuntimeState, ShutdownSignal,
};

pub(super) async fn run(
    config: LaunchConfig,
    client: LiveHostClient,
    restored: RestoredState,
    mut shutdown: ShutdownSignal,
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
    let mut interrupted = None;
    let mut emitted = 0;
    let mut last_status = String::new();
    let mut last_overlay = String::new();
    write_linear("Garive. Connecting to durable workspace.")?;
    loop {
        guard
            .set_title(&crate::view::terminal_title(&state.model))
            .map_err(map_terminal_error)?;
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
            signal = shutdown.recv() => {
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
        write_linear(&format!("{role}: {}", crate::view::linear_safe(&item.text)))?;
    }
    *emitted = state.model.timeline.len();
    let overlay = crate::view::linear_overlay(&state.model);
    if *last_overlay != overlay {
        if !overlay.is_empty() {
            write_linear(&overlay)?;
        }
        *last_overlay = overlay;
    }
    Ok(())
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
