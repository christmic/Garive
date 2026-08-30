use crate::{
    application::{AppModel, BootState, TerminalSize},
    Theme,
};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Widget},
};

mod command_suggestions;
mod composer;
mod conversation;
mod footer;
mod linear;
mod markdown_syntax;
mod markdown_table;
mod motion;
mod overlay;
pub(crate) mod presentation;
mod primitives;
mod session;
mod style;
mod title;

use conversation::render_conversation;
pub(crate) use conversation::RenderCache;
use footer::render_footer;
pub(crate) use linear::{overlay_text as linear_overlay, safe as linear_safe};
use motion::status_motion;
pub(crate) use motion::{status_motion_enabled, MotionFrame};
use overlay::render_overlay;
use primitives::{centered_column, status_chip};
use session::{rail_lines, rail_window};
use style::{connection_name, connection_style, execution_style, palette};
pub(crate) use title::terminal_title;

pub(crate) fn render_cached(
    model: &AppModel,
    theme: Theme,
    area: Rect,
    buffer: &mut Buffer,
    cache: &mut RenderCache,
) -> Option<(u16, u16)> {
    render_cached_with_motion(model, theme, MotionFrame::reduced(), area, buffer, cache)
}

pub(crate) fn render_cached_with_motion(
    model: &AppModel,
    theme: Theme,
    motion: MotionFrame,
    area: Rect,
    buffer: &mut Buffer,
    cache: &mut RenderCache,
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
    let frame = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);
    render_header(model, theme, motion, frame[0], buffer);
    let composer = if area.width >= 100 {
        let rail_width = if area.width >= 160 { 34 } else { 28 };
        let workspace = Layout::horizontal([Constraint::Length(rail_width), Constraint::Min(1)])
            .split(frame[1]);
        render_navigation(model, theme, workspace[0], buffer);
        let content = if area.width >= 160 {
            centered_column(workspace[1], 114)
        } else {
            workspace[1]
        };
        render_content(model, theme, content, buffer, cache)
    } else {
        render_content(model, theme, frame[1], buffer, cache)
    };
    if let Some(overlay) = model.overlay {
        render_overlay(model, overlay, theme, area, buffer);
        None
    } else {
        (model.focus == crate::application::FocusTarget::Composer)
            .then(|| composer::cursor(model, composer))
            .flatten()
    }
}

fn render_content(
    model: &AppModel,
    theme: Theme,
    area: Rect,
    buffer: &mut Buffer,
    cache: &mut RenderCache,
) -> Rect {
    let rows = content_rows(model, area);
    render_conversation(model, theme, rows[0], buffer, cache);
    composer::render(model, palette(theme), rows[1], buffer);
    render_footer(model, theme, rows[2], buffer);
    command_suggestions::render(model, rows[1], palette(theme), buffer);
    rows[1]
}

fn content_rows(model: &AppModel, area: Rect) -> std::rc::Rc<[Rect]> {
    let composer_height = if area.height < 12 {
        3
    } else {
        composer::desired_height(&model.composer, area.width).clamp(3, 7)
    };
    Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(composer_height),
        Constraint::Length(1),
    ])
    .split(area)
}

fn main_content_area(area: Rect) -> Rect {
    let body = Rect::new(
        area.x,
        area.y.saturating_add(2),
        area.width,
        area.height.saturating_sub(2),
    );
    if area.width < 100 {
        return body;
    }
    let rail_width = if area.width >= 160 { 34 } else { 28 };
    let workspace = Rect::new(
        body.x.saturating_add(rail_width),
        body.y,
        body.width.saturating_sub(rail_width),
        body.height,
    );
    if area.width >= 160 {
        centered_column(workspace, 114)
    } else {
        workspace
    }
}

pub(crate) fn command_suggestion_hit_test(
    model: &AppModel,
    column: u16,
    row: u16,
) -> Option<usize> {
    let full = Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height);
    let composer = content_rows(model, main_content_area(full))[1];
    command_suggestions::selection_at(model, composer, column, row)
}

pub(crate) fn composer_hit_test(
    model: &AppModel,
    column: u16,
    row: u16,
    clamp: bool,
) -> Option<usize> {
    let full = Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height);
    let composer = content_rows(model, main_content_area(full))[1];
    composer::selection_at(model, composer, column, row, clamp)
}

fn render_header(
    model: &AppModel,
    theme: Theme,
    motion: MotionFrame,
    area: Rect,
    buffer: &mut Buffer,
) {
    let colors = palette(theme);
    let motion = status_motion(model, motion);
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
        vec![
            status_chip(
                motion.connection_icon,
                connection_style(model.connection, colors),
            ),
            Span::styled(" · ", colors.header_text),
            status_chip(
                &motion.execution_label,
                execution_style(model.execution, colors),
            ),
        ]
    } else {
        vec![
            status_chip(
                &format!(
                    "{} {}",
                    motion.connection_icon,
                    connection_name(model.connection)
                ),
                connection_style(model.connection, colors),
            ),
            Span::styled(" · ", colors.header_text),
            status_chip(
                &motion.execution_label,
                execution_style(model.execution, colors),
            ),
            Span::styled(" ", colors.header_text),
        ]
    };
    Line::from(status)
        .alignment(Alignment::Right)
        .style(colors.header_text)
        .render(row[1], buffer);
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
    let regions = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(inner);
    let mut lines = Vec::new();
    if model.sessions.is_empty() {
        lines.push(Line::styled("No sessions yet", colors.muted));
        lines.push(Line::default());
        lines.push(Line::styled("Ctrl+N  Create one", colors.accent));
    } else {
        let focus_id = if model.focus == crate::application::FocusTarget::Navigation {
            model.navigation_selection.as_deref()
        } else {
            None
        };
        let (start, end) = rail_window(model, regions[0].height);
        for (offset, session) in model.sessions[start..end].iter().enumerate() {
            let active = model.selected_session.as_deref() == Some(&session.session_id);
            let focused = focus_id == Some(session.session_id.as_str());
            lines.extend(rail_lines(session, active, focused, colors));
            if start + offset + 1 < end {
                lines.push(Line::default());
            }
        }
    }
    Paragraph::new(lines).render(regions[0], buffer);
    if model.focus == crate::application::FocusTarget::Navigation {
        if let Some(focused) = model.navigation_selection.as_deref().and_then(|id| {
            model
                .sessions
                .iter()
                .position(|session| session.session_id == id)
        }) {
            let (start, end) = rail_window(model, regions[0].height);
            if focused >= start && focused < end {
                let y = regions[0].y + u16::try_from((focused - start) * 3).unwrap_or(u16::MAX);
                buffer.set_style(
                    Rect::new(regions[0].x, y, regions[0].width, 2.min(regions[0].height)),
                    colors.selection_row,
                );
            }
        }
    }
    Line::styled(" Ctrl+N new · Ctrl+S list", colors.muted).render(regions[1], buffer);
}

pub(crate) fn navigation_hit_test(model: &AppModel, column: u16, row: u16) -> Option<usize> {
    session::rail_hit_test(model, column, row)
}

pub(crate) fn overlay_hit_test(model: &AppModel, column: u16, row: u16) -> Option<usize> {
    let overlay = model.overlay?;
    overlay::geometry::selection_at(
        model,
        overlay,
        Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height),
        column,
        row,
    )
}

pub(crate) fn overlay_contains(model: &AppModel, column: u16, row: u16) -> bool {
    let Some(overlay) = model.overlay else {
        return false;
    };
    overlay::geometry::contains(
        model,
        overlay,
        Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height),
        column,
        row,
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
pub(super) fn turn_label(count: u64) -> &'static str {
    if count == 1 {
        "turn"
    } else {
        "turns"
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

mod markdown;

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn markdown_preview(source: &str, theme: Theme) -> Vec<Line<'static>> {
    markdown_preview_at_width(source, theme, 80)
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn markdown_preview_at_width(
    source: &str,
    theme: Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let colors = palette(theme);
    markdown::render_markdown(
        source,
        "",
        colors.normal,
        colors.agent,
        colors.muted,
        markdown_syntax::SyntaxPalette::from_palette(colors),
        width,
    )
}
