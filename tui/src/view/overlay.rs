use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget, Wrap},
};

use crate::{
    application::{AppModel, ExecutionState, Overlay},
    input::{command_matches, describe_schema, COMMAND_PALETTE},
    Theme,
};

use super::{palette, primitives::centered_popup, safe_text, short_tail};

pub(super) fn render_overlay(
    model: &AppModel,
    overlay: Overlay,
    theme: Theme,
    area: Rect,
    buffer: &mut Buffer,
) {
    let colors = palette(theme);
    let (title, content, height) = match overlay {
        Overlay::CommandPalette => (
            " Command palette ",
            palette_text(model),
            (COMMAND_PALETTE.len() as u16 + 7).clamp(12, 22),
        ),
        Overlay::Help => (" Keyboard guide ", "Enter  Send message       Ctrl+J  New line\nCtrl+N Create session      Ctrl+S  Sessions\nCtrl+P Command palette     Ctrl+R  Prompt history\nEsc    Cancel running turn Ctrl+Q  Quit\n\nAll durable truth comes from the local Garive Host.".into(), 10),
        Overlay::SessionPicker => (" Switch session ", session_picker_text(model), (model.sessions.len() as u16 + 5).clamp(7, 16)),
        Overlay::PromptHistory => (" Prompt history ", history_text(model), (model.prompt_history.len() as u16 + 5).clamp(7, 16)),
        Overlay::Suspension => (" Action required ", suspension_text(model), 11),
        Overlay::UnknownCommand => (" Unknown command ", format!("{}\n\nEnter  Exact retry     A  Abandon local record", model.notice.as_deref().unwrap_or("Nothing was sent to the Host.")), 8),
        Overlay::ErrorDetails => (" Status details ", model.notice.clone().unwrap_or_else(|| "No additional safe details.".into()), 7),
        Overlay::EphemeralConfirmation => (" Ephemeral mode ", "A lost response cannot be recovered after exit.\n\nEnter  Accept for this run     Esc  Cancel".into(), 7),
        Overlay::QuitConfirmation => (" Quit Garive? ", "Your Sessions stay durable in the Host.\n\nEnter  Quit     Esc  Keep working".into(), 7),
    };
    let popup_width = if overlay == Overlay::CommandPalette {
        74
    } else {
        62
    };
    let popup = centered_popup(
        area,
        popup_width.min(area.width.saturating_sub(4)),
        height.min(area.height.saturating_sub(2)),
    );
    buffer.set_style(area, colors.modal_backdrop);
    Clear.render(popup, buffer);
    let block = Block::default()
        .title(Line::styled(title, colors.title))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(colors.overlay_border)
        .padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(popup);
    Paragraph::new(content)
        .block(block)
        .style(colors.normal)
        .wrap(Wrap { trim: false })
        .render(popup, buffer);
    if let Some(row) = selection_row(model, overlay) {
        if row < inner.height {
            buffer.set_style(
                Rect::new(inner.x, inner.y + row, inner.width, 1),
                colors.selection_row,
            );
        }
    }
}

fn selection_row(model: &AppModel, overlay: Overlay) -> Option<u16> {
    let selection = match overlay {
        Overlay::CommandPalette => model.command_selection,
        Overlay::SessionPicker => model.session_selection,
        Overlay::PromptHistory => model.history_selection,
        _ => return None,
    };
    u16::try_from(selection).ok()?.checked_add(1)
}

fn session_picker_text(model: &AppModel) -> String {
    let filter = model.session_filter.to_lowercase();
    let mut rows = vec![format!(
        "Filter  {}",
        if model.session_filter.is_empty() {
            "type to search".into()
        } else {
            safe_text(&model.session_filter)
        }
    )];
    rows.extend(
        model
            .sessions
            .iter()
            .filter(|session| {
                filter.is_empty()
                    || session.session_id.to_lowercase().contains(&filter)
                    || session.definition_id.to_lowercase().contains(&filter)
            })
            .enumerate()
            .map(|(index, session)| {
                let marker = if index == model.session_selection {
                    "›"
                } else {
                    " "
                };
                format!(
                    "{marker} {} · {}   {}",
                    super::short_id(&session.definition_id),
                    short_tail(&session.session_id),
                    session.latest_turn_state.as_deref().unwrap_or("new")
                )
            })
            .collect::<Vec<_>>(),
    );
    if rows.len() == 1 {
        rows.push("  No matching Sessions".into());
    }
    rows.push(String::new());
    rows.push(if model.sessions_loading {
        "Loading older Sessions…".into()
    } else if model.sessions_next_before.is_some() {
        "↑/↓ select · ↓ at end loads more · Enter open · Esc close".into()
    } else {
        "↑/↓ select   Enter open   Esc close".into()
    });
    rows.join("\n")
}

fn history_text(model: &AppModel) -> String {
    let filter = model.history_filter.to_lowercase();
    let mut rows = vec![format!(
        "Search  {}",
        if model.history_filter.is_empty() {
            "type to search".into()
        } else {
            safe_text(&model.history_filter)
        }
    )];
    rows.extend(
        model
            .prompt_history
            .iter()
            .filter(|text| filter.is_empty() || text.to_lowercase().contains(&filter))
            .take(10)
            .enumerate()
            .map(|(index, text)| {
                let marker = if index == model.history_selection {
                    "›"
                } else {
                    " "
                };
                let first = text.lines().next().unwrap_or_default();
                let preview = first.chars().take(46).collect::<String>();
                format!("{marker} {preview}")
            })
            .collect::<Vec<_>>(),
    );
    if rows.len() == 1 {
        rows.push("  No local prompt history".into());
    }
    rows.push(String::new());
    rows.push("↑/↓ select   Enter restore   Esc close".into());
    rows.join("\n")
}

fn palette_text(model: &AppModel) -> String {
    let mut rows = vec![format!(
        "Search  {}",
        if model.command_filter.is_empty() {
            "type to search".into()
        } else {
            safe_text(&model.command_filter)
        }
    )];
    rows.extend(
        COMMAND_PALETTE
            .iter()
            .filter(|(name, help)| command_matches(name, help, &model.command_filter))
            .enumerate()
            .map(|(index, (name, help))| {
                let marker = if index == model.command_selection {
                    "›"
                } else {
                    " "
                };
                let disabled = match *name {
                    "/new" if model.definitions.is_empty() => "  · no Agent installed",
                    "/retry" if !model.has_pending_command => "  · no pending command",
                    "/cancel" if model.execution != ExecutionState::Following => {
                        "  · no Turn running"
                    }
                    "/copy session-id" if model.selected_session.is_none() => {
                        "  · no Session selected"
                    }
                    _ => "",
                };
                format!("{marker} {name:<12} {help}{disabled}")
            })
            .collect::<Vec<_>>(),
    );
    if rows.len() == 1 {
        rows.push("  No matching commands".into());
    }
    rows.push(String::new());
    rows.push("↑/↓ select   Enter run   Esc close".into());
    rows.join("\n")
}

fn suspension_text(model: &AppModel) -> String {
    let prompt = model
        .suspension
        .as_ref()
        .map(|value| value.prompt_json.as_str())
        .unwrap_or("Action required");
    let guidance = model
        .suspension
        .as_ref()
        .and_then(|value| value.response_schema_json.as_deref())
        .map(describe_schema)
        .unwrap_or("Enter a text response.");
    format!(
        "{}\n\n{}\n\nEnter  Reply now     Ctrl+Q  Leave safely",
        safe_text(prompt),
        guidance
    )
}
