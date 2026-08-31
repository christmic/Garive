use unicode_segmentation::UnicodeSegmentation;

use super::{EditorState, SelectionEdge};

impl EditorState {
    pub(crate) fn move_logical_line_start(&mut self, selecting: bool) {
        if !selecting {
            self.collapse_selection(SelectionEdge::Start);
        }
        self.prepare_selection(selecting);
        let graphemes = self.text.graphemes(true).collect::<Vec<_>>();
        self.cursor_grapheme = graphemes[..self.cursor_grapheme]
            .iter()
            .rposition(|value| *value == "\n")
            .map_or(0, |newline| newline + 1);
        self.preferred_display_column = None;
    }

    pub(crate) fn move_logical_line_end(&mut self, selecting: bool) {
        if !selecting {
            self.collapse_selection(SelectionEdge::End);
        }
        self.prepare_selection(selecting);
        let graphemes = self.text.graphemes(true).collect::<Vec<_>>();
        self.cursor_grapheme = graphemes[self.cursor_grapheme..]
            .iter()
            .position(|value| *value == "\n")
            .map_or(graphemes.len(), |newline| self.cursor_grapheme + newline);
        self.preferred_display_column = None;
    }
}
