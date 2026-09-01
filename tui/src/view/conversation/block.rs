use ratatui::text::Line;

use crate::{application::AppModel, Theme};

use super::{wrapped_height, RenderCache};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CellCursor {
    pub(super) block: usize,
    pub(super) slot: usize,
}

#[derive(Clone, Copy)]
enum BlockCell<'a> {
    Item(&'a crate::application::TimelineItem),
    Activities(&'a [crate::application::TimelineItem]),
}

impl BlockCell<'_> {
    fn key(&self) -> &str {
        match self {
            Self::Item(item) => &item.stable_key,
            Self::Activities(items) => &items[0].stable_key,
        }
    }
}

fn block_cells(block: &crate::application::TurnBlock) -> Vec<BlockCell<'_>> {
    let mut cells = vec![BlockCell::Item(&block.user)];
    if !block.activities.is_empty() {
        cells.push(BlockCell::Activities(&block.activities));
    }
    if let Some(answer) = block.committed_answer.as_ref() {
        cells.push(BlockCell::Item(answer));
    }
    if let Some(outcome) = block.outcome.as_ref() {
        cells.push(BlockCell::Item(outcome));
    }
    cells
}

pub(super) fn first_cell(model: &AppModel) -> Option<CellCursor> {
    (!model.turn_blocks.is_empty()).then_some(CellCursor { block: 0, slot: 0 })
}

pub(super) fn last_cell(model: &AppModel) -> Option<CellCursor> {
    let block = model.turn_blocks.len().checked_sub(1)?;
    Some(CellCursor {
        block,
        slot: block_cells(&model.turn_blocks[block]).len() - 1,
    })
}

pub(super) fn anchor_cell(model: &AppModel) -> Option<CellCursor> {
    let key = model.viewport.anchor_key.as_deref()?;
    model.turn_blocks.iter().enumerate().find_map(|(block, value)| {
        block_cells(value)
            .iter()
            .position(|cell| {
                cell.key() == key
                    || value.child(key).is_some_and(|child| {
                        matches!(cell, BlockCell::Activities(items) if items.iter().any(|item| item.stable_key == child.stable_key))
                    })
            })
            .map(|slot| CellCursor { block, slot })
    })
}

pub(super) fn previous_cell(model: &AppModel, cursor: CellCursor) -> Option<CellCursor> {
    if cursor.slot > 0 {
        return Some(CellCursor {
            slot: cursor.slot - 1,
            ..cursor
        });
    }
    let block = cursor.block.checked_sub(1)?;
    Some(CellCursor {
        block,
        slot: block_cells(&model.turn_blocks[block]).len() - 1,
    })
}

pub(super) fn next_cell(model: &AppModel, cursor: CellCursor) -> Option<CellCursor> {
    if cursor.slot + 1 < block_cells(&model.turn_blocks[cursor.block]).len() {
        return Some(CellCursor {
            slot: cursor.slot + 1,
            ..cursor
        });
    }
    (cursor.block + 1 < model.turn_blocks.len()).then_some(CellCursor {
        block: cursor.block + 1,
        slot: 0,
    })
}

pub(super) fn cell_key(model: &AppModel, cursor: CellCursor) -> &str {
    match block_cells(&model.turn_blocks[cursor.block])[cursor.slot] {
        BlockCell::Item(item) => &item.stable_key,
        BlockCell::Activities(items) => &items[0].stable_key,
    }
}

pub(super) fn rendered_cell_height(
    model: &AppModel,
    cursor: CellCursor,
    width: u16,
    theme: Theme,
    cache: &mut RenderCache,
) -> usize {
    let mut cell = render_block_cell(model, cursor, width, theme, cache);
    append_block_gap(
        &mut cell,
        model,
        cursor,
        model.live_answer.current().is_some(),
        width,
    );
    wrapped_height(&cell, width).max(1)
}

pub(super) fn render_block_cell(
    model: &AppModel,
    cursor: CellCursor,
    width: u16,
    theme: Theme,
    cache: &mut RenderCache,
) -> Vec<Line<'static>> {
    match block_cells(&model.turn_blocks[cursor.block])[cursor.slot] {
        BlockCell::Item(item) => cache.render(item, width, theme),
        BlockCell::Activities(items) => super::super::activity_stack::render(items, theme, width),
    }
}

pub(super) fn append_block_gap(
    cell: &mut Vec<Line<'static>>,
    model: &AppModel,
    cursor: CellCursor,
    live_answer_follows: bool,
    width: u16,
) {
    let cells = block_cells(&model.turn_blocks[cursor.block]);
    let current_is_activity = matches!(cells.get(cursor.slot), Some(BlockCell::Activities(_)));
    let durable_answer_follows = matches!(
        cells.get(cursor.slot + 1),
        Some(BlockCell::Item(item)) if item.role == crate::application::TimelineRole::Agent
    );
    let live_answer_follows_activity = cursor.slot + 1 == cells.len()
        && cursor.block + 1 == model.turn_blocks.len()
        && live_answer_follows;
    if width >= 52
        && current_is_activity
        && (durable_answer_follows || live_answer_follows_activity)
    {
        cell.push(Line::default());
        return;
    }
    let ends_turn = cursor.slot + 1 == cells.len()
        && !(cursor.block + 1 == model.turn_blocks.len() && live_answer_follows);
    if ends_turn {
        cell.push(Line::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{TimelineItem, TimelineRole, TimelineTone};

    fn item(key: &str, role: TimelineRole, text: &str) -> TimelineItem {
        TimelineItem {
            stable_key: key.into(),
            position: 1,
            role,
            tone: TimelineTone::Active,
            text: text.into(),
        }
    }

    #[test]
    fn activity_group_breathes_before_durable_or_live_answer() {
        let mut durable = AppModel::default();
        durable.push_test_timeline_item(item("user", TimelineRole::User, "request"));
        durable.push_test_timeline_item(item("activity", TimelineRole::Status, "reading"));
        durable.push_test_timeline_item(item("answer", TimelineRole::Agent, "done"));
        let mut durable_lines = vec![Line::from("activity")];
        append_block_gap(
            &mut durable_lines,
            &durable,
            CellCursor { block: 0, slot: 1 },
            false,
            80,
        );
        assert_eq!(durable_lines, vec![Line::from("activity"), Line::default()]);

        let mut live = AppModel::default();
        live.push_test_timeline_item(item("user", TimelineRole::User, "request"));
        live.push_test_timeline_item(item("activity", TimelineRole::Status, "reading"));
        let mut live_lines = vec![Line::from("activity")];
        append_block_gap(
            &mut live_lines,
            &live,
            CellCursor { block: 0, slot: 1 },
            true,
            80,
        );
        assert_eq!(live_lines, vec![Line::from("activity"), Line::default()]);

        let mut compact_lines = vec![Line::from("activity")];
        append_block_gap(
            &mut compact_lines,
            &live,
            CellCursor { block: 0, slot: 1 },
            true,
            40,
        );
        assert_eq!(compact_lines, vec![Line::from("activity")]);
    }
}
