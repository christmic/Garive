//! Visual-cell navigation for the conversation surface.

use crate::{application::AppModel, Theme};

use super::{
    anchor_cell, cell_key, first_cell, last_cell, next_cell, previous_cell, rendered_cell_height,
    CellCursor, RenderCache,
};

pub(crate) fn scroll_by_visual_cells(
    model: &mut AppModel,
    theme: Theme,
    width: u16,
    viewport_height: u16,
    delta: isize,
    cache: &mut RenderCache,
) {
    if model.turn_blocks.is_empty() || delta == 0 {
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
    model.viewport.anchor_key = Some(cell_key(model, start).to_owned());
    model.viewport.source_line = source_line;
}

pub(crate) fn reflow_visual_anchor(
    model: &mut AppModel,
    theme: Theme,
    width: u16,
    cache: &mut RenderCache,
) {
    if model.viewport.follow_latest || model.turn_blocks.is_empty() {
        return;
    }
    let start = anchor_cell(model)
        .or_else(|| first_cell(model))
        .expect("non-empty blocks");
    let height = rendered_cell_height(model, start, width, theme, cache);
    model.viewport.anchor_key = Some(cell_key(model, start).to_owned());
    model.viewport.source_line = model.viewport.source_line.min(height.saturating_sub(1));
}

fn current_visual_anchor(
    model: &AppModel,
    theme: Theme,
    width: u16,
    viewport_height: u16,
    cache: &mut RenderCache,
) -> (CellCursor, usize) {
    if !model.viewport.follow_latest {
        let start = anchor_cell(model)
            .or_else(|| first_cell(model))
            .expect("non-empty blocks");
        let height = rendered_cell_height(model, start, width, theme, cache);
        return (
            start,
            model.viewport.source_line.min(height.saturating_sub(1)),
        );
    }

    let mut cursor = last_cell(model).expect("non-empty blocks");
    let mut measured = 0usize;
    let target = usize::from(viewport_height.max(1));
    loop {
        let height = rendered_cell_height(model, cursor, width, theme, cache);
        if measured.saturating_add(height) >= target || previous_cell(model, cursor).is_none() {
            let visible_from_cell = target.saturating_sub(measured).min(height);
            return (cursor, height.saturating_sub(visible_from_cell));
        }
        measured = measured.saturating_add(height);
        cursor = previous_cell(model, cursor).expect("checked above");
    }
}

fn move_anchor_up(
    model: &AppModel,
    theme: Theme,
    width: u16,
    mut start: CellCursor,
    mut source_line: usize,
    mut cells: usize,
    cache: &mut RenderCache,
) -> (CellCursor, usize, bool) {
    while cells > source_line {
        cells -= source_line;
        let Some(previous) = previous_cell(model, start) else {
            return (start, 0, false);
        };
        start = previous;
        source_line = rendered_cell_height(model, start, width, theme, cache);
    }
    (start, source_line - cells, false)
}

fn move_anchor_down(
    model: &AppModel,
    theme: Theme,
    width: u16,
    mut start: CellCursor,
    mut source_line: usize,
    mut cells: usize,
    cache: &mut RenderCache,
) -> (CellCursor, usize, bool) {
    loop {
        let height = rendered_cell_height(model, start, width, theme, cache);
        let remaining_in_cell = height.saturating_sub(source_line.saturating_add(1));
        if cells <= remaining_in_cell {
            return (start, source_line + cells, false);
        }
        cells -= remaining_in_cell.saturating_add(1);
        let Some(next) = next_cell(model, start) else {
            return (start, height.saturating_sub(1), true);
        };
        start = next;
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

    fn model_with(items: Vec<TimelineItem>) -> AppModel {
        let mut model = AppModel::default();
        for item in items {
            model.push_test_timeline_item(item);
        }
        model
    }

    #[test]
    fn wrapped_markdown_and_cjk_scroll_inside_an_item_then_cross_items() {
        let mut model = model_with(vec![
            visual_item(
                "wide",
                TimelineRole::Agent,
                "**alpha** 中文中文中文中文 alpha alpha alpha",
            ),
            visual_item("next", TimelineRole::User, "next"),
        ]);
        let mut cache = RenderCache::default();
        model.viewport.follow_latest = false;
        model.viewport.anchor_key = Some("wide".into());

        scroll_by_visual_cells(&mut model, Theme::Dark, 12, 3, 1, &mut cache);
        assert_eq!(model.viewport.anchor_key.as_deref(), Some("wide"));
        assert_eq!(model.viewport.source_line, 1);

        let first_height = rendered_cell_height(
            &model,
            anchor_cell(&model).expect("answer anchor"),
            12,
            Theme::Dark,
            &mut cache,
        );
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
        let mut model = model_with(vec![visual_item(
            "answer",
            TimelineRole::Agent,
            "one two three four five six seven eight nine ten eleven twelve",
        )]);
        let mut cache = RenderCache::default();
        model.viewport.follow_latest = false;
        model.viewport.anchor_key = Some("answer".into());

        scroll_by_visual_cells(&mut model, Theme::Dark, 10, 3, 3, &mut cache);

        assert_eq!(model.viewport.anchor_key.as_deref(), Some("answer"));
        assert_eq!(model.viewport.source_line, 3);
    }

    #[test]
    fn page_up_from_follow_moves_one_viewport_from_the_rendered_tail() {
        let mut model = model_with(vec![visual_item(
            "answer",
            TimelineRole::Agent,
            "one two three four five six seven eight nine ten eleven twelve thirteen fourteen",
        )]);
        let mut cache = RenderCache::default();
        let (_, tail_top) = current_visual_anchor(&model, Theme::Dark, 10, 3, &mut cache);
        assert!(tail_top >= 3);

        scroll_by_visual_cells(&mut model, Theme::Dark, 10, 3, -3, &mut cache);

        assert!(!model.viewport.follow_latest);
        assert_eq!(model.viewport.anchor_key.as_deref(), Some("answer"));
        assert_eq!(model.viewport.source_line, tail_top - 3);
    }

    #[test]
    fn reflow_keeps_the_top_item_and_clamps_its_visual_line() {
        let mut model = model_with(vec![visual_item(
            "answer",
            TimelineRole::Agent,
            "one two three four five six seven eight nine ten",
        )]);
        model.viewport.follow_latest = false;
        model.viewport.anchor_key = Some("answer".into());
        model.viewport.source_line = 5;

        reflow_visual_anchor(&mut model, Theme::Dark, 80, &mut RenderCache::default());

        assert_eq!(model.viewport.anchor_key.as_deref(), Some("answer"));
        assert_eq!(model.viewport.source_line, 1);
    }

    #[test]
    fn detached_updates_do_not_move_the_visual_anchor() {
        let mut model = model_with(vec![visual_item(
            "answer",
            TimelineRole::Agent,
            "durable answer",
        )]);
        model.viewport.follow_latest = false;
        model.viewport.anchor_key = Some("answer".into());
        model.viewport.source_line = 1;
        model.viewport.newer_updates = 4;
        let before = model.viewport.clone();
        model.push_test_timeline_item(visual_item(
            "newer",
            TimelineRole::Agent,
            "new durable output",
        ));

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
