//! Collision-safe left action and right context layout for the Footer row.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::application::AppModel;

use super::{context_line, style::Palette};

pub(super) fn render_footer_layout(
    model: &AppModel,
    left: Option<Line<'static>>,
    colors: Palette,
    area: Rect,
    buffer: &mut Buffer,
) {
    let left_width = left.as_ref().map_or(0, |line| line.width() as u16);
    if let Some(left) = left {
        Paragraph::new(left).render(area, buffer);
    }
    if context_line::visible(model) || area.width < 52 {
        return;
    }
    for context in [
        ambient_context_label(model, area.width),
        ambient_context_label(model, 79),
    ]
    .into_iter()
    .flatten()
    {
        let copy = format!("{context}  ");
        let width = u16::try_from(UnicodeWidthStr::width(copy.as_str()))
            .unwrap_or(area.width)
            .min(area.width);
        if left_width > 0 && left_width.saturating_add(2).saturating_add(width) > area.width {
            continue;
        }
        let right = Rect::new(area.right().saturating_sub(width), area.y, width, 1);
        Paragraph::new(Line::styled(copy, colors.muted)).render(right, buffer);
        return;
    }
}

pub(super) fn ambient_context_label(model: &AppModel, width: u16) -> Option<String> {
    if width < 52 {
        return None;
    }
    let session = ambient_session_label(model)?;
    if width < 80 {
        return Some(session);
    }
    let Some(selected_turn) = model.selected_turn.as_deref() else {
        return Some(session);
    };
    let Some(selected_session) = model.selected_session.as_deref() else {
        return Some(session);
    };
    let Some(summary) = model.sessions.iter().find(|item| {
        item.session_id == selected_session && item.latest_turn_id.as_deref() == Some(selected_turn)
    }) else {
        return Some(session);
    };
    let Some(ordinal) = model.conversation_landmarks.last().map(|item| item.ordinal) else {
        return Some(session);
    };
    if summary.turn_count > 0 && ordinal > summary.turn_count as usize {
        return Some(session);
    }
    Some(format!("{session} · Turn {ordinal}"))
}

pub(super) fn ambient_session_label(model: &AppModel) -> Option<String> {
    let selected = model.selected_session.as_deref()?;
    model
        .sessions
        .iter()
        .position(|session| session.session_id == selected)
        .map(|index| format!("Session {}", index + 1))
}
