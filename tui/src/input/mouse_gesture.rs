use std::time::{Duration, Instant};

const MULTI_CLICK_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ComposerClick {
    Place,
    SelectWord,
    SelectLine,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ComposerClickTracker {
    previous: Option<((u16, u16), Instant)>,
    count: u8,
}

impl ComposerClickTracker {
    pub(crate) fn register(&mut self, column: u16, row: u16) -> ComposerClick {
        self.register_at(column, row, Instant::now())
    }

    fn register_at(&mut self, column: u16, row: u16, now: Instant) -> ComposerClick {
        let position = (column, row);
        let continues = self.previous.is_some_and(|(previous, at)| {
            previous == position && now.saturating_duration_since(at) <= MULTI_CLICK_INTERVAL
        });
        self.count = if continues { self.count % 3 + 1 } else { 1 };
        self.previous = Some((position, now));
        match self.count {
            2 => ComposerClick::SelectWord,
            3 => ComposerClick::SelectLine,
            _ => ComposerClick::Place,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.previous = None;
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_cell_clicks_cycle_place_word_line_then_place() {
        let start = Instant::now();
        let mut tracker = ComposerClickTracker::default();
        assert_eq!(tracker.register_at(4, 2, start), ComposerClick::Place);
        assert_eq!(
            tracker.register_at(4, 2, start + Duration::from_millis(100)),
            ComposerClick::SelectWord
        );
        assert_eq!(
            tracker.register_at(4, 2, start + Duration::from_millis(200)),
            ComposerClick::SelectLine
        );
        assert_eq!(
            tracker.register_at(4, 2, start + Duration::from_millis(300)),
            ComposerClick::Place
        );
    }

    #[test]
    fn position_timeout_and_reset_break_a_sequence() {
        let start = Instant::now();
        let mut tracker = ComposerClickTracker::default();
        let _ = tracker.register_at(4, 2, start);
        assert_eq!(tracker.register_at(5, 2, start), ComposerClick::Place);
        assert_eq!(
            tracker.register_at(5, 2, start + Duration::from_millis(501)),
            ComposerClick::Place
        );
        tracker.reset();
        assert_eq!(tracker.register_at(5, 2, start), ComposerClick::Place);
    }
}
