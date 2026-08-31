//! Visual-cell navigation for the conversation surface.

use crate::{application::AppModel, Theme};

use super::{cell_end, cell_start, containing_cell_start, rendered_cell_height, RenderCache};

pub(crate) fn scroll_by_visual_cells(
    model: &mut AppModel,
    theme: Theme,
    width: u16,
    viewport_height: u16,
    delta: isize,
    cache: &mut RenderCache,
) {
    if model.timeline.is_empty() || delta == 0 {
        return;
    }
    let (start, source_line) = current_visual_anchor(model, theme, width, viewport_height, cache);
    let (start, source_line, ran_past_end) = if delta.is_negative() {
        move_anchor_up(
            model,
            theme,
            width,
            start,
            source_line,
            delta.unsigned_abs(),
            cache,
        )
    } else {
        move_anchor_down(
            model,
            theme,
            width,
            start,
            source_line,
            delta.unsigned_abs(),
            cache,
        )
    };
    if ran_past_end {
        model.follow_latest();
        return;
    }
    model.viewport.follow_latest = false;
    model.viewport.anchor_key = Some(model.timeline[start].stable_key.clone());
    model.viewport.source_line = source_line;
}

pub(crate) fn reflow_visual_anchor(
    model: &mut AppModel,
    theme: Theme,
    width: u16,
    cache: &mut RenderCache,
) {
    if model.viewport.follow_latest || model.timeline.is_empty() {
        return;
    }
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
    let height = rendered_cell_height(model, start, end, width, theme, cache);
    model.viewport.anchor_key = Some(model.timeline[start].stable_key.clone());
    model.viewport.source_line = model.viewport.source_line.min(height.saturating_sub(1));
}

fn current_visual_anchor(
    model: &AppModel,
    theme: Theme,
    width: u16,
    viewport_height: u16,
    cache: &mut RenderCache,
) -> (usize, usize) {
    if !model.viewport.follow_latest {
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
        let height = rendered_cell_height(model, start, end, width, theme, cache);
        return (
            start,
            model.viewport.source_line.min(height.saturating_sub(1)),
        );
    }

    let mut end = model.timeline.len();
    let mut measured = 0usize;
    let target = usize::from(viewport_height.max(1));
    loop {
        let start = cell_start(&model.timeline, end);
        let height = rendered_cell_height(model, start, end, width, theme, cache);
        if measured.saturating_add(height) >= target || start == 0 {
            let visible_from_cell = target.saturating_sub(measured).min(height);
            return (start, height.saturating_sub(visible_from_cell));
        }
        measured = measured.saturating_add(height);
        end = start;
    }
}

fn move_anchor_up(
    model: &AppModel,
    theme: Theme,
    width: u16,
    mut start: usize,
    mut source_line: usize,
    mut cells: usize,
    cache: &mut RenderCache,
) -> (usize, usize, bool) {
    while cells > source_line {
        cells -= source_line;
        if start == 0 {
            return (0, 0, false);
        }
        let end = start;
        start = cell_start(&model.timeline, end);
        source_line = rendered_cell_height(model, start, end, width, theme, cache);
    }
    (start, source_line - cells, false)
}

fn move_anchor_down(
    model: &AppModel,
    theme: Theme,
    width: u16,
    mut start: usize,
    mut source_line: usize,
    mut cells: usize,
    cache: &mut RenderCache,
) -> (usize, usize, bool) {
    loop {
        let end = cell_end(&model.timeline, start);
        let height = rendered_cell_height(model, start, end, width, theme, cache);
        let remaining_in_cell = height.saturating_sub(source_line.saturating_add(1));
        if cells <= remaining_in_cell {
            return (start, source_line + cells, false);
        }
        cells -= remaining_in_cell.saturating_add(1);
        if end == model.timeline.len() {
            return (start, height.saturating_sub(1), true);
        }
        start = end;
        source_line = 0;
        if cells == 0 {
            return (start, 0, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{TimelineItem, TimelineRole},
        view::MotionFrame,
    };

    fn visual_item(key: &str, role: TimelineRole, text: &str) -> TimelineItem {
        TimelineItem {
            stable_key: key.into(),
            position: 1,
            role,
            tone: Default::default(),
            text: text.into(),
        }
    }

    #[test]
    fn wrapped_markdown_and_cjk_scroll_inside_an_item_then_cross_items() {
        let mut model = AppModel {
            timeline: vec![
                visual_item(
                    "wide",
                    TimelineRole::Agent,
                    "**alpha** 中文中文中文中文 alpha alpha alpha",
                ),
                visual_item("next", TimelineRole::User, "next"),
            ],
            ..Default::default()
        };
        let mut cache = RenderCache::default();
        model.viewport.follow_latest = false;
        model.viewport.anchor_key = Some("wide".into());

        scroll_by_visual_cells(&mut model, Theme::Dark, 12, 3, 1, &mut cache);
        assert_eq!(model.viewport.anchor_key.as_deref(), Some("wide"));
        assert_eq!(model.viewport.source_line, 1);

        let first_height = rendered_cell_height(&model, 0, 1, 12, Theme::Dark, &mut cache);
        scroll_by_visual_cells(
            &mut model,
            Theme::Dark,
            12,
            3,
            (first_height - 1) as isize,
            &mut cache,
        );
        assert_eq!(model.viewport.anchor_key.as_deref(), Some("next"));
        assert_eq!(model.viewport.source_line, 0);
    }

    #[test]
    fn page_scroll_advances_exactly_one_rendered_viewport() {
        let mut model = AppModel {
            timeline: vec![visual_item(
                "answer",
                TimelineRole::Agent,
                "one two three four five six seven eight nine ten eleven twelve",
            )],
            ..Default::default()
        };
        let mut cache = RenderCache::default();
        model.viewport.follow_latest = false;
        model.viewport.anchor_key = Some("answer".into());

        scroll_by_visual_cells(&mut model, Theme::Dark, 10, 3, 3, &mut cache);

        assert_eq!(model.viewport.anchor_key.as_deref(), Some("answer"));
        assert_eq!(model.viewport.source_line, 3);
    }

    #[test]
    fn reflow_keeps_the_top_item_and_clamps_its_visual_line() {
        let mut model = AppModel {
            timeline: vec![visual_item(
                "answer",
                TimelineRole::Agent,
                "one two three four five six seven eight nine ten",
            )],
            ..Default::default()
        };
        model.viewport.follow_latest = false;
        model.viewport.anchor_key = Some("answer".into());
        model.viewport.source_line = 5;

        reflow_visual_anchor(&mut model, Theme::Dark, 80, &mut RenderCache::default());

        assert_eq!(model.viewport.anchor_key.as_deref(), Some("answer"));
        assert_eq!(model.viewport.source_line, 2);
    }

    #[test]
    fn detached_updates_do_not_move_the_visual_anchor() {
        let mut model = AppModel {
            timeline: vec![visual_item("answer", TimelineRole::Agent, "durable answer")],
            ..Default::default()
        };
        model.viewport.follow_latest = false;
        model.viewport.anchor_key = Some("answer".into());
        model.viewport.source_line = 1;
        model.viewport.newer_updates = 4;
        let before = model.viewport.clone();

        let _ = super::super::conversation_window(
            &model,
            Theme::Dark,
            MotionFrame::reduced(),
            20,
            4,
            &mut RenderCache::default(),
        );

        assert_eq!(model.viewport, before);
    }
}
