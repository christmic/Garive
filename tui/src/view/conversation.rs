use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Padding, Paragraph, Widget, Wrap},
};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;

use crate::{
    application::{AppModel, TimelineRole},
    Theme,
};

use super::{empty_detail, empty_title, markdown::render_markdown, palette, safe_text, turn_label};

pub(super) fn render_conversation(
    model: &AppModel,
    theme: Theme,
    area: Rect,
    buffer: &mut Buffer,
    cache: &mut RenderCache,
) {
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
        .then(|| conversation_window(model, theme, inner.width, inner.height, cache));
    let title = if model.viewport.newer_updates > 0 {
        format!(
            " Conversation · {} newer updates ",
            model.viewport.newer_updates
        )
    } else if window.as_ref().is_some_and(|value| value.has_earlier) {
        " Conversation · ↑ earlier ".to_owned()
    } else if model.execution == crate::application::ExecutionState::Following {
        " Conversation · ● live ".to_owned()
    } else if let Some(turn_count) = model
        .selected_session
        .as_deref()
        .and_then(|selected| {
            model
                .sessions
                .iter()
                .find(|session| session.session_id == selected)
        })
        .map(|session| session.turn_count)
    {
        format!(" Conversation · {turn_count} {} ", turn_label(turn_count))
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
    cache: &mut RenderCache,
) -> ConversationWindow {
    let target_height = usize::from(height).saturating_add(4);
    let mut cells = VecDeque::new();
    let mut laid_out = 0;
    let mut measured_height: usize = 0;
    if model.viewport.follow_latest {
        for item in model.timeline.iter().rev() {
            let cell = cache.render(item, width, theme);
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
            let cell = cache.render(item, width, theme);
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
        let lines = render_timeline_item(item, theme);
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
        TimelineRole::Status => {
            let text = safe_text(&item.text);
            let (icon, style) = if text.contains("failed") {
                ("×", colors.danger)
            } else if text.contains("suspended") || text.contains("required") {
                ("!", colors.warning)
            } else if text.contains("completed") {
                ("✓", colors.success)
            } else {
                ("◌", colors.activity)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {icon}  "), style),
                Span::styled(text, colors.muted),
            ]));
        }
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

        let window = conversation_window(&model, Theme::Dark, 90, 30, &mut RenderCache::default());

        assert!(window.laid_out < 30);
    }

    #[test]
    fn rendered_cell_cache_keys_width_theme_and_content() {
        let item = TimelineItem {
            stable_key: "answer".into(),
            position: 1,
            role: TimelineRole::Agent,
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
                text: "bounded".into(),
            };
            let _ = cache.render(&item, 80, Theme::Dark);
        }
        assert_eq!(cache.cells.len(), MAX_CACHED_CELLS);
        assert!(cache.bytes <= MAX_CACHE_BYTES);
    }
}
