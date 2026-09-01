use std::{
    collections::BTreeMap,
    io::{self, Write},
};

use garive_host_client::LiveHostClient;
use tokio::sync::mpsc;

use crate::{
    application::{AppAction, AppModel, ConnectionState, ExecutionState, TimelineRole},
    persistence::DiagnosticEvent,
    LaunchConfig, TuiError,
};

use super::{
    super::{
        controller::handle_terminal, external_editor, terminal_events::TerminalEventReader,
        SystemTerminal, TerminalGuard, TerminalOptions, TerminalTheme,
    },
    handle_host, map_terminal_error,
    scheduling::{self, FairScheduler, ResizeCoalescer, Scheduled, ShutdownSignal},
    wait_for_external_editor, RestoredState, RuntimeState,
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
    let (action_sender, mut action_receiver) = mpsc::channel(64);
    let mut state = RuntimeState::new(
        config,
        client,
        sender,
        action_sender,
        TerminalTheme::default(),
        restored,
    );
    state.dispatch(AppAction::BootStarted);
    let mut events = TerminalEventReader::start().map_err(|_| TuiError::TerminalIo)?;
    let mut interrupted = None;
    let mut emitted = BTreeMap::new();
    let mut last_status = String::new();
    let mut last_overlay = String::new();
    let mut last_composer = String::new();
    let mut scheduler = FairScheduler::default();
    let mut resize = ResizeCoalescer::default();
    write_linear("Garive. Connecting to durable workspace.")?;
    loop {
        state.advance_graceful_quit();
        if let Some(request) = state.external_editor_request.take() {
            match external_editor::prepare(request) {
                Err((message, request)) => {
                    external_editor::apply(&mut state, request, Err(message))
                }
                Ok(prepared) => {
                    events.pause().map_err(|_| TuiError::TerminalIo)?;
                    guard.restore().map_err(map_terminal_error)?;
                    write_linear(
                        "Garive paused. The external editor owns this terminal until it exits.",
                    )?;
                    let (request, result, signal) =
                        wait_for_external_editor(prepared, &mut shutdown).await;
                    guard = TerminalGuard::acquire(
                        SystemTerminal::default(),
                        TerminalOptions {
                            screen_reader: true,
                            mouse: false,
                        },
                    )
                    .map_err(map_terminal_error)?;
                    external_editor::apply(&mut state, request, result);
                    events.resume().map_err(|_| TuiError::TerminalIo)?;
                    write_linear(
                        state
                            .model
                            .notice
                            .as_deref()
                            .unwrap_or("External editing finished."),
                    )?;
                    if let Some(signal) = signal {
                        interrupted = Some(signal);
                        break;
                    }
                }
            }
        }
        guard
            .set_title(&crate::view::terminal_title(&state.model))
            .map_err(map_terminal_error)?;
        if std::mem::take(&mut state.bell_requested) {
            write_linear_bell()?;
        }
        if !resize.is_pending() {
            emit_linear_changes(
                &state,
                &mut emitted,
                &mut last_status,
                &mut last_overlay,
                &mut last_composer,
            )?;
        }
        if state.model.quit_requested {
            break;
        }
        let scheduled = scheduling::select_next(
            &mut scheduler,
            shutdown.recv(),
            events.recv(),
            action_receiver.recv(),
            std::future::pending::<()>(),
            std::future::pending::<()>(),
            receiver.recv(),
            resize.wait(),
            false,
            false,
            resize.is_pending(),
        )
        .await;
        match scheduled {
            Scheduled::Shutdown(signal) => {
                interrupted = Some(signal);
                break;
            }
            Scheduled::Terminal(event) => match event {
                Some(Ok(crossterm::event::Event::Resize(width, height))) => {
                    resize.push(width, height, tokio::time::Instant::now());
                }
                Some(Ok(event)) => handle_terminal(event, &mut state),
                Some(Err(_)) | None => return Err(TuiError::TerminalIo),
            },
            Scheduled::Action(action) => match action {
                Some(action) => state.dispatch(action),
                None => break,
            },
            Scheduled::Host(message) => match message {
                Some(message) => handle_host(message, &mut state),
                None => break,
            },
            Scheduled::ResizeDeadline => {
                if let Some(event) = resize.take() {
                    handle_terminal(event, &mut state);
                }
            }
            Scheduled::Motion | Scheduled::LiveFrame => unreachable!(),
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
    emitted: &mut BTreeMap<String, String>,
    last_status: &mut String,
    last_overlay: &mut String,
    last_composer: &mut String,
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
    let rows = linear_conversation_rows(&state.model);
    let present = rows.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();
    for (key, line) in rows {
        if emitted.get(&key) == Some(&line) {
            continue;
        }
        write_linear(&line)?;
        emitted.insert(key, line);
    }
    emitted.retain(|key, _| present.contains(key));
    let overlay = crate::view::linear_overlay(&state.model);
    if *last_overlay != overlay {
        if !overlay.is_empty() {
            write_linear(&overlay)?;
        }
        *last_overlay = overlay;
    }
    let composer = crate::view::linear_composer_status(&state.model);
    if *last_composer != composer {
        write_linear(composer)?;
        *last_composer = composer.into();
    }
    Ok(())
}

fn linear_conversation_rows(model: &AppModel) -> Vec<(String, String)> {
    model
        .durable_children()
        .map(|item| {
            let role = match item.role {
                TimelineRole::User => "You",
                TimelineRole::Agent => "Garive",
                TimelineRole::Status => "Activity",
            };
            (
                item.stable_key.clone(),
                format!("{role}: {}", crate::view::linear_safe(&item.text)),
            )
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{TimelineItem, TimelineRole, TimelineTone, TurnBlock, TurnBlockKey};

    fn item(key: &str, role: TimelineRole, text: &str) -> TimelineItem {
        TimelineItem {
            stable_key: key.into(),
            position: 1,
            role,
            tone: TimelineTone::Neutral,
            text: text.into(),
        }
    }

    #[test]
    fn linear_rows_follow_turn_block_semantic_child_order() {
        let model = AppModel {
            turn_blocks: vec![TurnBlock {
                key: TurnBlockKey {
                    session_id: "session".into(),
                    turn_id: "turn".into(),
                },
                user: item("user", TimelineRole::User, "question"),
                activities: vec![item("activity", TimelineRole::Status, "working")],
                committed_answer: Some(item("answer", TimelineRole::Agent, "done")),
                outcome: Some(item("outcome", TimelineRole::Status, "stopped")),
            }],
            ..Default::default()
        };

        assert_eq!(
            linear_conversation_rows(&model),
            vec![
                ("user".into(), "You: question".into()),
                ("activity".into(), "Activity: working".into()),
                ("answer".into(), "Garive: done".into()),
                ("outcome".into(), "Activity: stopped".into()),
            ]
        );
    }
}
