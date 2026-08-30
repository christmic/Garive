use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget, Wrap},
};
use std::collections::VecDeque;

use crate::{
    application::{
        AppModel, BootState, ConnectionState, ExecutionState, Overlay, TerminalSize, TimelineRole,
    },
    input::{command_matches, describe_schema, COMMAND_PALETTE},
    Theme,
};
use markdown::render_markdown;

pub(crate) fn render(
    model: &AppModel,
    theme: Theme,
    area: Rect,
    buffer: &mut Buffer,
) -> Option<(u16, u16)> {
    if !(TerminalSize {
        width: area.width,
        height: area.height,
    })
    .is_supported()
    {
        Paragraph::new("Need 20×8")
            .style(palette(theme).muted)
            .render(area, buffer);
        return None;
    }
    let composer_height = if area.height < 14 {
        3
    } else {
        (model.composer.line_count() as u16 + 2).clamp(3, 7)
    };
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(composer_height),
            Constraint::Length(1),
        ])
        .split(area);
    render_header(model, theme, vertical[0], buffer);
    render_body(model, theme, vertical[1], buffer);
    render_composer(model, theme, vertical[2], buffer);
    render_footer(model, theme, vertical[3], buffer);
    if let Some(overlay) = model.overlay {
        render_overlay(model, overlay, theme, area, buffer);
        None
    } else {
        (model.focus == crate::application::FocusTarget::Composer)
            .then(|| composer_cursor(model, vertical[2]))
            .flatten()
    }
}

fn render_header(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    let colors = palette(theme);
    Block::default()
        .style(colors.header_background)
        .render(area, buffer);
    let session = model
        .selected_session
        .as_deref()
        .map(|value| format!("Session {}", short_id(value)))
        .unwrap_or_else(|| "Workspace".into());
    let definition = model
        .definitions
        .first()
        .map(|item| short_id(&item.definition_id))
        .unwrap_or("No agent");
    let compact = area.width < 60;
    let status_width = if compact { 16 } else { 28 };
    let row =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(status_width)]).split(area);
    let identity = if compact {
        Line::styled("  GARIVE", colors.brand)
    } else {
        Line::from(vec![
            Span::styled("  GARIVE ", colors.brand),
            Span::styled(format!(" {definition}  /  {session}"), colors.header_text),
        ])
    };
    identity.render(row[0], buffer);
    let status = if compact {
        format!(
            "{} {} ",
            connection_icon(model.connection),
            execution_name(model.execution)
        )
    } else {
        format!(
            "{} {}  ·  {}  ",
            connection_icon(model.connection),
            connection_name(model.connection),
            execution_name(model.execution)
        )
    };
    Line::from(vec![
        Span::styled(connection_icon(model.connection), colors.connection),
        Span::raw(status[connection_icon(model.connection).len()..].to_owned()),
    ])
    .alignment(Alignment::Right)
    .style(colors.header_text)
    .render(row[1], buffer);
}

fn render_body(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    if area.width >= 100 {
        let rail_width = if area.width >= 160 { 34 } else { 28 };
        let horizontal =
            Layout::horizontal([Constraint::Length(rail_width), Constraint::Min(1)]).split(area);
        render_navigation(model, theme, horizontal[0], buffer);
        render_conversation(model, theme, horizontal[1], buffer);
    } else {
        render_conversation(model, theme, area, buffer);
    }
}

fn render_navigation(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    let colors = palette(theme);
    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" Sessions ", colors.title),
            Span::styled(format!("{} ", model.session_count), colors.badge),
        ]))
        .borders(Borders::RIGHT)
        .border_style(
            if model.focus == crate::application::FocusTarget::Navigation {
                colors.accent
            } else {
                colors.border
            },
        )
        .padding(Padding::new(1, 1, 1, 0));
    let inner = block.inner(area);
    block.render(area, buffer);
    let mut lines = Vec::new();
    if model.sessions.is_empty() {
        lines.push(Line::styled("No sessions yet", colors.muted));
        lines.push(Line::default());
        lines.push(Line::styled("Ctrl+N  Create one", colors.accent));
    } else {
        for (index, session) in model
            .sessions
            .iter()
            .take(inner.height.saturating_sub(3) as usize)
            .enumerate()
        {
            let selected = model.selected_session.as_deref() == Some(&session.session_id);
            let marker = if selected { "▸" } else { " " };
            let state = session.latest_turn_state.as_deref().unwrap_or("new");
            let style = if selected {
                colors.selected
            } else {
                colors.normal
            };
            lines.push(Line::styled(
                format!("{marker} New session · {}", short_tail(&session.session_id)),
                style,
            ));
            lines.push(Line::styled(
                format!("  {state}  ·  {} turns", session.turn_count),
                colors.muted,
            ));
            if index + 1 < model.sessions.len() {
                lines.push(Line::default());
            }
        }
    }
    Paragraph::new(lines).render(inner, buffer);
}

fn render_conversation(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    let colors = palette(theme);
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(
            if model.focus == crate::application::FocusTarget::Conversation {
                colors.accent
            } else {
                colors.border
            },
        )
        .padding(Padding::new(2, 2, 1, 0));
    let inner = block.inner(area);
    let window = (!model.timeline.is_empty())
        .then(|| conversation_window(model, theme, inner.width, inner.height));
    let title = if model.viewport.newer_updates > 0 {
        format!(
            " Conversation · {} newer updates ",
            model.viewport.newer_updates
        )
    } else if window.as_ref().is_some_and(|value| value.has_earlier) {
        " Conversation · ↑ earlier ".to_owned()
    } else {
        " Conversation ".to_owned()
    };
    let block = block.title(Line::styled(title, colors.title));
    block.render(area, buffer);
    let mut lines = Vec::new();
    let mut scroll = 0;
    if model.timeline.is_empty() {
        lines.push(Line::default());
        lines.push(Line::styled(empty_title(model.boot), colors.empty_title));
        lines.push(Line::styled(empty_detail(model.boot), colors.muted));
    } else if let Some(window) = window {
        lines = window.lines;
        scroll = window.scroll;
    }
    Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll.min(u16::MAX as usize) as u16, 0))
        .render(inner, buffer);
}

struct ConversationWindow {
    lines: Vec<Line<'static>>,
    scroll: usize,
    has_earlier: bool,
    #[cfg_attr(not(test), allow(dead_code))]
    laid_out: usize,
}

fn conversation_window(
    model: &AppModel,
    theme: Theme,
    width: u16,
    height: u16,
) -> ConversationWindow {
    let target_height = usize::from(height).saturating_add(4);
    let mut cells = VecDeque::new();
    let mut laid_out = 0;
    let mut measured_height: usize = 0;
    if model.viewport.follow_latest {
        for item in model.timeline.iter().rev() {
            let cell = render_timeline_item(item, theme);
            measured_height = measured_height.saturating_add(wrapped_height(&cell, width));
            cells.push_front(cell);
            laid_out += 1;
            if measured_height >= target_height {
                break;
            }
        }
    } else {
        let start = model
            .viewport
            .anchor_key
            .as_deref()
            .and_then(|key| {
                model
                    .timeline
                    .iter()
                    .position(|item| item.stable_key == key)
            })
            .unwrap_or(0);
        for item in model.timeline.iter().skip(start) {
            let cell = render_timeline_item(item, theme);
            measured_height = measured_height.saturating_add(wrapped_height(&cell, width));
            cells.push_back(cell);
            laid_out += 1;
            if measured_height >= target_height.saturating_add(model.viewport.source_line) {
                break;
            }
        }
    }
    let lines = cells.into_iter().flatten().collect::<Vec<_>>();
    let scroll = if model.viewport.follow_latest {
        wrapped_height(&lines, width).saturating_sub(usize::from(height))
    } else {
        model.viewport.source_line
    };
    ConversationWindow {
        lines,
        scroll,
        has_earlier: laid_out < model.timeline.len() || scroll > 0,
        laid_out,
    }
}

fn render_timeline_item(
    item: &crate::application::TimelineItem,
    theme: Theme,
) -> Vec<Line<'static>> {
    let colors = palette(theme);
    let mut lines = Vec::new();
    match item.role {
        TimelineRole::User => {
            lines.push(Line::from(vec![
                Span::styled("╭─ YOU ", colors.user),
                Span::styled(format!("#{}", item.position), colors.muted),
            ]));
            push_content(&mut lines, &item.text, "│  ", colors.normal);
            lines.push(Line::styled("╰─", colors.user));
        }
        TimelineRole::Agent => {
            lines.push(Line::from(vec![
                Span::styled("◆  GARIVE ", colors.agent),
                Span::styled(format!("#{}", item.position), colors.muted),
            ]));
            lines.extend(render_markdown(
                &item.text,
                "   ",
                colors.normal,
                colors.agent,
                colors.muted,
            ));
        }
        TimelineRole::Status => lines.push(Line::from(vec![
            Span::styled("  ◌  ", colors.activity),
            Span::styled(safe_text(&item.text), colors.muted),
        ])),
    }
    lines.push(Line::default());
    lines
}

fn wrapped_height(lines: &[Line<'static>], width: u16) -> usize {
    let width = usize::from(width.max(1));
    lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(width))
        .sum()
}

fn push_content(lines: &mut Vec<Line<'static>>, text: &str, prefix: &str, style: Style) {
    lines.extend(
        safe_text(text)
            .lines()
            .map(|line| Line::styled(format!("{prefix}{line}"), style)),
    );
}

fn render_composer(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    let colors = palette(theme);
    let title = if model.execution == ExecutionState::Suspended {
        " Reply to request "
    } else {
        " Message "
    };
    let block = Block::default()
        .title(Line::styled(title, colors.title))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(
            if model.focus == crate::application::FocusTarget::Composer {
                colors.composer_border
            } else {
                colors.border
            },
        )
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buffer);
    let text = if model.composer.text().is_empty() {
        Text::from(Line::styled("›  Ask Garive anything…", colors.placeholder))
    } else {
        Text::from(safe_text(model.composer.text()))
    };
    Paragraph::new(text)
        .style(colors.normal)
        .render(inner, buffer);
}

fn render_footer(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    let colors = palette(theme);
    let cells = Layout::horizontal([Constraint::Min(1), Constraint::Length(14)]).split(area);
    let hint = if area.width < 60 && model.execution == ExecutionState::Following {
        " Esc cancel · ? help"
    } else if area.width < 60 {
        " Enter send · ? help"
    } else if model.execution == ExecutionState::Following {
        " Esc cancel   Ctrl+S sessions   Ctrl+P commands   ? help"
    } else {
        " Enter send   Ctrl+J newline   Ctrl+P commands   ? help"
    };
    Line::styled(model.notice.as_deref().unwrap_or(hint), colors.muted).render(cells[0], buffer);
    Line::styled(
        format!("{}/4096 bytes ", model.composer.text().len()),
        colors.muted,
    )
    .alignment(Alignment::Right)
    .render(cells[1], buffer);
}

fn render_overlay(
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
    let popup = centered(
        area,
        popup_width.min(area.width.saturating_sub(4)),
        height.min(area.height.saturating_sub(2)),
    );
    Clear.render(popup, buffer);
    Paragraph::new(content)
        .block(
            Block::default()
                .title(Line::styled(title, colors.title))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(colors.overlay_border)
                .padding(Padding::new(2, 2, 1, 1)),
        )
        .style(colors.normal)
        .wrap(Wrap { trim: false })
        .render(popup, buffer);
}

fn session_picker_text(model: &AppModel) -> String {
    let filter = model.session_filter.to_lowercase();
    let mut rows = vec![format!(
        "Filter: {}",
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
                filter.is_empty() || session.session_id.to_lowercase().contains(&filter)
            })
            .enumerate()
            .map(|(index, session)| {
                let marker = if index == model.session_selection {
                    "›"
                } else {
                    " "
                };
                format!(
                    "{marker} New session · {}   {}",
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
        "Search: {}",
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
        "Search: {}",
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

fn composer_cursor(model: &AppModel, area: Rect) -> Option<(u16, u16)> {
    let inner_width = area.width.saturating_sub(4);
    let inner_height = area.height.saturating_sub(2);
    if inner_width == 0 || inner_height == 0 {
        return None;
    }
    Some((
        area.x + 2 + (model.composer.display_column() as u16).min(inner_width - 1),
        area.y + 1 + (model.composer.cursor_line() as u16).min(inner_height - 1),
    ))
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

pub(super) fn safe_text(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\n' | '\t' => character.to_string(),
            '\u{202a}'..='\u{202e}' => "⟦bidi⟧".into(),
            '\u{2066}' => "⟦LRI⟧".into(),
            '\u{2067}' => "⟦RLI⟧".into(),
            '\u{2068}' => "⟦FSI⟧".into(),
            '\u{2069}' => "⟦PDI⟧".into(),
            value if value.is_control() => "�".into(),
            value => value.to_string(),
        })
        .collect()
}

fn short_id(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}
fn short_tail(value: &str) -> &str {
    value.get(value.len().saturating_sub(6)..).unwrap_or(value)
}
fn connection_icon(value: ConnectionState) -> &'static str {
    if value == ConnectionState::Online {
        "●"
    } else {
        "○"
    }
}
fn connection_name(value: ConnectionState) -> &'static str {
    match value {
        ConnectionState::Connecting => "connecting",
        ConnectionState::Online => "online",
        ConnectionState::Disconnected { .. } => "disconnected",
        ConnectionState::Reconnecting { .. } => "reconnecting",
        ConnectionState::Unavailable { .. } => "unavailable",
    }
}
fn execution_name(value: ExecutionState) -> &'static str {
    match value {
        ExecutionState::Idle => "ready",
        ExecutionState::Following => "running",
        ExecutionState::Suspended => "action required",
        ExecutionState::Failed => "failed",
    }
}
fn empty_title(value: BootState) -> &'static str {
    match value {
        BootState::Cold | BootState::Loading => "  Connecting to your durable workspace…",
        BootState::NotConfigured => "  No Agent is installed",
        BootState::Degraded => "  Garive Host is unavailable",
        BootState::Ready => "  A quiet place to get things done",
    }
}
fn empty_detail(value: BootState) -> &'static str {
    match value {
        BootState::Cold | BootState::Loading => "  Sessions and activity will appear here.",
        BootState::NotConfigured => "  Install an Agent definition before creating a Session.",
        BootState::Degraded => "  Open /status for safe recovery details.",
        BootState::Ready => "  Write below, or press Ctrl+N for a fresh Session.",
    }
}

struct Palette {
    normal: Style,
    muted: Style,
    accent: Style,
    title: Style,
    badge: Style,
    brand: Style,
    header_text: Style,
    header_background: Style,
    connection: Style,
    border: Style,
    composer_border: Style,
    overlay_border: Style,
    selected: Style,
    user: Style,
    agent: Style,
    activity: Style,
    placeholder: Style,
    empty_title: Style,
}
fn palette(theme: Theme) -> Palette {
    let mono = theme == Theme::Mono;
    let (accent, violet, surface, text, muted) = if theme == Theme::Light {
        (
            Color::Blue,
            Color::Magenta,
            Color::Rgb(235, 238, 244),
            Color::Black,
            Color::DarkGray,
        )
    } else if mono {
        (
            Color::Reset,
            Color::Reset,
            Color::Reset,
            Color::Reset,
            Color::DarkGray,
        )
    } else {
        (
            Color::Rgb(72, 202, 228),
            Color::Rgb(189, 147, 249),
            Color::Rgb(24, 28, 38),
            Color::Rgb(232, 235, 242),
            Color::Rgb(126, 134, 151),
        )
    };
    let bold = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    Palette {
        normal: Style::default().fg(text),
        muted: Style::default().fg(muted),
        accent: bold,
        title: bold,
        badge: Style::default().fg(violet),
        brand: Style::default()
            .fg(if mono { text } else { Color::Black })
            .bg(accent)
            .add_modifier(Modifier::BOLD),
        header_text: Style::default().fg(text).bg(surface),
        header_background: Style::default().bg(surface),
        connection: Style::default().fg(if mono { text } else { Color::Green }),
        border: Style::default().fg(muted),
        composer_border: Style::default().fg(accent),
        overlay_border: Style::default().fg(violet),
        selected: Style::default().fg(accent).add_modifier(Modifier::BOLD),
        user: Style::default().fg(violet).add_modifier(Modifier::BOLD),
        agent: bold,
        activity: Style::default().fg(if mono { text } else { Color::Yellow }),
        placeholder: Style::default().fg(muted).add_modifier(Modifier::ITALIC),
        empty_title: Style::default().fg(text).add_modifier(Modifier::BOLD),
    }
}
mod markdown;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{TimelineItem, TimelineRole};

    #[test]
    fn latest_window_layout_is_independent_of_history_length() {
        let mut model = AppModel::default();
        for position in 1..=10_000 {
            model.timeline.push(TimelineItem {
                stable_key: format!("item-{position}"),
                position,
                role: TimelineRole::Agent,
                text: "A short bounded response.".into(),
            });
        }

        let window = conversation_window(&model, Theme::Dark, 90, 30);

        assert!(window.laid_out < 30);
    }
}
