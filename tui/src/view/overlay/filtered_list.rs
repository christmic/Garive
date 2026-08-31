use ratatui::layout::Rect;

use super::super::primitives::selection_window;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FilteredListGeometry {
    pub(super) window: (usize, usize),
    pub(super) first_item_row: u16,
    pub(super) action_row: u16,
}

impl FilteredListGeometry {
    pub(super) fn resolve(inner: Rect, count: usize, selected: usize) -> Self {
        let first_item_row = inner.y.saturating_add(1);
        let action_row = inner.bottom().saturating_sub(1);
        let capacity = usize::from(inner.height.saturating_sub(3)).max(1);
        Self {
            window: selection_window(count, selected, capacity),
            first_item_row,
            action_row,
        }
    }

    pub(super) fn selection_at(self, inner: Rect, column: u16, row: u16) -> Option<usize> {
        let (start, end) = self.window;
        if column < inner.x
            || column >= inner.right()
            || row < self.first_item_row
            || row
                >= self
                    .first_item_row
                    .saturating_add(u16::try_from(end - start).ok()?)
        {
            return None;
        }
        Some(start + usize::from(row - self.first_item_row))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_geometry_keeps_a_real_item_and_action_row() {
        let inner = Rect::new(3, 2, 34, 3);
        let geometry = FilteredListGeometry::resolve(inner, 20, 19);
        assert_eq!(geometry.window, (19, 20));
        assert_eq!(geometry.first_item_row, 3);
        assert_eq!(geometry.action_row, 4);
        assert_eq!(geometry.selection_at(inner, 3, 3), Some(19));
        assert_eq!(geometry.selection_at(inner, 3, 4), None);
    }
}
