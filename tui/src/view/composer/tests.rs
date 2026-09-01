use super::*;
use crate::{view::style::palette, Theme};
use ratatui::{buffer::Buffer, layout::Rect};

#[test]
fn focused_frame_reserves_accent_for_one_input_marker() {
    let model = AppModel::default();
    let colors = palette(Theme::Dark);
    let area = Rect::new(0, 0, 24, 3);
    let mut buffer = Buffer::empty(area);

    render(&model, colors, area, &mut buffer);

    let border_cells = (0..area.width)
        .flat_map(|x| [(x, 0), (x, area.height - 1)])
        .chain((1..area.height - 1).flat_map(|y| [(0, y), (area.width - 1, y)]))
        .collect::<Vec<_>>();
    let accent_cells = border_cells
        .iter()
        .filter(|(x, y)| buffer[(*x, *y)].style().fg == colors.accent.fg)
        .count();

    assert!((1..=3).contains(&accent_cells));
    assert!(border_cells
        .iter()
        .any(|(x, y)| { buffer[(*x, *y)].style().fg == colors.border.fg }));
}

#[test]
fn word_wrap_and_cursor_share_the_same_rows() {
    let mut editor = EditorState::new(128);
    editor.replace("hello world").unwrap();
    let layout = EditorLayout::new(&editor, 8);
    let lines = layout
        .text(palette(Theme::Mono))
        .lines
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>();
    assert_eq!(lines, ["hello ", "world"]);
    assert_eq!(layout.visible_cursor(3), ((5, 1), 0));
}

#[test]
fn wrapped_selection_preserves_graphemes_and_semantic_style() {
    let mut editor = EditorState::new(128);
    editor.replace("ab 界e\u{301} cd").unwrap();
    editor.move_document_start(false);
    for _ in 0..5 {
        editor.move_right(true);
    }
    let colors = palette(Theme::Mono);
    let text = EditorLayout::new(&editor, 5).text(colors);
    assert_eq!(text.lines.len(), 3);
    let selected = text
        .lines
        .iter()
        .flat_map(|line| &line.spans)
        .filter(|span| span.style == colors.text_selection)
        .map(|span| span.content.as_ref())
        .collect::<String>();
    assert_eq!(selected, "ab 界e\u{301}");
}

#[test]
fn exact_width_cursor_advances_to_a_visible_continuation_row() {
    let mut editor = EditorState::new(128);
    editor.replace("12345").unwrap();
    assert_eq!(EditorLayout::new(&editor, 5).visible_cursor(2), ((0, 1), 0));
}

#[test]
fn explicit_newline_and_word_wrap_share_cursor_geometry() {
    let mut editor = EditorState::new(128);
    editor.replace("one\ntwo three").unwrap();
    let layout = EditorLayout::new(&editor, 6);
    let lines = layout
        .text(palette(Theme::Mono))
        .lines
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>();
    assert_eq!(lines, ["one", "two ", "three"]);
    assert_eq!(layout.visible_cursor(2), ((5, 2), 1));
}

#[test]
fn pointer_hit_testing_uses_wrapped_grapheme_boundaries() {
    let mut editor = EditorState::new(128);
    editor.replace("ab 界e\u{301} cd").unwrap();
    let layout = EditorLayout::new(&editor, 5);
    assert_eq!(layout.grapheme_at(0, 0), 0);
    assert_eq!(layout.grapheme_at(4, 0), 3);
    assert_eq!(layout.grapheme_at(0, 1), 3);
    assert_eq!(layout.grapheme_at(1, 1), 4);
    assert_eq!(layout.grapheme_at(4, 1), 6);
    assert_eq!(layout.grapheme_at(0, 9), 8);
}

#[test]
fn desired_height_counts_visual_rows_and_exact_width_cursor() {
    let mut editor = EditorState::new(128);
    assert_eq!(desired_height(&editor, 12), 3);
    editor.replace("hello world").unwrap();
    assert_eq!(desired_height(&editor, 12), 4);
    editor.replace("12345").unwrap();
    assert_eq!(desired_height(&editor, 9), 4);
}

#[test]
fn visual_vertical_navigation_uses_wrapped_rows_and_sticky_column() {
    let mut editor = EditorState::new(128);
    editor.replace("hello world").unwrap();
    let (target, preferred) = vertical_target(&editor, 8, -1);
    assert_eq!((target, preferred), (5, 5));
    editor.apply_visual_vertical_move(target, preferred, -1, false);
    assert_eq!(editor.cursor_grapheme(), 5);
    let (target, preferred) = vertical_target(&editor, 8, 1);
    assert_eq!((target, preferred), (11, 5));
    editor.apply_visual_vertical_move(target, preferred, 1, false);
    assert_eq!(editor.cursor_grapheme(), 11);
}

#[test]
fn visual_vertical_navigation_handles_wide_cells_and_continuation_rows() {
    let mut editor = EditorState::new(128);
    editor.replace("ab界cd").unwrap();
    assert_eq!(vertical_target(&editor, 4, -1), (2, 2));
    editor.replace("12345").unwrap();
    let (target, preferred) = vertical_target(&editor, 5, -1);
    assert_eq!((target, preferred), (0, 0));
    editor.apply_visual_vertical_move(target, preferred, -1, false);
    assert_eq!(vertical_target(&editor, 5, 1), (5, 0));
}

#[test]
fn visual_line_edges_follow_soft_wraps_and_exact_width_continuation() {
    let mut editor = EditorState::new(128);
    editor.replace("hello world").unwrap();
    editor.move_document_start(false);
    assert_eq!(line_edge_target(&editor, 8, 1), 5);
    editor.place_cursor(8, false);
    assert_eq!(line_edge_target(&editor, 8, -1), 6);
    assert_eq!(line_edge_target(&editor, 8, 1), 11);
    editor.move_document_end(false);
    editor.move_left(true);
    editor.move_left(true);
    assert_eq!(line_edge_target(&editor, 8, -1), 6);
    assert_eq!(line_edge_target(&editor, 8, 1), 11);
    editor.replace("12345").unwrap();
    assert_eq!(line_edge_target(&editor, 5, -1), 5);
    assert_eq!(line_edge_target(&editor, 5, 1), 5);
}
