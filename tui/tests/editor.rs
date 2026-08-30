#![allow(dead_code, unused_imports)]

#[path = "../src/input/editor.rs"]
mod editor;

use editor::{EditError, EditorState};

#[test]
fn edits_extended_graphemes_without_splitting_bytes() {
    let mut editor = EditorState::new(128);
    editor.insert("a👨‍👩‍👧‍👦界").unwrap();
    assert_eq!(editor.display_column(), 5);
    editor.move_left(false);
    assert!(editor.backspace());
    assert_eq!(editor.text(), "a界");
    assert!(editor.undo());
    assert_eq!(editor.text(), "a👨‍👩‍👧‍👦界");
    assert!(editor.redo());
    assert_eq!(editor.text(), "a界");
}

#[test]
fn paste_is_normalized_bounded_and_undoes_atomically() {
    let mut editor = EditorState::new(8);
    editor.insert("a\r\nb").unwrap();
    assert_eq!(editor.text(), "a\nb");
    assert_eq!(
        editor.insert("123456").unwrap_err(),
        EditError::TooLarge { excess_bytes: 1 }
    );
    assert_eq!(editor.text(), "a\nb");
    assert!(editor.undo());
    assert_eq!(editor.text(), "");
}

#[test]
fn selection_replacement_and_forward_delete_are_grapheme_safe() {
    let mut editor = EditorState::new(128);
    editor.insert("abc界").unwrap();
    editor.move_left(true);
    editor.move_left(true);
    editor.insert("Z").unwrap();
    assert_eq!(editor.text(), "abZ");
    editor.move_left(false);
    assert!(editor.delete());
    assert_eq!(editor.text(), "ab");
}

#[test]
fn selection_can_be_cleared_without_mutating_the_draft() {
    let mut editor = EditorState::new(128);
    editor.insert("abc界").unwrap();
    editor.move_left(true);
    assert!(editor.has_selection());

    editor.clear_selection();

    assert!(!editor.has_selection());
    assert_eq!(editor.text(), "abc界");
}

#[test]
fn selected_byte_range_follows_extended_grapheme_boundaries() {
    let mut editor = EditorState::new(128);
    editor.insert("a界e\u{301}z").unwrap();
    editor.move_left(true);
    editor.move_left(true);

    let (start, end) = editor.selected_byte_range().unwrap();
    assert_eq!(&editor.text()[start..end], "e\u{301}z");
}

#[test]
fn terminal_controls_and_bidi_overrides_never_enter_the_model() {
    let mut editor = EditorState::new(128);
    for value in ["secret\u{1b}[31m", "left\u{202e}right"] {
        assert_eq!(editor.insert(value), Err(EditError::UnsafeControl));
        assert_eq!(editor.text(), "");
    }
    editor.insert("isolate\u{2066}x\u{2069}").unwrap();
}

#[test]
fn word_deletion_document_motion_and_tab_expansion_are_grapheme_safe() {
    let mut editor = EditorState::new(1_024);
    editor.insert("one 界界 three\tend").unwrap();
    assert_eq!(editor.text(), "one 界界 three    end");
    editor.move_document_start(false);
    editor.move_word_right(false);
    assert!(editor.delete_word_right());
    assert_eq!(editor.text(), "one three    end");
    editor.move_document_end(false);
    assert!(editor.delete_word_left());
    assert_eq!(editor.text(), "one three    ");
}

#[test]
fn multiline_navigation_preserves_display_column_and_word_boundaries() {
    let mut editor = EditorState::new(100);
    editor.insert("ab 界\nx\nhello world").unwrap();
    editor.move_line_start(false);
    assert_eq!(editor.display_column(), 0);
    editor.move_up(false);
    assert_eq!(editor.display_column(), 0);
    editor.move_line_end(false);
    assert_eq!(editor.display_column(), 1);
    editor.move_up(false);
    assert_eq!(editor.display_column(), 1);
    editor.move_word_right(false);
    assert_eq!(editor.display_column(), 3);
    editor.move_word_left(false);
    assert_eq!(editor.display_column(), 0);
    editor.move_down(false);
    assert_eq!(editor.display_column(), 0);
}
