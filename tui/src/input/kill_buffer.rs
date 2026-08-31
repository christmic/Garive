use unicode_segmentation::UnicodeSegmentation;

use super::{grapheme_byte, EditError, EditorState};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct KillBuffer {
    text: String,
}

impl EditorState {
    pub(crate) fn kill_to_logical_line_start(&mut self) -> bool {
        if self.has_selection() {
            return self.kill_selection();
        }
        let graphemes = self.text.graphemes(true).collect::<Vec<_>>();
        let cursor = self.cursor_grapheme;
        let line_start = graphemes[..cursor]
            .iter()
            .rposition(|value| *value == "\n")
            .map_or(0, |newline| newline + 1);
        let start = if cursor == line_start && line_start > 0 {
            line_start - 1
        } else {
            line_start
        };
        self.kill_grapheme_range(start, cursor)
    }

    pub(crate) fn kill_to_logical_line_end(&mut self) -> bool {
        if self.has_selection() {
            return self.kill_selection();
        }
        let graphemes = self.text.graphemes(true).collect::<Vec<_>>();
        let cursor = self.cursor_grapheme;
        let line_end = graphemes[cursor..]
            .iter()
            .position(|value| *value == "\n")
            .map_or(graphemes.len(), |newline| cursor + newline);
        let end = if cursor == line_end && line_end < graphemes.len() {
            line_end + 1
        } else {
            line_end
        };
        self.kill_grapheme_range(cursor, end)
    }

    pub(crate) fn yank(&mut self) -> Result<bool, EditError> {
        let Some(text) = self.kill_buffer.text().map(str::to_owned) else {
            return Ok(false);
        };
        self.insert(&text)?;
        Ok(true)
    }

    pub(crate) fn clear_private_edit_buffer(&mut self) {
        self.kill_buffer.clear();
    }

    fn kill_selection(&mut self) -> bool {
        let (start, end) = self.selection_bytes();
        self.kill_byte_range(start, end)
    }

    fn kill_grapheme_range(&mut self, start: usize, end: usize) -> bool {
        self.kill_byte_range(
            grapheme_byte(&self.text, start),
            grapheme_byte(&self.text, end),
        )
    }

    fn kill_byte_range(&mut self, start: usize, end: usize) -> bool {
        if start == end {
            return false;
        }
        self.kill_buffer.store(&self.text[start..end]);
        self.checkpoint();
        self.text.replace_range(start..end, "");
        self.cursor_grapheme = self.text[..start].graphemes(true).count();
        self.selection_anchor = None;
        self.preferred_display_column = None;
        true
    }
}

impl KillBuffer {
    pub(super) fn store(&mut self, text: &str) {
        if !text.is_empty() {
            self.text.clear();
            self.text.push_str(text);
        }
    }

    pub(super) fn text(&self) -> Option<&str> {
        (!self.text.is_empty()).then_some(self.text.as_str())
    }

    pub(super) fn clear(&mut self) {
        self.text.clear();
    }
}
