use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

use crate::{
    application::{AppModel, BootState, ConnectionState, ExecutionState, Overlay, TimelineRole},
    Theme,
};

pub(crate) fn render(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    if area.width < 20 || area.height < 8 {
        Paragraph::new("Need 20x8")
            .style(palette(theme).muted)
            .render(area, buffer);
        return;
    }
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);
    render_header(model, theme, vertical[0], buffer);
    render_body(model, theme, vertical[1], buffer);
    render_composer(model, theme, vertical[2], buffer);
    render_footer(model, theme, vertical[3], buffer);
    if let Some(overlay) = model.overlay {
        render_overlay(overlay, theme, area, buffer);
    }
}

fn render_header(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    let session = model
        .selected_session
        .as_deref()
        .map(short_id)
        .unwrap_or("No session");
    let label = format!(
        " Garive  {session}  {} | {} ",
        connection_name(model.connection),
        execution_name(model.execution)
    );
    Paragraph::new(label)
        .style(palette(theme).header)
        .render(area, buffer);
}

fn render_body(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    if area.width >= 100 {
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(1)])
            .split(area);
        let navigation = format!(
            "Sessions ({})\n\nCtrl+N  New\nCtrl+S  Open",
            model.session_count
        );
        Paragraph::new(navigation)
            .block(Block::default().borders(Borders::RIGHT))
            .style(palette(theme).muted)
            .render(horizontal[0], buffer);
        render_conversation(model, theme, horizontal[1], buffer);
    } else {
        render_conversation(model, theme, area, buffer);
    }
}

fn render_conversation(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    let mut lines = Vec::new();
    if model.timeline.is_empty() {
        lines.push(Line::from(match model.boot {
            BootState::Cold | BootState::Loading => "Loading durable Sessions…",
            BootState::NotConfigured => "No installed Agent definition",
            BootState::Degraded => "Host unavailable — open status for details",
            BootState::Ready => "Start a conversation from the composer",
        }));
    }
    for item in &model.timeline {
        let role = match item.role {
            TimelineRole::User => "You",
            TimelineRole::Agent => "Agent",
            TimelineRole::Status => "Status",
        };
        lines.push(Line::styled(role, palette(theme).role));
        lines.extend(
            safe_text(&item.text)
                .lines()
                .map(|line| Line::from(format!("  {line}"))),
        );
        lines.push(Line::default());
    }
    Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .render(area, buffer);
}

fn render_composer(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    let title = format!(" Compose  {}/4096 bytes ", model.composer.text().len());
    Paragraph::new(safe_text(model.composer.text()))
        .block(Block::default().title(title).borders(Borders::ALL))
        .style(palette(theme).normal)
        .render(area, buffer);
}

fn render_footer(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    let hint = if model.execution == ExecutionState::Following {
        " Esc cancel  Ctrl+P commands  ? help "
    } else {
        " Enter send  Ctrl+J newline  Ctrl+P commands  ? help "
    };
    Paragraph::new(hint)
        .style(palette(theme).muted)
        .render(area, buffer);
}

fn render_overlay(overlay: Overlay, theme: Theme, area: Rect, buffer: &mut Buffer) {
    let popup = centered(
        area,
        52.min(area.width.saturating_sub(4)),
        7.min(area.height),
    );
    Clear.render(popup, buffer);
    let title = match overlay {
        Overlay::CommandPalette => " Commands ",
        Overlay::Help => " Help ",
        Overlay::SessionPicker => " Sessions ",
        Overlay::PromptHistory => " Prompt history ",
        Overlay::Suspension => " Action required ",
        Overlay::UnknownCommand => " Command outcome unknown ",
        Overlay::ErrorDetails => " Error details ",
        Overlay::QuitConfirmation => " Quit Garive? ",
    };
    Paragraph::new("Enter confirm   Esc back")
        .block(Block::default().title(title).borders(Borders::ALL))
        .style(palette(theme).normal)
        .render(popup, buffer);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn safe_text(value: &str) -> String {
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
        ExecutionState::Idle => "idle",
        ExecutionState::Following => "running",
        ExecutionState::Suspended => "action required",
        ExecutionState::Failed => "failed",
    }
}

struct Palette {
    normal: Style,
    muted: Style,
    header: Style,
    role: Style,
}
fn palette(theme: Theme) -> Palette {
    let accent = match theme {
        Theme::Light => Color::Blue,
        Theme::Mono => Color::Reset,
        _ => Color::Cyan,
    };
    Palette {
        normal: Style::default(),
        muted: Style::default().fg(Color::DarkGray),
        header: Style::default().fg(accent).add_modifier(Modifier::BOLD),
        role: Style::default().fg(accent).add_modifier(Modifier::BOLD),
    }
}
