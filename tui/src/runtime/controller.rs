use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::time::Duration;

use super::app::RuntimeState;
use crate::application::{AppAction, ExecutionState, FocusTarget, Overlay, TerminalSize};
use crate::input::{resolve_shortcut, HistoryDraft, HistoryRecall, ShortcutIntent};

mod actions;
mod mouse;
mod navigation;
mod overlay;

pub(super) use actions::replay_pending;
use actions::{cancel, copy_composer_selection, create_session, submit};
use navigation::{cycle_focus, open_command_palette, open_prompt_history, open_session_picker};

pub(super) fn handle_terminal(event: Event, state: &mut RuntimeState) {
    match event {
        Event::Resize(width, height) => {
            state.composer_clicks.reset();
            state.dispatch(AppAction::TerminalResized(TerminalSize { width, height }));
            crate::view::reflow_conversation(
                &mut state.model,
                state.config.theme,
                &mut state.render_cache,
            );
        }
        Event::FocusGained => state.dispatch(AppAction::TerminalFocusChanged(true)),
        Event::FocusLost => {
            state.composer_mouse_selecting = false;
            state.composer_clicks.reset();
            state.dispatch(AppAction::TerminalFocusChanged(false));
        }
        Event::Paste(text) => {
            state.composer_clicks.reset();
            let previous = state.model.composer.text().to_owned();
            if state.composer_is_frozen() {
                state.explain_frozen_composer();
            } else {
                let _ = state.model.composer.insert(&text);
            }
            if previous != state.model.composer.text() {
                state.model.prompt_history_browser.reset();
            }
            sync_command_suggestions(state, &previous);
        }
        Event::Mouse(event) => mouse::handle(event, state),
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            state.composer_clicks.reset();
            handle_key(key, state);
        }
        _ => {}
    }
}

fn handle_key(key: KeyEvent, state: &mut RuntimeState) {
    let previous = state.model.composer.text().to_owned();
    handle_key_inner(key, state);
    if previous != state.model.composer.text() && !matches!(key.code, KeyCode::Up | KeyCode::Down) {
        state.model.prompt_history_browser.reset();
    }
    sync_command_suggestions(state, &previous);
}

fn handle_key_inner(key: KeyEvent, state: &mut RuntimeState) {
    let shortcut = resolve_shortcut(key);
    if shortcut == Some(ShortcutIntent::Quit) {
        state.dispatch(AppAction::QuitRequested);
        return;
    }
    if overlay::handle(key, state) {
        return;
    }
    if shortcut.is_some_and(|intent| handle_shortcut(intent, state)) {
        return;
    }
    if handle_command_suggestion_key(key, state) {
        return;
    }
    if key.code == KeyCode::Char('?')
        && (state.model.composer.text().is_empty() || state.composer_is_frozen())
    {
        state.dispatch(AppAction::OverlayOpened(Overlay::Help));
        return;
    }
    if state.model.focus == FocusTarget::Conversation {
        match key.code {
            KeyCode::Up => scroll_conversation(state, -1),
            KeyCode::Down => scroll_conversation(state, 1),
            KeyCode::PageUp => {
                let page = crate::view::conversation_page_cells(&state.model) as isize;
                scroll_conversation(state, -page);
            }
            KeyCode::PageDown => {
                let page = crate::view::conversation_page_cells(&state.model) as isize;
                scroll_conversation(state, page);
            }
            KeyCode::Home => state.model.jump_to_oldest(),
            KeyCode::End => state.model.follow_latest(),
            _ => {}
        }
        if matches!(
            key.code,
            KeyCode::Up
                | KeyCode::Down
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::End
        ) {
            return;
        }
    }
    if state.composer_is_frozen()
        && matches!(
            key.code,
            KeyCode::Char(_)
                | KeyCode::Backspace
                | KeyCode::Delete
                | KeyCode::Enter
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
        )
    {
        state.explain_frozen_composer();
        return;
    }
    match key.code {
        KeyCode::Tab => cycle_focus(state, key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::BackTab => cycle_focus(state, true),
        KeyCode::Char(character) => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Composer));
            let _ = state.model.composer.insert(&character.to_string());
        }
        KeyCode::Esc if state.model.execution == ExecutionState::Following => cancel(state),
        _ => {}
    }
    if matches!(
        key.code,
        KeyCode::Tab | KeyCode::BackTab | KeyCode::Char(_) | KeyCode::Esc
    ) || state.model.focus != FocusTarget::Composer
    {
        return;
    }
    match key.code {
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::ALT) => {
            state.model.composer.delete_word_left();
        }
        KeyCode::Delete if key.modifiers.contains(KeyModifiers::ALT) => {
            state.model.composer.delete_word_right();
        }
        KeyCode::Backspace => {
            state.model.composer.backspace();
        }
        KeyCode::Delete => {
            state.model.composer.delete();
        }
        KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => state
            .model
            .composer
            .move_word_left(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => state
            .model
            .composer
            .move_word_right(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Left => state
            .model
            .composer
            .move_left(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Right => state
            .model
            .composer
            .move_right(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Up | KeyCode::Down => {
            let direction = if key.code == KeyCode::Up { -1 } else { 1 };
            let (target, preferred) =
                crate::view::composer_vertical_target(&state.model, direction);
            if !key.modifiers.contains(KeyModifiers::SHIFT)
                && !state.model.composer.has_selection()
                && target == state.model.composer.cursor_grapheme()
                && browse_prompt_history(state, direction)
            {
                return;
            }
            state.model.composer.apply_visual_vertical_move(
                target,
                preferred,
                direction,
                key.modifiers.contains(KeyModifiers::SHIFT),
            );
        }
        KeyCode::Home | KeyCode::End => {
            let direction = if key.code == KeyCode::Home { -1 } else { 1 };
            let target = crate::view::composer_line_edge_target(&state.model, direction);
            state
                .model
                .composer
                .place_cursor(target, key.modifiers.contains(KeyModifiers::SHIFT));
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
            let _ = state.model.composer.insert("\n");
        }
        KeyCode::Enter => submit(state),
        _ => {}
    }
}

pub(super) fn scroll_conversation(state: &mut RuntimeState, cells: isize) {
    crate::view::scroll_conversation(
        &mut state.model,
        state.config.theme,
        &mut state.render_cache,
        cells,
    );
}

fn handle_shortcut(intent: ShortcutIntent, state: &mut RuntimeState) -> bool {
    use ShortcutIntent::*;
    match intent {
        Quit => return false,
        ClearOrCancel => handle_ctrl_c(state),
        NewSession => {
            create_session(state);
        }
        OpenSessions => open_session_picker(state),
        OpenCommands => open_command_palette(state),
        OpenHistory => open_prompt_history(state),
        Redraw if state.model.focus == FocusTarget::Conversation => state.force_redraw = true,
        DocumentStart if state.model.focus == FocusTarget::Conversation => {
            state.model.jump_to_oldest();
        }
        DocumentEnd if state.model.focus == FocusTarget::Conversation => {
            state.model.follow_latest();
        }
        intent if state.model.focus != FocusTarget::Composer && is_composer_shortcut(intent) => {
            return true;
        }
        intent if state.composer_is_frozen() && is_composer_shortcut(intent) => {
            state.explain_frozen_composer();
        }
        OpenExternalEditor => super::external_editor::request(state),
        InsertNewline => {
            let _ = state.model.composer.insert("\n");
        }
        DocumentStart => state.model.composer.move_document_start(false),
        DocumentEnd => state.model.composer.move_document_end(false),
        Undo => {
            state.model.composer.undo();
        }
        Redo => {
            state.model.composer.redo();
        }
        KillStart => {
            state.model.composer.kill_to_logical_line_start();
        }
        KillEnd => {
            state.model.composer.kill_to_logical_line_end();
        }
        Yank => {
            let _ = state.model.composer.yank();
        }
        CopySelection => copy_composer_selection(state, false),
        LogicalLineStart => state.model.composer.move_logical_line_start(false),
        LogicalLineEnd => state.model.composer.move_logical_line_end(false),
        GraphemeLeft => state.model.composer.move_left(false),
        GraphemeRight => state.model.composer.move_right(false),
        WordLeft => state.model.composer.move_word_left(false),
        WordRight => state.model.composer.move_word_right(false),
        DeleteBackward => {
            state.model.composer.backspace();
        }
        DeleteForward => {
            state.model.composer.delete();
        }
        DeleteWordBackward => {
            state.model.composer.delete_word_left();
        }
        DeleteWordForward => {
            state.model.composer.delete_word_right();
        }
        Redraw => {}
    }
    true
}

fn is_composer_shortcut(intent: ShortcutIntent) -> bool {
    !matches!(
        intent,
        ShortcutIntent::Quit
            | ShortcutIntent::ClearOrCancel
            | ShortcutIntent::NewSession
            | ShortcutIntent::OpenSessions
            | ShortcutIntent::OpenCommands
            | ShortcutIntent::OpenHistory
            | ShortcutIntent::Redraw
    )
}

fn browse_prompt_history(state: &mut RuntimeState, direction: i8) -> bool {
    let recall = if direction < 0 {
        let current = HistoryDraft {
            text: state.model.composer.text().to_owned(),
            cursor_grapheme: state.model.composer.cursor_grapheme(),
        };
        state
            .model
            .prompt_history_browser
            .older(&state.model.prompt_history, current)
    } else if state.model.prompt_history_browser.is_active() {
        state
            .model
            .prompt_history_browser
            .newer(&state.model.prompt_history)
    } else {
        None
    };
    let Some(recall) = recall else {
        return false;
    };
    match recall {
        HistoryRecall::Entry(text) => {
            if state.model.composer.replace(&text).is_ok() {
                state.model.composer.move_document_end(false);
            }
        }
        HistoryRecall::Draft(draft) => {
            if state.model.composer.replace(&draft.text).is_ok() {
                state
                    .model
                    .composer
                    .place_cursor(draft.cursor_grapheme, false);
            }
        }
    }
    true
}

fn handle_command_suggestion_key(key: KeyEvent, state: &mut RuntimeState) -> bool {
    if state.config.screen_reader || !state.model.command_suggestions_active() {
        return false;
    }
    let count = state.model.matching_command_suggestion_indices().len();
    match key.code {
        KeyCode::Up | KeyCode::BackTab => {
            state.model.command_suggestion_selection = state
                .model
                .command_suggestion_selection
                .checked_sub(1)
                .unwrap_or(count - 1);
            true
        }
        KeyCode::Down => {
            state.model.command_suggestion_selection =
                (state.model.command_suggestion_selection + 1) % count;
            true
        }
        KeyCode::Tab | KeyCode::Enter => {
            accept_command_suggestion(state);
            true
        }
        KeyCode::Esc => {
            state.model.command_suggestion_dismissed = Some(state.model.composer.text().to_owned());
            true
        }
        _ => false,
    }
}

pub(super) fn accept_command_suggestion(state: &mut RuntimeState) {
    let matches = state.model.matching_command_suggestion_indices();
    let Some(index) = matches
        .get(state.model.command_suggestion_selection)
        .copied()
    else {
        return;
    };
    let command = crate::input::COMMAND_PALETTE[index];
    if let Some(reason) = command.unavailable_reason(state.model.command_context()) {
        state.model.notice = Some(format!("Command unavailable: {reason}."));
        return;
    }
    let completion = if command.accepts_args {
        format!("{} ", command.input)
    } else {
        command.input.to_owned()
    };
    let _ = state.model.composer.replace(&completion);
    state.model.command_suggestion_dismissed = Some(completion);
}

fn sync_command_suggestions(state: &mut RuntimeState, previous: &str) {
    let current = state.model.composer.text();
    if previous != current {
        if state.model.command_suggestion_dismissed.as_deref() != Some(current) {
            state.model.command_suggestion_dismissed = None;
        }
        state.model.command_suggestion_selection = 0;
    }
    let count = state.model.matching_command_suggestion_indices().len();
    state.model.command_suggestion_selection = state
        .model
        .command_suggestion_selection
        .min(count.saturating_sub(1));
}

fn handle_ctrl_c(state: &mut RuntimeState) {
    if state.model.execution == ExecutionState::Following {
        cancel(state);
        return;
    }
    if state.composer_is_frozen() {
        state.explain_frozen_composer();
        return;
    }
    if state.model.composer.has_selection() {
        state.model.composer.clear_selection();
        state.last_empty_ctrl_c = None;
    } else if !state.model.composer.text().is_empty() {
        state.model.composer.clear();
        state.last_empty_ctrl_c = None;
    } else {
        let now = std::time::Instant::now();
        if state
            .last_empty_ctrl_c
            .is_some_and(|previous| now.duration_since(previous) <= Duration::from_millis(1_500))
        {
            state.last_empty_ctrl_c = None;
            state.dispatch(AppAction::QuitRequested);
        } else {
            state.last_empty_ctrl_c = Some(now);
            state.model.notice = Some("Press Ctrl+C again to quit.".into());
        }
    }
}
