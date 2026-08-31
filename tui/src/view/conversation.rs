use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Paragraph, Widget, Wrap},
};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;

use crate::{
    application::{AppModel, TimelineRole},
    Theme,
};

use super::{
    empty_detail, empty_title, markdown::render_markdown, palette, safe_text, MotionFrame,
};

pub(super) mod live_cache;
mod scroll;
pub(crate) use scroll::{reflow_visual_anchor, scroll_by_visual_cells};

pub(super) fn render_conversation(
    model: &AppModel,
    theme: Theme,
    motion: MotionFrame,
    area: Rect,
    buffer: &mut Buffer,
    cache: &mut RenderCache,
) {
    let colors = palette(theme);
    let context = context_copy(model);
    let inner = viewport_rect(model, area);
    if let Some(context) = context {
        Line::styled(context, colors.muted)
            .alignment(ratatui::layout::Alignment::Center)
            .render(Rect::new(area.x, area.y, area.width, 1), buffer);
    }
    let window = (!model.timeline.is_empty() || model.live_answer.current().is_some())
        .then(|| conversation_window(model, theme, motion, inner.width, inner.height, cache));
    let mut lines = Vec::new();
    let mut scroll = 0;
    if model.timeline.is_empty() && model.live_answer.current().is_none() {
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

fn context_copy(model: &AppModel) -> Option<String> {
    if model.viewport.newer_updates > 0 {
        Some(format!(
            "↓ {} newer updates · End to follow",
            model.viewport.newer_updates
        ))
    } else if !model.viewport.follow_latest {
        Some("↑ Browsing history · End to follow".into())
    } else {
        None
    }
}

pub(crate) fn viewport_rect(model: &AppModel, area: Rect) -> Rect {
    let context_height = u16::from(context_copy(model).is_some());
    Rect::new(
        area.x.saturating_add(2),
        area.y.saturating_add(1).saturating_add(context_height),
        area.width.saturating_sub(4),
        area.height.saturating_sub(1).saturating_sub(context_height),
    )
}

struct ConversationWindow {
    lines: Vec<Line<'static>>,
    scroll: usize,
    #[cfg_attr(not(test), allow(dead_code))]
    laid_out: usize,
}

fn conversation_window(
    model: &AppModel,
    theme: Theme,
    motion: MotionFrame,
    width: u16,
    height: u16,
    cache: &mut RenderCache,
) -> ConversationWindow {
    cache.live.retain_for(model.live_answer.current());
    let target_height = usize::from(height).saturating_add(4);
    let mut cells = VecDeque::new();
    let mut laid_out = 0;
    let mut measured_height: usize = 0;
    if model.viewport.follow_latest {
        if let Some(answer) = model.live_answer.current() {
            let cell = super::live_answer::render(
                answer,
                theme,
                width,
                motion.is_reduced(),
                &mut cache.live,
            );
            measured_height = measured_height.saturating_add(wrapped_height(&cell, width));
            cells.push_front(cell);
        }
        let mut end = model.timeline.len();
        while end > 0 {
            let start = cell_start(&model.timeline, end);
            let mut cell = render_cell(&model.timeline[start..end], width, theme, cache);
            append_turn_gap(
                &mut cell,
                model.timeline[end - 1].role,
                model.timeline.get(end).map(|item| item.role),
                end == model.timeline.len() && model.live_answer.current().is_some(),
            );
            measured_height = measured_height.saturating_add(wrapped_height(&cell, width));
            cells.push_front(cell);
            laid_out += end - start;
            if measured_height >= target_height {
                break;
            }
            end = start;
        }
    } else {
        let requested = model
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
        let start = containing_cell_start(&model.timeline, requested);
        let first_end = cell_end(&model.timeline, start);
        let first_height = rendered_cell_height(model, start, first_end, width, theme, cache);
        let source_line = model
            .viewport
            .source_line
            .min(first_height.saturating_sub(1));
        let mut index = start;
        while index < model.timeline.len() {
            let end = cell_end(&model.timeline, index);
            let mut cell = render_cell(&model.timeline[index..end], width, theme, cache);
            append_turn_gap(
                &mut cell,
                model.timeline[end - 1].role,
                model.timeline.get(end).map(|item| item.role),
                end == model.timeline.len() && model.live_answer.current().is_some(),
            );
            measured_height = measured_height.saturating_add(wrapped_height(&cell, width));
            cells.push_back(cell);
            laid_out += end - index;
            if measured_height >= target_height.saturating_add(source_line) {
                break;
            }
            index = end;
        }
    }
    let lines = cells.into_iter().flatten().collect::<Vec<_>>();
    let scroll = if model.viewport.follow_latest {
        wrapped_height(&lines, width).saturating_sub(usize::from(height))
    } else {
        let requested = model
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
        let start = containing_cell_start(&model.timeline, requested);
        let end = cell_end(&model.timeline, start);
        model
            .viewport
            .source_line
            .min(rendered_cell_height(model, start, end, width, theme, cache).saturating_sub(1))
    };
    ConversationWindow {
        lines,
        scroll,
        laid_out,
    }
}

fn containing_cell_start(items: &[crate::application::TimelineItem], index: usize) -> usize {
    if items[index].role != TimelineRole::Status {
        return index;
    }
    let mut start = index;
    while start > 0 && items[start - 1].role == TimelineRole::Status {
        start -= 1;
    }
    start
}

fn rendered_cell_height(
    model: &AppModel,
    start: usize,
    end: usize,
    width: u16,
    theme: Theme,
    cache: &mut RenderCache,
) -> usize {
    let mut cell = render_cell(&model.timeline[start..end], width, theme, cache);
    append_turn_gap(
        &mut cell,
        model.timeline[end - 1].role,
        model.timeline.get(end).map(|item| item.role),
        end == model.timeline.len() && model.live_answer.current().is_some(),
    );
    wrapped_height(&cell, width).max(1)
}

fn cell_start(items: &[crate::application::TimelineItem], end: usize) -> usize {
    if items[end - 1].role != TimelineRole::Status {
        return end - 1;
    }
    let mut start = end - 1;
    while start > 0 && items[start - 1].role == TimelineRole::Status {
        start -= 1;
    }
    start
}

fn cell_end(items: &[crate::application::TimelineItem], start: usize) -> usize {
    if items[start].role != TimelineRole::Status {
        return start + 1;
    }
    let mut end = start + 1;
    while end < items.len() && items[end].role == TimelineRole::Status {
        end += 1;
    }
    end
}

fn render_cell(
    items: &[crate::application::TimelineItem],
    width: u16,
    theme: Theme,
    cache: &mut RenderCache,
) -> Vec<Line<'static>> {
    if items[0].role == TimelineRole::Status {
        super::activity_stack::render(items, theme, width)
    } else {
        cache.render(&items[0], width, theme)
    }
}

fn append_turn_gap(
    cell: &mut Vec<Line<'static>>,
    role: TimelineRole,
    next: Option<TimelineRole>,
    live_answer_follows: bool,
) {
    let ends_turn = role == TimelineRole::Agent
        || next == Some(TimelineRole::User)
        || (next.is_none() && !live_answer_follows);
    if ends_turn {
        cell.push(Line::default());
    }
}

const MAX_CACHED_CELLS: usize = 512;
const MAX_CACHE_BYTES: usize = 4 * 1_024 * 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
struct RenderCacheKey {
    stable_key: String,
    width: u16,
    theme: u8,
    content_digest: [u8; 32],
}

struct CachedCell {
    key: RenderCacheKey,
    lines: Vec<Line<'static>>,
    bytes: usize,
}

#[derive(Default)]
pub(crate) struct RenderCache {
    cells: VecDeque<CachedCell>,
    bytes: usize,
    live: live_cache::LiveRenderCache,
    #[cfg(test)]
    hits: usize,
}

impl RenderCache {
    fn render(
        &mut self,
        item: &crate::application::TimelineItem,
        width: u16,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let key = RenderCacheKey {
            stable_key: item.stable_key.clone(),
            width,
            theme: theme_key(theme),
            content_digest: content_digest(item),
        };
        if let Some(index) = self.cells.iter().position(|cell| cell.key == key) {
            let cell = self
                .cells
                .remove(index)
                .expect("cache index came from lookup");
            let lines = cell.lines.clone();
            self.cells.push_back(cell);
            #[cfg(test)]
            {
                self.hits += 1;
            }
            return lines;
        }
        let lines = render_timeline_item(item, theme, width);
        let bytes = item.stable_key.len()
            + lines
                .iter()
                .flat_map(|line| &line.spans)
                .map(|span| span.content.len())
                .sum::<usize>();
        if bytes <= MAX_CACHE_BYTES {
            while self.cells.len() >= MAX_CACHED_CELLS
                || self.bytes.saturating_add(bytes) > MAX_CACHE_BYTES
            {
                let Some(evicted) = self.cells.pop_front() else {
                    break;
                };
                self.bytes = self.bytes.saturating_sub(evicted.bytes);
            }
            self.bytes = self.bytes.saturating_add(bytes);
            self.cells.push_back(CachedCell {
                key,
                lines: lines.clone(),
                bytes,
            });
        }
        lines
    }
}

fn content_digest(item: &crate::application::TimelineItem) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(item.position.to_be_bytes());
    digest.update([match item.role {
        TimelineRole::User => 0,
        TimelineRole::Agent => 1,
        TimelineRole::Status => 2,
    }]);
    digest.update(item.text.as_bytes());
    digest.finalize().into()
}

fn theme_key(theme: Theme) -> u8 {
    match theme {
        Theme::System => 0,
        Theme::Dark => 1,
        Theme::Light => 2,
        Theme::Mono => 3,
    }
}

fn render_timeline_item(
    item: &crate::application::TimelineItem,
    theme: Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let colors = palette(theme);
    let mut lines = Vec::new();
    match item.role {
        TimelineRole::User => {
            lines.push(Line::styled("You", colors.user));
            push_content(&mut lines, &item.text, "  ", colors.normal);
        }
        TimelineRole::Agent => {
            lines.push(Line::styled("◆ Garive", colors.agent));
            lines.extend(render_markdown(
                &item.text,
                "  ",
                colors.normal,
                colors.agent,
                colors.muted,
                super::markdown_syntax::SyntaxPalette::from_palette(colors),
                width,
            ));
        }
        TimelineRole::Status => {
            let text = safe_text(&item.text);
            let (icon, style) = match item.tone {
                crate::application::TimelineTone::Success => ("✓", colors.success),
                crate::application::TimelineTone::Warning => ("!", colors.warning),
                crate::application::TimelineTone::Danger => ("×", colors.danger),
                crate::application::TimelineTone::Active => ("●", colors.accent),
                crate::application::TimelineTone::Neutral => ("◌", colors.activity),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {icon}  "), style),
                Span::styled(text, colors.muted),
            ]));
        }
    }
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
                tone: Default::default(),
                text: "A short bounded response.".into(),
            });
        }

        let window = conversation_window(
            &model,
            Theme::Dark,
            MotionFrame::reduced(),
            90,
            30,
            &mut RenderCache::default(),
        );

        assert!(window.laid_out < 30);
    }

    #[test]
    fn rendered_cell_cache_keys_width_theme_and_content() {
        let item = TimelineItem {
            stable_key: "answer".into(),
            position: 1,
            role: TimelineRole::Agent,
            tone: Default::default(),
            text: "**cached** answer".into(),
        };
        let mut cache = RenderCache::default();
        let first = cache.render(&item, 80, Theme::Dark);
        assert_eq!(cache.hits, 0);
        assert_eq!(cache.render(&item, 80, Theme::Dark), first);
        assert_eq!(cache.hits, 1);
        let _ = cache.render(&item, 100, Theme::Dark);
        let _ = cache.render(&item, 80, Theme::Light);
        let mut changed = item;
        changed.text.push('!');
        let _ = cache.render(&changed, 80, Theme::Dark);
        assert_eq!(cache.hits, 1);

        for position in 2..=520 {
            let item = TimelineItem {
                stable_key: format!("item-{position}"),
                position,
                role: TimelineRole::Status,
                tone: Default::default(),
                text: "bounded".into(),
            };
            let _ = cache.render(&item, 80, Theme::Dark);
        }
        assert_eq!(cache.cells.len(), MAX_CACHED_CELLS);
        assert!(cache.bytes <= MAX_CACHE_BYTES);
    }
}
