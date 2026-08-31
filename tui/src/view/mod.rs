use crate::{
    application::{AppModel, BootState, TerminalSize},
    Theme,
};
#[cfg(test)]
use ratatui::text::Line;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Paragraph, Widget},
};

mod activity_stack;
mod command_suggestions;
mod composer;
mod context_line;
mod conversation;
mod decision_sheet;
mod footer;
mod inspector;
mod layout;
mod linear;
mod live_answer;
mod markdown_syntax;
mod markdown_table;
mod minimum;
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
use layout::FrameLayout;
pub(crate) use linear::composer_status as linear_composer_status;
pub(crate) use linear::{overlay_text as linear_overlay, safe as linear_safe};
pub(crate) use motion::{status_motion_enabled, MotionFrame};
use overlay::render_overlay;
use style::palette;
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
    if area.width < 40 {
        minimum::render(model, palette(theme), area, buffer);
        return None;
    }
    let frame = FrameLayout::resolve(model, area);
    context_line::render(model, theme, motion, frame.context, buffer);
    render_conversation(model, theme, motion, frame.transcript, buffer, cache);
    if let Some(area) = frame.inspector {
        inspector::render(model, theme, area, buffer, false);
    }
    composer::render(model, palette(theme), frame.composer, buffer);
    render_footer(model, theme, frame.hint, buffer);
    command_suggestions::render(model, frame.composer, palette(theme), buffer);
    if let Some(overlay) = model.overlay {
        render_overlay(model, overlay, theme, area, buffer);
        None
    } else {
        (model.focus == crate::application::FocusTarget::Composer)
            .then(|| composer::cursor(model, frame.composer))
            .flatten()
    }
}

pub(crate) fn command_suggestion_hit_test(
    model: &AppModel,
    column: u16,
    row: u16,
) -> Option<usize> {
    let full = Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height);
    let frame = FrameLayout::resolve(model, full);
    command_suggestions::selection_at(model, frame.composer, column, row)
}

pub(crate) fn composer_hit_test(
    model: &AppModel,
    column: u16,
    row: u16,
    clamp: bool,
) -> Option<usize> {
    if model.composer_is_frozen {
        return None;
    }
    let full = Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height);
    let frame = FrameLayout::resolve(model, full);
    composer::selection_at(model, frame.composer, column, row, clamp)
}

pub(crate) fn inspector_hit_test(model: &AppModel, column: u16, row: u16) -> Option<usize> {
    let area = inspector::wide_area(Rect::new(
        0,
        0,
        model.terminal_size.width,
        model.terminal_size.height,
    ))?;
    inspector::selection_at(model, area, column, row)
}

pub(crate) fn inspector_contains(model: &AppModel, column: u16, row: u16) -> bool {
    inspector::wide_area(Rect::new(
        0,
        0,
        model.terminal_size.width,
        model.terminal_size.height,
    ))
    .is_some_and(|area| {
        column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
    })
}

pub(crate) fn composer_vertical_target(model: &AppModel, direction: i8) -> (usize, usize) {
    let full = Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height);
    let frame = FrameLayout::resolve(model, full);
    composer::vertical_target(
        &model.composer,
        frame.composer.width.saturating_sub(4),
        direction,
    )
}

pub(crate) fn composer_line_edge_target(model: &AppModel, direction: i8) -> usize {
    let full = Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height);
    let frame = FrameLayout::resolve(model, full);
    composer::line_edge_target(
        &model.composer,
        frame.composer.width.saturating_sub(4),
        direction,
    )
}

pub(crate) fn conversation_page_cells(model: &AppModel) -> usize {
    let area = Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height);
    let frame = FrameLayout::resolve(model, area);
    usize::from(
        conversation::viewport_rect(model, frame.transcript)
            .height
            .max(1),
    )
}

pub(crate) fn scroll_conversation(
    model: &mut AppModel,
    theme: Theme,
    cache: &mut RenderCache,
    cells: isize,
) {
    let area = Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height);
    let frame = FrameLayout::resolve(model, area);
    let viewport = conversation::viewport_rect(model, frame.transcript);
    conversation::scroll_by_visual_cells(
        model,
        theme,
        viewport.width,
        viewport.height,
        cells,
        cache,
    );
}

pub(crate) fn reflow_conversation(model: &mut AppModel, theme: Theme, cache: &mut RenderCache) {
    let area = Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height);
    let frame = FrameLayout::resolve(model, area);
    let viewport = conversation::viewport_rect(model, frame.transcript);
    conversation::reflow_visual_anchor(model, theme, viewport.width, cache);
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

pub(crate) fn decision_action_hit_test(
    model: &AppModel,
    column: u16,
    row: u16,
) -> Option<crate::application::ActionOverlayIntent> {
    let overlay = model.overlay?;
    overlay::geometry::decision_action_at(
        model,
        overlay,
        Rect::new(0, 0, model.terminal_size.width, model.terminal_size.height),
        column,
        row,
    )
}

pub(crate) fn decision_choice_hit_test(model: &AppModel, column: u16, row: u16) -> Option<usize> {
    let overlay = model.overlay?;
    overlay::geometry::decision_choice_at(
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

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn live_answer_preview(
    answer: &crate::application::LiveAnswer,
    theme: Theme,
    reduced_motion: bool,
) -> Vec<Line<'static>> {
    live_answer::render(
        answer,
        theme,
        80,
        reduced_motion,
        &mut conversation::live_cache::LiveRenderCache::default(),
    )
}
