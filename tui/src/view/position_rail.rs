use ratatui::{buffer::Buffer, layout::Rect};

use crate::{application::AppModel, Theme};

use super::palette;

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
        cell.set_symbol(if in_thumb { thumb_glyph } else { track_glyph });
        cell.set_style(if in_thumb { thumb } else { track });
    }
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
