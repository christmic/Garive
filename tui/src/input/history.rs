#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HistoryDraft {
    pub(crate) text: String,
    pub(crate) cursor_grapheme: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HistoryRecall {
    Entry(String),
    Draft(HistoryDraft),
}

#[derive(Debug, Default)]
pub(crate) struct PromptHistoryBrowser {
    index: Option<usize>,
    draft: Option<HistoryDraft>,
}

impl PromptHistoryBrowser {
    pub(crate) fn is_active(&self) -> bool {
        self.index.is_some()
    }

    pub(crate) fn reset(&mut self) {
        self.index = None;
        self.draft = None;
    }

    pub(crate) fn older(
        &mut self,
        history: &[String],
        current: HistoryDraft,
    ) -> Option<HistoryRecall> {
        if history.is_empty() {
            return None;
        }
        if self.index.is_none() {
            self.draft = Some(current);
        }
        let index = self
            .index
            .map_or(0, |index| index.saturating_add(1).min(history.len() - 1));
        self.index = Some(index);
        Some(HistoryRecall::Entry(history[index].clone()))
    }

    pub(crate) fn newer(&mut self, history: &[String]) -> Option<HistoryRecall> {
        let index = self.index?;
        if index > 0 {
            let index = index - 1;
            self.index = Some(index);
            return history.get(index).cloned().map(HistoryRecall::Entry);
        }
        self.index = None;
        self.draft.take().map(HistoryRecall::Draft)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(text: &str, cursor_grapheme: usize) -> HistoryDraft {
        HistoryDraft {
            text: text.into(),
            cursor_grapheme,
        }
    }

    #[test]
    fn browsing_is_bounded_and_restores_the_original_draft_cursor() {
        let history = vec!["newest".into(), "oldest".into()];
        let mut browser = PromptHistoryBrowser::default();
        assert_eq!(
            browser.older(&history, draft("working", 3)),
            Some(HistoryRecall::Entry("newest".into()))
        );
        assert_eq!(
            browser.older(&history, draft("ignored", 0)),
            Some(HistoryRecall::Entry("oldest".into()))
        );
        assert_eq!(
            browser.older(&history, draft("ignored", 0)),
            Some(HistoryRecall::Entry("oldest".into()))
        );
        assert_eq!(
            browser.newer(&history),
            Some(HistoryRecall::Entry("newest".into()))
        );
        assert_eq!(
            browser.newer(&history),
            Some(HistoryRecall::Draft(draft("working", 3)))
        );
        assert!(!browser.is_active());
    }

    #[test]
    fn empty_history_and_reset_never_destroy_a_draft() {
        let mut browser = PromptHistoryBrowser::default();
        assert_eq!(browser.older(&[], draft("safe", 2)), None);
        assert_eq!(browser.newer(&[]), None);
        let _ = browser.older(&["past".into()], draft("safe", 2));
        browser.reset();
        assert_eq!(browser.newer(&["past".into()]), None);
    }
}
