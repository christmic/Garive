//! Full-screen frame rendering and explicit back-buffer invalidation.

use std::io::{self, Write};

use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{application::TerminalSize, view, TuiError};

use super::RuntimeState;

pub(super) fn draw(
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
        clear_fullscreen(terminal)?;
    }
    terminal
        .draw(|frame| {
            let area = frame.area();
            let size = TerminalSize {
                width: area.width,
                height: area.height,
            };
            if state.model.terminal_size != size {
                crate::application::reduce(
                    &mut state.model,
                    crate::application::AppAction::TerminalResized(size),
                );
            }
            let cursor = if state.config.reduced_motion {
                view::render_cached(
                    &state.model,
                    state.theme(),
                    area,
                    frame.buffer_mut(),
                    &mut state.render_cache,
                )
            } else {
                view::render_cached_with_motion(
                    &state.model,
                    state.theme(),
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

pub(super) fn clear_fullscreen(
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
) -> Result<(), TuiError> {
    let area = terminal.size().map_err(|_| TuiError::TerminalIo)?.into();
    terminal.resize(area).map_err(|_| TuiError::TerminalIo)
}
