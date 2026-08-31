use unicode_segmentation::UnicodeSegmentation;

#[path = "kill_buffer.rs"]
mod kill_buffer;
#[path = "logical_line.rs"]
mod logical_line;

use kill_buffer::KillBuffer;

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
    kill_buffer: KillBuffer,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    max_bytes: usize,
}

#[derive(Clone, Copy)]
enum SelectionEdge {
    Start,
    End,
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
            kill_buffer: KillBuffer::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            max_bytes,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn cursor_grapheme(&self) -> usize {
        self.cursor_grapheme
    }

    pub(crate) fn place_cursor(&mut self, grapheme: usize, selecting: bool) {
        self.prepare_selection(selecting);
        self.cursor_grapheme = grapheme.min(self.grapheme_len());
        self.preferred_display_column = None;
    }

    pub(crate) fn select_word_at(&mut self, grapheme: usize) -> bool {
        let graphemes = self.text.graphemes(true).collect::<Vec<_>>();
        let Some(index) = hit_grapheme_index(grapheme, graphemes.len()) else {
            self.place_cursor(grapheme, false);
            return false;
        };
        let class = grapheme_class(graphemes[index]);
        if class == GraphemeClass::Whitespace {
            self.place_cursor(grapheme, false);
            return false;
        }
        let mut start = index;
        while start > 0 && grapheme_class(graphemes[start - 1]) == class {
            start -= 1;
        }
        let mut end = index + 1;
        while end < graphemes.len() && grapheme_class(graphemes[end]) == class {
            end += 1;
        }
        self.selection_anchor = Some(start);
        self.cursor_grapheme = end;
        self.preferred_display_column = None;
        true
    }

    pub(crate) fn select_logical_line_at(&mut self, grapheme: usize) -> bool {
        let graphemes = self.text.graphemes(true).collect::<Vec<_>>();
        if graphemes.is_empty() {
            self.place_cursor(0, false);
            return false;
        }
        let index = grapheme.min(graphemes.len().saturating_sub(1));
        let start = graphemes[..index]
            .iter()
            .rposition(|value| *value == "\n")
            .map_or(0, |newline| newline + 1);
        let end = graphemes[index..]
            .iter()
            .position(|value| *value == "\n")
            .map_or(graphemes.len(), |newline| index + newline + 1);
        self.selection_anchor = Some(start);
        self.cursor_grapheme = end;
        self.preferred_display_column = None;
        start != end
    }

    pub(crate) fn visual_vertical_state(&self, direction: i8) -> (usize, Option<usize>) {
        (
            self.visual_directional_origin(direction),
            if self.has_selection() {
                None
            } else {
                self.preferred_display_column
            },
        )
    }

    pub(crate) fn visual_directional_origin(&self, direction: i8) -> usize {
        let Some(anchor) = self
            .selection_anchor
            .filter(|anchor| *anchor != self.cursor_grapheme)
        else {
            return self.cursor_grapheme;
        };
        if direction < 0 {
            anchor.min(self.cursor_grapheme)
        } else {
            anchor.max(self.cursor_grapheme)
        }
    }

    pub(crate) fn apply_visual_vertical_move(
        &mut self,
        target: usize,
        preferred_column: usize,
        direction: i8,
        selecting: bool,
    ) {
        if !selecting {
            self.collapse_selection(if direction < 0 {
                SelectionEdge::Start
            } else {
                SelectionEdge::End
            });
        }
        self.prepare_selection(selecting);
        self.cursor_grapheme = target.min(self.grapheme_len());
        self.preferred_display_column = Some(preferred_column);
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
        if !selecting && self.collapse_selection(SelectionEdge::Start) {
            return;
        }
        self.prepare_selection(selecting);
        self.cursor_grapheme = self.cursor_grapheme.saturating_sub(1);
        self.preferred_display_column = None;
    }

    pub(crate) fn move_right(&mut self, selecting: bool) {
        if !selecting && self.collapse_selection(SelectionEdge::End) {
            return;
        }
        self.prepare_selection(selecting);
        self.cursor_grapheme = (self.cursor_grapheme + 1).min(self.grapheme_len());
        self.preferred_display_column = None;
    }

    pub(crate) fn move_word_left(&mut self, selecting: bool) {
        if !selecting {
            self.collapse_selection(SelectionEdge::Start);
        }
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
        if !selecting {
            self.collapse_selection(SelectionEdge::End);
        }
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
        if !selecting {
            self.collapse_selection(SelectionEdge::Start);
        }
        self.prepare_selection(selecting);
        self.cursor_grapheme = 0;
        self.preferred_display_column = None;
    }

    pub(crate) fn move_document_end(&mut self, selecting: bool) {
        if !selecting {
            self.collapse_selection(SelectionEdge::End);
        }
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
        self.validate_replacement(value)?;
        self.text = value.into();
        self.cursor_grapheme = self.text.graphemes(true).count();
        self.selection_anchor = None;
        self.preferred_display_column = None;
        self.undo.clear();
        self.redo.clear();
        Ok(())
    }

    pub(crate) fn replace_undoable(&mut self, value: &str) -> Result<(), EditError> {
        self.validate_replacement(value)?;
        if self.text == value {
            return Ok(());
        }
        self.checkpoint();
        self.text = value.into();
        self.cursor_grapheme = self.text.graphemes(true).count();
        self.selection_anchor = None;
        self.preferred_display_column = None;
        Ok(())
    }

    fn validate_replacement(&self, value: &str) -> Result<(), EditError> {
        if value.len() > self.max_bytes {
            return Err(EditError::TooLarge {
                excess_bytes: value.len().saturating_sub(self.max_bytes),
            });
        }
        if value.chars().any(is_unsafe_control) {
            return Err(EditError::UnsafeControl);
        }
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

    pub(crate) fn selected_text(&self) -> Option<&str> {
        self.selected_byte_range()
            .map(|(start, end)| &self.text[start..end])
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

    fn collapse_selection(&mut self, edge: SelectionEdge) -> bool {
        let Some(anchor) = self
            .selection_anchor
            .filter(|anchor| *anchor != self.cursor_grapheme)
        else {
            return false;
        };
        self.cursor_grapheme = match edge {
            SelectionEdge::Start => anchor.min(self.cursor_grapheme),
            SelectionEdge::End => anchor.max(self.cursor_grapheme),
        };
        self.selection_anchor = None;
        self.preferred_display_column = None;
        true
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

#[derive(Clone, Copy, Eq, PartialEq)]
enum GraphemeClass {
    Whitespace,
    Word,
    Punctuation,
}

fn grapheme_class(grapheme: &str) -> GraphemeClass {
    if grapheme.chars().all(char::is_whitespace) {
        GraphemeClass::Whitespace
    } else if grapheme
        .chars()
        .next()
        .is_some_and(|character| character.is_alphanumeric() || character == '_')
    {
        GraphemeClass::Word
    } else {
        GraphemeClass::Punctuation
    }
}

fn hit_grapheme_index(position: usize, len: usize) -> Option<usize> {
    (len != 0).then(|| position.min(len - 1))
}
fn is_unsafe_control(character: char) -> bool {
    (character.is_control() && character != '\n' && character != '\t')
        || matches!(character, '\u{202a}'..='\u{202e}')
}
