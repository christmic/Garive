use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const MAX_UNDO_OPERATIONS: usize = 100;
const MAX_UNDO_BYTES: usize = 256 * 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Snapshot {
    text: String,
    cursor: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditError {
    TooLarge { excess_bytes: usize },
    UnsafeControl,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EditorState {
    text: String,
    cursor_grapheme: usize,
    selection_anchor: Option<usize>,
    preferred_display_column: Option<usize>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    max_bytes: usize,
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new(4_096)
    }
}

impl EditorState {
    pub(crate) fn new(max_bytes: usize) -> Self {
        Self {
            text: String::new(),
            cursor_grapheme: 0,
            selection_anchor: None,
            preferred_display_column: None,
            undo: Vec::new(),
            redo: Vec::new(),
            max_bytes,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn display_column(&self) -> usize {
        let byte = grapheme_byte(&self.text, self.cursor_grapheme);
        let line = self.text[..byte]
            .rsplit_once('\n')
            .map_or(&self.text[..byte], |v| v.1);
        UnicodeWidthStr::width(line)
    }

    pub(crate) fn cursor_line(&self) -> usize {
        let byte = grapheme_byte(&self.text, self.cursor_grapheme);
        self.text[..byte]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
    }

    pub(crate) fn line_count(&self) -> usize {
        self.text.bytes().filter(|byte| *byte == b'\n').count() + 1
    }

    pub(crate) fn insert(&mut self, value: &str) -> Result<(), EditError> {
        let normalized = value
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\t', "    ");
        if normalized.chars().any(is_unsafe_control) {
            return Err(EditError::UnsafeControl);
        }
        if normalized.is_empty() && !self.has_selection() {
            return Ok(());
        }
        let (start, end) = self.selection_bytes();
        let new_len = self.text.len() - (end - start) + normalized.len();
        if new_len > self.max_bytes {
            return Err(EditError::TooLarge {
                excess_bytes: new_len - self.max_bytes,
            });
        }
        self.checkpoint();
        self.text.replace_range(start..end, &normalized);
        self.cursor_grapheme =
            self.text[..start].graphemes(true).count() + normalized.graphemes(true).count();
        self.selection_anchor = None;
        self.preferred_display_column = None;
        Ok(())
    }

    pub(crate) fn backspace(&mut self) -> bool {
        if self.has_selection() {
            return self.delete_selection();
        }
        if self.cursor_grapheme == 0 {
            return false;
        }
        self.selection_anchor = Some(self.cursor_grapheme - 1);
        self.delete_selection()
    }

    pub(crate) fn delete(&mut self) -> bool {
        if self.has_selection() {
            return self.delete_selection();
        }
        if self.cursor_grapheme == self.grapheme_len() {
            return false;
        }
        self.selection_anchor = Some(self.cursor_grapheme + 1);
        self.delete_selection()
    }

    pub(crate) fn move_left(&mut self, selecting: bool) {
        self.prepare_selection(selecting);
        self.cursor_grapheme = self.cursor_grapheme.saturating_sub(1);
        self.preferred_display_column = None;
    }

    pub(crate) fn move_right(&mut self, selecting: bool) {
        self.prepare_selection(selecting);
        self.cursor_grapheme = (self.cursor_grapheme + 1).min(self.grapheme_len());
        self.preferred_display_column = None;
    }

    pub(crate) fn move_up(&mut self, selecting: bool) {
        self.move_vertical(-1, selecting);
    }

    pub(crate) fn move_down(&mut self, selecting: bool) {
        self.move_vertical(1, selecting);
    }

    pub(crate) fn move_line_start(&mut self, selecting: bool) {
        self.prepare_selection(selecting);
        let byte = grapheme_byte(&self.text, self.cursor_grapheme);
        let start = self.text[..byte].rfind('\n').map_or(0, |value| value + 1);
        self.cursor_grapheme = self.text[..start].graphemes(true).count();
        self.preferred_display_column = None;
    }

    pub(crate) fn move_line_end(&mut self, selecting: bool) {
        self.prepare_selection(selecting);
        let byte = grapheme_byte(&self.text, self.cursor_grapheme);
        let end = self.text[byte..]
            .find('\n')
            .map_or(self.text.len(), |value| byte + value);
        self.cursor_grapheme = self.text[..end].graphemes(true).count();
        self.preferred_display_column = None;
    }

    pub(crate) fn move_word_left(&mut self, selecting: bool) {
        self.prepare_selection(selecting);
        let graphemes = self.text.graphemes(true).collect::<Vec<_>>();
        let mut cursor = self.cursor_grapheme;
        while cursor > 0 && graphemes[cursor - 1].chars().all(char::is_whitespace) {
            cursor -= 1;
        }
        while cursor > 0 && !graphemes[cursor - 1].chars().all(char::is_whitespace) {
            cursor -= 1;
        }
        self.cursor_grapheme = cursor;
        self.preferred_display_column = None;
    }

    pub(crate) fn move_word_right(&mut self, selecting: bool) {
        self.prepare_selection(selecting);
        let graphemes = self.text.graphemes(true).collect::<Vec<_>>();
        let mut cursor = self.cursor_grapheme;
        while cursor < graphemes.len() && !graphemes[cursor].chars().all(char::is_whitespace) {
            cursor += 1;
        }
        while cursor < graphemes.len() && graphemes[cursor].chars().all(char::is_whitespace) {
            cursor += 1;
        }
        self.cursor_grapheme = cursor;
        self.preferred_display_column = None;
    }

    pub(crate) fn delete_word_left(&mut self) -> bool {
        if self.has_selection() {
            return self.delete_selection();
        }
        let end = self.cursor_grapheme;
        self.move_word_left(true);
        self.selection_anchor = Some(end);
        self.delete_selection()
    }

    pub(crate) fn delete_word_right(&mut self) -> bool {
        if self.has_selection() {
            return self.delete_selection();
        }
        let start = self.cursor_grapheme;
        self.move_word_right(true);
        self.selection_anchor = Some(start);
        self.delete_selection()
    }

    pub(crate) fn move_document_start(&mut self, selecting: bool) {
        self.prepare_selection(selecting);
        self.cursor_grapheme = 0;
        self.preferred_display_column = None;
    }

    pub(crate) fn move_document_end(&mut self, selecting: bool) {
        self.prepare_selection(selecting);
        self.cursor_grapheme = self.grapheme_len();
        self.preferred_display_column = None;
    }

    pub(crate) fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(previous);
        true
    }

    pub(crate) fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(next);
        true
    }

    pub(crate) fn clear(&mut self) {
        if self.text.is_empty() {
            return;
        }
        self.checkpoint();
        self.text.clear();
        self.cursor_grapheme = 0;
        self.selection_anchor = None;
        self.preferred_display_column = None;
    }

    pub(crate) fn replace(&mut self, value: &str) -> Result<(), EditError> {
        if value.len() > self.max_bytes || value.chars().any(is_unsafe_control) {
            return Err(EditError::TooLarge {
                excess_bytes: value.len().saturating_sub(self.max_bytes),
            });
        }
        self.text = value.into();
        self.cursor_grapheme = self.text.graphemes(true).count();
        self.selection_anchor = None;
        self.preferred_display_column = None;
        self.undo.clear();
        self.redo.clear();
        Ok(())
    }

    fn grapheme_len(&self) -> usize {
        self.text.graphemes(true).count()
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.selection_anchor
            .is_some_and(|anchor| anchor != self.cursor_grapheme)
    }

    pub(crate) fn selected_byte_range(&self) -> Option<(usize, usize)> {
        self.has_selection().then(|| self.selection_bytes())
    }

    pub(crate) fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    fn selection_bytes(&self) -> (usize, usize) {
        let anchor = self.selection_anchor.unwrap_or(self.cursor_grapheme);
        let (start, end) = (
            anchor.min(self.cursor_grapheme),
            anchor.max(self.cursor_grapheme),
        );
        (
            grapheme_byte(&self.text, start),
            grapheme_byte(&self.text, end),
        )
    }

    fn delete_selection(&mut self) -> bool {
        let (start, end) = self.selection_bytes();
        if start == end {
            return false;
        }
        self.checkpoint();
        self.text.replace_range(start..end, "");
        self.cursor_grapheme = self.text[..start].graphemes(true).count();
        self.selection_anchor = None;
        self.preferred_display_column = None;
        true
    }

    fn prepare_selection(&mut self, selecting: bool) {
        if selecting {
            self.selection_anchor.get_or_insert(self.cursor_grapheme);
        } else {
            self.selection_anchor = None;
        }
    }

    fn move_vertical(&mut self, direction: i8, selecting: bool) {
        let current_line = self.cursor_line();
        let line_count = self.line_count();
        let target_line = if direction < 0 {
            current_line.saturating_sub(1)
        } else {
            (current_line + 1).min(line_count.saturating_sub(1))
        };
        if target_line == current_line {
            return;
        }
        self.prepare_selection(selecting);
        let lines = self.text.split('\n').collect::<Vec<_>>();
        let column = self
            .preferred_display_column
            .unwrap_or_else(|| self.display_column());
        self.preferred_display_column = Some(column);
        let prefix_graphemes = lines[..target_line]
            .iter()
            .map(|line| line.graphemes(true).count() + 1)
            .sum::<usize>();
        let mut width = 0;
        let mut in_line = 0;
        for grapheme in lines[target_line].graphemes(true) {
            let next = width + UnicodeWidthStr::width(grapheme);
            if next > column {
                break;
            }
            width = next;
            in_line += 1;
        }
        self.cursor_grapheme = prefix_graphemes + in_line;
    }

    fn checkpoint(&mut self) {
        self.undo.push(self.snapshot());
        self.redo.clear();
        while self.undo.len() > MAX_UNDO_OPERATIONS
            || self.undo.iter().map(|item| item.text.len()).sum::<usize>() > MAX_UNDO_BYTES
        {
            self.undo.remove(0);
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            cursor: self.cursor_grapheme,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.cursor_grapheme = snapshot.cursor;
        self.selection_anchor = None;
        self.preferred_display_column = None;
    }
}

fn grapheme_byte(text: &str, index: usize) -> usize {
    text.grapheme_indices(true)
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

fn is_unsafe_control(character: char) -> bool {
    (character.is_control() && character != '\n' && character != '\t')
        || matches!(character, '\u{202a}'..='\u{202e}')
}
