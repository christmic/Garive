//! Content-driven row geometry shared by Inspector render and hit testing.

use ratatui::layout::Rect;

use crate::application::InspectorEntry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct VisibleEntry {
    pub(super) index: usize,
    pub(super) area: Rect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct InspectorRowLayout {
    pub(super) entries: Vec<VisibleEntry>,
}

impl InspectorRowLayout {
    pub(super) fn resolve(inner: Rect, entries: &[InspectorEntry], selected: usize) -> Self {
        let available = inner.height.saturating_sub(1);
        if entries.is_empty() || available == 0 {
            return Self { entries: vec![] };
        }

        let selected = selected.min(entries.len() - 1);
        let mut start = selected;
        let mut end = selected + 1;
        let mut used = entry_rows(&entries[selected]).min(available);

        while start > 0 {
            let rows = entry_rows(&entries[start - 1]);
            if used.saturating_add(rows) > available {
                break;
            }
            start -= 1;
            used += rows;
        }
        while end < entries.len() {
            let rows = entry_rows(&entries[end]);
            if used.saturating_add(rows) > available {
                break;
            }
            used += rows;
            end += 1;
        }

        let mut y = inner.y;
        let bottom = inner.bottom().saturating_sub(1);
        let entries = (start..end)
            .map(|index| {
                let height = entry_rows(&entries[index]).min(bottom.saturating_sub(y));
                let area = Rect::new(inner.x, y, inner.width, height);
                y = y.saturating_add(height);
                VisibleEntry { index, area }
            })
            .filter(|entry| entry.area.height > 0)
            .collect();
        Self { entries }
    }

    pub(super) fn selection_at(&self, column: u16, row: u16) -> Option<usize> {
        self.entries
            .iter()
            .find(|entry| entry.area.contains((column, row).into()))
            .map(|entry| entry.index)
    }
}

pub(super) fn desired_rows(entries: &[InspectorEntry]) -> u16 {
    entries
        .iter()
        .fold(0_u16, |rows, entry| rows.saturating_add(entry_rows(entry)))
}

pub(super) fn has_detail(entry: &InspectorEntry) -> bool {
    let detail = entry.detail.trim();
    !detail.is_empty()
        && !entry
            .label
            .trim()
            .to_lowercase()
            .ends_with(&format!("· {}", detail.to_lowercase()))
}

fn entry_rows(entry: &InspectorEntry) -> u16 {
    1 + u16::from(has_detail(entry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{InspectorActivation, InspectorTone};

    fn entry(label: &str, detail: &str) -> InspectorEntry {
        InspectorEntry {
            key: label.into(),
            label: label.into(),
            detail: detail.into(),
            tone: InspectorTone::Neutral,
            activation: InspectorActivation::None,
        }
    }

    #[test]
    fn repeated_or_empty_detail_uses_one_row() {
        let entries = [
            entry("Agent action · completed", "Completed"),
            entry("No detail", ""),
            entry("Running tests", "cargo test --workspace"),
        ];
        assert_eq!(desired_rows(&entries), 4);
        assert!(!has_detail(&entries[0]));
        assert!(has_detail(&entries[2]));
    }

    #[test]
    fn variable_rows_share_window_and_hit_geometry() {
        let entries = [
            entry("Completed · completed", "Completed"),
            entry("Running tests", "cargo test"),
            entry("Waiting · pending", "Pending"),
        ];
        let layout = InspectorRowLayout::resolve(Rect::new(4, 7, 20, 4), &entries, 1);
        assert_eq!(
            layout.entries,
            vec![
                VisibleEntry {
                    index: 0,
                    area: Rect::new(4, 7, 20, 1),
                },
                VisibleEntry {
                    index: 1,
                    area: Rect::new(4, 8, 20, 2),
                },
            ]
        );
        assert_eq!(layout.selection_at(5, 7), Some(0));
        assert_eq!(layout.selection_at(5, 8), Some(1));
        assert_eq!(layout.selection_at(5, 9), Some(1));
        assert_eq!(layout.selection_at(5, 10), None);
    }
}
