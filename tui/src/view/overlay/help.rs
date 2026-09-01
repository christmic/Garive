use ratatui::text::Text;
use unicode_width::UnicodeWidthStr;

use crate::input::help_hints;

use super::super::{primitives::key_hints, style::Palette};

type HelpItem = (&'static str, &'static str);

pub(super) fn text(colors: Palette, content_width: u16) -> Text<'static> {
    Text::from(
        rows(content_width)
            .iter()
            .map(|row| key_hints(row, colors))
            .collect::<Vec<_>>(),
    )
}

pub(super) fn desired_height(popup_width: u16) -> u16 {
    let content_width = popup_width.saturating_sub(6).max(1);
    u16::try_from(rows(content_width).len().saturating_add(4)).unwrap_or(u16::MAX)
}

fn rows(content_width: u16) -> Vec<Vec<HelpItem>> {
    let items = help_hints()
        .map(|hint| (hint.visual_key, hint.action))
        .collect::<Vec<_>>();
    let width = usize::from(content_width);
    if items.chunks(2).all(|pair| row_width(pair) <= width) {
        return items.chunks(2).map(<[HelpItem]>::to_vec).collect();
    }

    let mut rows = Vec::<Vec<HelpItem>>::new();
    for item in items {
        if rows
            .last()
            .is_some_and(|row| row_width_with(row, item) <= width)
        {
            rows.last_mut()
                .expect("a fitting help row exists")
                .push(item);
        } else {
            rows.push(vec![item]);
        }
    }
    rows
}

fn row_width(row: &[HelpItem]) -> usize {
    1 + row
        .iter()
        .enumerate()
        .map(|(index, (key, action))| {
            usize::from(index != 0) * 2 + key.width() + action.width() + 2
        })
        .sum::<usize>()
}

fn row_width_with(row: &[HelpItem], item: HelpItem) -> usize {
    row_width(row) + usize::from(!row.is_empty()) * 2 + item.0.width() + item.1.width() + 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_help_is_a_stable_two_column_action_grid() {
        let rows = rows(56);
        assert_eq!(rows.len(), 6);
        assert!(rows.iter().all(|row| row.len() <= 2));
        assert!(rows.iter().all(|row| row_width(row) <= 56));
    }

    #[test]
    fn narrow_help_reflows_without_overflowing_a_row() {
        assert!(rows(30).iter().all(|row| row_width(row) <= 30));
    }
}
