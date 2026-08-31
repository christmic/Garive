//! Shared rendering, windowing, and hit geometry for Inspector presentations.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Widget},
};

use crate::{
    application::{AppModel, FocusTarget, InspectorActivation, InspectorEntry, InspectorTone},
    Theme,
};

use super::{palette, primitives::truncate_display, safe_text, style::Palette};

pub(super) const WIDE_WIDTH: u16 = 32;
const ENTRY_ROWS: u16 = 2;

#[derive(Clone, Debug)]
struct Geometry {
    inner: Rect,
    start: usize,
    end: usize,
}

pub(super) fn wide_area(area: Rect) -> Option<Rect> {
    if area.width < 120 {
        return None;
    }
    let combined_width = area.width.min(129);
    let x = area.x + area.width.saturating_sub(combined_width) / 2;
    Some(Rect::new(
        x + combined_width - WIDE_WIDTH,
        area.y,
        WIDE_WIDTH,
        area.height,
    ))
}

pub(super) fn render(
    model: &AppModel,
    theme: Theme,
    area: Rect,
    buffer: &mut Buffer,
    overlay: bool,
) {
    if area.is_empty() {
        return;
    }
    let colors = palette(theme);
    let title = format!(" Inspector · {} ", model.inspector.variant.label());
    let block = Block::default()
        .title(Line::styled(title, colors.title))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if overlay || model.focus == FocusTarget::Inspector {
            colors.overlay_border
        } else {
            colors.border
        })
        .padding(Padding::new(1, 1, 0, 0));
    let geometry = geometry(model, area);
    block.render(area, buffer);
    let projection = model.inspector_projection();
    if projection.entries.is_empty() {
        Line::styled(empty_copy(model), colors.muted).render(geometry.inner, buffer);
    } else {
        for (offset, entry) in projection.entries[geometry.start..geometry.end]
            .iter()
            .enumerate()
        {
            let index = geometry.start + offset;
            let y = geometry.inner.y + u16::try_from(offset).unwrap_or(u16::MAX) * ENTRY_ROWS;
            render_entry(
                entry,
                index == model.inspector_selection(),
                colors,
                Rect::new(geometry.inner.x, y, geometry.inner.width, ENTRY_ROWS),
                buffer,
            );
        }
    }
    if geometry.inner.height > 0 {
        let footer = Rect::new(
            geometry.inner.x,
            geometry.inner.bottom() - 1,
            geometry.inner.width,
            1,
        );
        Line::styled(footer_copy(model, footer.width), colors.muted).render(footer, buffer);
    }
}

pub(super) fn selection_at(model: &AppModel, area: Rect, column: u16, row: u16) -> Option<usize> {
    let geometry = geometry(model, area);
    if column < geometry.inner.x
        || column >= geometry.inner.right()
        || row < geometry.inner.y
        || row >= geometry.inner.bottom().saturating_sub(1)
    {
        return None;
    }
    let index = geometry.start + usize::from((row - geometry.inner.y) / ENTRY_ROWS);
    (index < geometry.end).then_some(index)
}

pub(super) fn linear_text(model: &AppModel) -> String {
    let projection = model.inspector_projection();
    let selected = model.inspector_selection();
    let (start, end) = projection.window(selected, 10);
    let rows = projection.entries[start..end]
        .iter()
        .enumerate()
        .map(|(offset, entry)| {
            let marker = if start + offset == selected { ">" } else { " " };
            format!(
                "{marker} {}. {}. {}",
                start + offset + 1,
                safe_text(&entry.label),
                safe_text(&entry.detail)
            )
        })
        .collect::<Vec<_>>();
    let body = if rows.is_empty() {
        empty_copy(model).into()
    } else {
        rows.join("\n")
    };
    let guidance = selected_action(model).map_or(
        "Use arrows to select, or Escape to close.".into(),
        |label| format!("Use arrows to select, Enter to {label}, or Escape to close."),
    );
    format!(
        "Inspector, {}.\n{body}\n{guidance}",
        projection.variant.label()
    )
}

fn geometry(model: &AppModel, area: Rect) -> Geometry {
    let inner = Block::default()
        .borders(Borders::ALL)
        .padding(Padding::new(1, 1, 0, 0))
        .inner(area);
    let projection = model.inspector_projection();
    let capacity = usize::from(inner.height.saturating_sub(1) / ENTRY_ROWS);
    let (start, end) = projection.window(model.inspector_selection(), capacity);
    Geometry { inner, start, end }
}

fn render_entry(
    entry: &InspectorEntry,
    selected: bool,
    colors: Palette,
    area: Rect,
    buffer: &mut Buffer,
) {
    if area.is_empty() {
        return;
    }
    if selected {
        buffer.set_style(area, colors.selection_row);
    }
    let marker = if selected { "›" } else { " " };
    let icon = tone_icon(entry.tone);
    let label_width = usize::from(area.width.saturating_sub(4));
    Line::from(vec![
        Span::styled(format!("{marker} "), colors.selected),
        Span::styled(format!("{icon} "), tone_style(entry.tone, colors)),
        Span::styled(
            truncate_display(&safe_text(&entry.label), label_width),
            colors.normal,
        ),
    ])
    .render(Rect::new(area.x, area.y, area.width, 1), buffer);
    if area.height > 1 && !detail_repeats_label(&entry.label, &entry.detail) {
        Line::styled(
            format!(
                "    {}",
                truncate_display(
                    &safe_text(&entry.detail),
                    usize::from(area.width.saturating_sub(4))
                )
            ),
            colors.muted,
        )
        .render(Rect::new(area.x, area.y + 1, area.width, 1), buffer);
    }
}

fn footer_copy(model: &AppModel, width: u16) -> String {
    let close = "Esc close";
    let action = selected_action(model).map(|label| format!("Enter {label} · {close}"));
    let full = action.as_ref().map_or_else(
        || format!("↑↓ select · {close}"),
        |action| format!("↑↓ select · {action}"),
    );
    let width = usize::from(width);
    if unicode_width::UnicodeWidthStr::width(full.as_str()) <= width {
        full
    } else if let Some(action) =
        action.filter(|value| unicode_width::UnicodeWidthStr::width(value.as_str()) <= width)
    {
        action
    } else {
        truncate_display(close, width)
    }
}

fn detail_repeats_label(label: &str, detail: &str) -> bool {
    let detail = detail.trim();
    !detail.is_empty()
        && label
            .trim()
            .to_lowercase()
            .ends_with(&format!("· {}", detail.to_lowercase()))
}

fn empty_copy(model: &AppModel) -> &'static str {
    match model.inspector.variant {
        crate::application::InspectorVariant::Activity => "No public activity yet.",
        crate::application::InspectorVariant::Recovery => "No recovery action is needed.",
        crate::application::InspectorVariant::Details => "No safe details are available.",
    }
}

fn selected_action(model: &AppModel) -> Option<&'static str> {
    let projection = model.inspector_projection();
    match &projection
        .entries
        .get(model.inspector_selection())?
        .activation
    {
        InspectorActivation::None => None,
        InspectorActivation::Turn { .. } => Some("jump"),
        InspectorActivation::RetryPending => Some("retry"),
        InspectorActivation::Reconnect => Some("reconnect"),
        InspectorActivation::Suspension => Some("review"),
    }
}

fn tone_icon(tone: InspectorTone) -> &'static str {
    match tone {
        InspectorTone::Neutral => "○",
        InspectorTone::Active => "●",
        InspectorTone::Success => "✓",
        InspectorTone::Warning => "!",
        InspectorTone::Danger => "×",
    }
}

fn tone_style(tone: InspectorTone, colors: Palette) -> ratatui::style::Style {
    match tone {
        InspectorTone::Neutral => colors.muted,
        InspectorTone::Active => colors.accent,
        InspectorTone::Success => colors.success,
        InspectorTone::Warning => colors.warning,
        InspectorTone::Danger => colors.danger,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_footer_keeps_close_action_visible() {
        let model = AppModel::default();
        let copy = footer_copy(&model, 28);
        assert!(copy.ends_with("Esc close"));
        assert!(unicode_width::UnicodeWidthStr::width(copy.as_str()) <= 28);
    }

    #[test]
    fn completed_detail_is_not_repeated_under_completed_label() {
        assert!(detail_repeats_label(
            "Agent action · completed",
            "Completed"
        ));
        assert!(!detail_repeats_label(
            "Running tests",
            "cargo test --workspace"
        ));
    }
}
