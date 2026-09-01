use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Paragraph, Widget, Wrap},
};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;

use crate::{
    application::{AppModel, TimelineRole},
    Theme,
};

use super::{markdown::render_markdown, palette, safe_text, MotionFrame};

mod block;
mod empty_state;
mod follow_cue;
pub(super) mod live_cache;
mod request_surface;
mod scroll;
use block::*;
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
    let context_visible = follow_cue_visible(model);
    let inner = viewport_rect(model, area);
    if context_visible {
        follow_cue::render(model, colors, area, buffer);
    }
    let window = (!model.turn_blocks.is_empty() || model.live_answer.current().is_some())
        .then(|| conversation_window(model, theme, motion, inner.width, inner.height, cache));
    let mut lines = Vec::new();
    let mut scroll = 0;
    if model.turn_blocks.is_empty() && model.live_answer.current().is_none() {
        lines = empty_state::render(model.boot, colors);
    } else if let Some(window) = window {
        lines = window.lines;
        scroll = window.scroll;
    }
    Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll.min(u16::MAX as usize) as u16, 0))
        .render(inner, buffer);
}

fn follow_cue_visible(model: &AppModel) -> bool {
    model.viewport.newer_updates > 0 || !model.viewport.follow_latest
}

pub(crate) fn viewport_rect(model: &AppModel, area: Rect) -> Rect {
    let context_height = u16::from(follow_cue_visible(model));
    let insets = ViewportInsets::resolve(area.height.saturating_sub(context_height));
    Rect::new(
        area.x.saturating_add(insets.horizontal),
        area.y
            .saturating_add(insets.top)
            .saturating_add(context_height),
        area.width
            .saturating_sub(insets.horizontal.saturating_mul(2)),
        area.height
            .saturating_sub(insets.top)
            .saturating_sub(context_height),
    )
}

pub(crate) fn follow_cue_hit_test(model: &AppModel, area: Rect, column: u16, row: u16) -> bool {
    follow_cue::hit_test(model, area, column, row)
}

#[derive(Clone, Copy)]
struct ViewportInsets {
    horizontal: u16,
    top: u16,
}

impl ViewportInsets {
    const fn resolve(available_height: u16) -> Self {
        Self {
            horizontal: 2,
            top: if available_height > 8 { 1 } else { 0 },
        }
    }
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
        let mut cursor = last_cell(model);
        while let Some(current) = cursor {
            let mut cell = render_block_cell(model, current, width, theme, cache);
            append_block_gap(
                &mut cell,
                model,
                current,
                model.live_answer.current().is_some(),
            );
            measured_height = measured_height.saturating_add(wrapped_height(&cell, width));
            cells.push_front(cell);
            laid_out += 1;
            if measured_height >= target_height {
                break;
            }
            cursor = previous_cell(model, current);
        }
    } else {
        let start = anchor_cell(model).or_else(|| first_cell(model));
        let first_height = start
            .map(|cursor| rendered_cell_height(model, cursor, width, theme, cache))
            .unwrap_or(1);
        let source_line = model
            .viewport
            .source_line
            .min(first_height.saturating_sub(1));
        let mut cursor = start;
        while let Some(current) = cursor {
            let mut cell = render_block_cell(model, current, width, theme, cache);
            append_block_gap(
                &mut cell,
                model,
                current,
                model.live_answer.current().is_some(),
            );
            measured_height = measured_height.saturating_add(wrapped_height(&cell, width));
            cells.push_back(cell);
            laid_out += 1;
            if measured_height >= target_height.saturating_add(source_line) {
                break;
            }
            cursor = next_cell(model, current);
        }
    }
    let lines = cells.into_iter().flatten().collect::<Vec<_>>();
    let scroll = if model.viewport.follow_latest {
        wrapped_height(&lines, width).saturating_sub(usize::from(height))
    } else {
        model.viewport.source_line.min(
            anchor_cell(model)
                .or_else(|| first_cell(model))
                .map(|cursor| rendered_cell_height(model, cursor, width, theme, cache))
                .unwrap_or(1)
                .saturating_sub(1),
        )
    };
    ConversationWindow {
        lines,
        scroll,
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
            lines.extend(request_surface::render(&item.text, theme, width));
        }
        TimelineRole::Agent => {
            lines.push(Line::from(
                super::primitives::RoleMarker::Agent.span(colors),
            ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{TimelineItem, TimelineRole};

    #[test]
    fn compact_transcript_drops_decorative_top_inset_before_semantic_rows() {
        let model = AppModel::default();
        let compact = viewport_rect(&model, Rect::new(0, 0, 40, 8));
        assert_eq!(compact, Rect::new(2, 0, 36, 8));

        let standard = viewport_rect(&model, Rect::new(0, 0, 100, 9));
        assert_eq!(standard, Rect::new(2, 1, 96, 8));
    }

    #[test]
    fn user_request_is_one_compact_hanging_surface() {
        let item = TimelineItem {
            stable_key: "request".into(),
            position: 1,
            role: TimelineRole::User,
            tone: Default::default(),
            text: "Ship a polished terminal experience".into(),
        };

        let lines = render_timeline_item(&item, Theme::Dark, 22);
        let visible = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            visible,
            ["› Ship a polished     ", "  terminal experience "]
        );
        assert!(lines.iter().all(|line| line.width() == 22));
        assert!(lines.iter().all(|line| line
            .spans
            .iter()
            .all(|span| { span.style.bg == Some(ratatui::style::Color::Rgb(24, 28, 38)) })));
        let light = render_timeline_item(&item, Theme::Light, 22);
        assert!(light.iter().all(|line| line
            .spans
            .iter()
            .all(|span| { span.style.bg == Some(ratatui::style::Color::Rgb(235, 238, 244)) })));
        let mono = render_timeline_item(&item, Theme::Mono, 22);
        assert!(mono
            .iter()
            .flat_map(|line| &line.spans)
            .all(|span| span.style.bg.is_none()));
    }

    #[test]
    fn latest_window_layout_is_independent_of_history_length() {
        let mut model = AppModel::default();
        for position in 1..=10_000 {
            model.push_test_timeline_item(TimelineItem {
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
