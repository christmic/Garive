use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Text},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget, Wrap},
};

use crate::{application::AppModel, Theme};

use super::{palette, primitives::truncate_display, safe_text};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RailMetric {
    area: Rect,
    total: usize,
    visible: usize,
    start: usize,
    thumb_start: u16,
    thumb_len: u16,
}

pub(super) fn render(model: &AppModel, theme: Theme, conversation: Rect, buffer: &mut Buffer) {
    let Some(metric) = metric(model, conversation) else {
        return;
    };
    let colors = palette(theme);
    let track = colors.muted;
    let thumb = if model.viewport.newer_updates > 0 {
        colors.warning
    } else if model.viewport.follow_latest {
        colors.muted.add_modifier(ratatui::style::Modifier::BOLD)
    } else {
        colors.accent
    };
    let (track_glyph, thumb_glyph) = match theme {
        Theme::Mono => ("·", "█"),
        _ => ("┊", "┃"),
    };
    for offset in 0..metric.area.height {
        let in_thumb = offset >= metric.thumb_start
            && offset < metric.thumb_start.saturating_add(metric.thumb_len);
        let cell = &mut buffer[(metric.area.x, metric.area.y + offset)];
        let hovered = model
            .conversation_rail_hover
            .is_some_and(|hover| hover.row == metric.area.y + offset);
        cell.set_symbol(if hovered {
            if theme == Theme::Mono {
                "▓"
            } else {
                "╋"
            }
        } else if in_thumb {
            thumb_glyph
        } else {
            track_glyph
        });
        cell.set_style(if hovered {
            colors.accent
        } else if in_thumb {
            thumb
        } else {
            track
        });
    }
    render_preview(model, theme, conversation, metric, buffer);
}

fn render_preview(
    model: &AppModel,
    theme: Theme,
    conversation: Rect,
    metric: RailMetric,
    buffer: &mut Buffer,
) {
    let Some(hover) = model.conversation_rail_hover else {
        return;
    };
    let Some(item) = model.timeline.get(hover.index) else {
        return;
    };
    if hover.row < metric.area.y || hover.row >= metric.area.bottom() || conversation.width < 24 {
        return;
    }
    let width = (conversation.width / 2)
        .clamp(20, 36)
        .min(conversation.width.saturating_sub(3));
    let height = 4_u16.min(metric.area.height);
    if width < 12 || height < 3 {
        return;
    }
    let x = metric
        .area
        .x
        .saturating_sub(width.saturating_add(1))
        .max(conversation.x);
    let max_y = metric.area.bottom().saturating_sub(height);
    let y = hover
        .row
        .saturating_sub(height / 2)
        .clamp(metric.area.y, max_y);
    let area = Rect::new(x, y, width, height);
    let colors = palette(theme);
    let role = match item.role {
        crate::application::TimelineRole::User => "You",
        crate::application::TimelineRole::Agent => "Garive",
        crate::application::TimelineRole::Status => "Status",
    };
    let title = format!(" Cell {} · {role} ", hover.index + 1);
    let excerpt = safe_text(&item.text.replace(['\r', '\n'], " "));
    let excerpt = truncate_display(&excerpt, usize::from(width.saturating_sub(4)) * 2);
    Clear.render(area, buffer);
    buffer.set_style(area, colors.header_background);
    let block = Block::default()
        .title(Line::styled(
            title,
            colors.title.patch(colors.header_background),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(colors.overlay_border.patch(colors.header_background))
        .padding(Padding::horizontal(1));
    Paragraph::new(Text::from(excerpt))
        .block(block)
        .style(colors.header_text)
        .wrap(Wrap { trim: true })
        .render(area, buffer);
}

pub(super) fn target_at(
    model: &AppModel,
    conversation: Rect,
    column: u16,
    row: u16,
) -> Option<usize> {
    let metric = metric(model, conversation)?;
    if column != metric.area.x || row < metric.area.y || row >= metric.area.bottom() {
        return None;
    }
    let offset = row - metric.area.y;
    if offset == 0 {
        return Some(0);
    }
    if offset + 1 >= metric.area.height {
        return Some(metric.total - 1);
    }
    Some(
        usize::from(offset)
            .saturating_mul(metric.total - 1)
            .div_ceil(usize::from(metric.area.height - 1))
            .min(metric.total - 1),
    )
}

fn metric(model: &AppModel, conversation: Rect) -> Option<RailMetric> {
    if model.overlay.is_some()
        || conversation.width < 20
        || conversation.height < 4
        || model.timeline.len() < 2
    {
        return None;
    }
    let area = track_area(conversation)?;
    let total = model.timeline.len();
    let capacity = usize::from(area.height / 3).max(1);
    let start = if model.viewport.follow_latest {
        total.saturating_sub(capacity)
    } else {
        model
            .viewport
            .anchor_key
            .as_deref()
            .and_then(|key| {
                model
                    .timeline
                    .iter()
                    .position(|item| item.stable_key == key)
            })
            .unwrap_or(0)
    };
    let visible = capacity.min(total.saturating_sub(start)).max(1);
    if total <= visible {
        return None;
    }
    let thumb_len = (usize::from(area.height)
        .saturating_mul(visible)
        .div_ceil(total))
    .clamp(1, usize::from(area.height)) as u16;
    let max_thumb_start = area.height.saturating_sub(thumb_len);
    let max_start = total.saturating_sub(visible);
    let thumb_start = if max_start == 0 {
        0
    } else {
        usize::from(max_thumb_start)
            .saturating_mul(start.min(max_start))
            .div_ceil(max_start) as u16
    };
    Some(RailMetric {
        area,
        total,
        visible,
        start,
        thumb_start,
        thumb_len,
    })
}

fn track_area(conversation: Rect) -> Option<Rect> {
    let block_bottom = 1;
    let top_padding = 1;
    let right_padding = 2;
    (conversation.width > right_padding && conversation.height > block_bottom + top_padding).then(
        || {
            Rect::new(
                conversation.right().saturating_sub(right_padding),
                conversation.y.saturating_add(top_padding),
                1,
                conversation
                    .height
                    .saturating_sub(block_bottom + top_padding),
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{TimelineItem, TimelineRole};

    fn model(count: usize) -> AppModel {
        let mut model = AppModel::default();
        for position in 0..count {
            model.timeline.push(TimelineItem {
                stable_key: format!("item-{position}"),
                position: position as u64 + 1,
                role: TimelineRole::Agent,
                tone: Default::default(),
                text: "bounded".into(),
            });
        }
        model
    }

    #[test]
    fn rail_is_bounded_suppressed_and_exact_at_track_edges() {
        let area = Rect::new(0, 2, 100, 18);
        assert!(metric(&model(1), area).is_none());
        let model = model(20);
        let metric = metric(&model, area).unwrap();
        assert_eq!(metric.area, Rect::new(98, 3, 1, 16));
        assert_eq!(target_at(&model, area, 98, 3), Some(0));
        assert_eq!(target_at(&model, area, 98, 18), Some(19));
        assert!(target_at(&model, area, 97, 10).is_none());
    }

    #[test]
    fn detached_thumb_and_intermediate_target_follow_stable_cell_indices() {
        let area = Rect::new(0, 2, 100, 18);
        let mut model = model(20);
        model.viewport.follow_latest = false;
        model.viewport.anchor_key = Some("item-5".into());
        let metric = metric(&model, area).unwrap();
        assert_eq!(metric.start, 5);
        assert!(metric.thumb_start > 0);
        let middle = target_at(&model, area, 98, 11).unwrap();
        assert!((9..=12).contains(&middle));
    }
}
