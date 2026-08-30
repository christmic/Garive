use super::*;
use ratatui::style::Color;

fn syntax() -> SyntaxPalette {
    SyntaxPalette::from_palette(super::super::style::palette(crate::Theme::Dark))
}

fn text(lines: &[Line<'static>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

#[test]
fn nested_inline_styles_compose_and_links_expose_their_destination() {
    let normal = Style::default().fg(Color::White);
    let accent = Style::default().fg(Color::Cyan);
    let muted = Style::default().fg(Color::DarkGray);
    let lines = render_markdown(
        "**outer *inner* tail** and [docs](https://example.com)",
        "",
        normal,
        accent,
        muted,
        syntax(),
        80,
    );

    assert_eq!(
        text(&lines),
        vec!["outer inner tail and docs (https://example.com)"]
    );
    let spans = &lines[0].spans;
    assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
    assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
    assert!(spans[1].style.add_modifier.contains(Modifier::ITALIC));
    assert!(spans[2].style.add_modifier.contains(Modifier::BOLD));
    assert!(spans[4].style.add_modifier.contains(Modifier::UNDERLINED));
    assert!(spans[6].style.add_modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn ordered_lists_and_fenced_code_keep_semantic_structure() {
    let lines = render_markdown(
        "3. first\n4. second\n\n```rust\nfn main() {}\n```",
        "",
        Style::default(),
        Style::default().add_modifier(Modifier::BOLD),
        Style::default().add_modifier(Modifier::DIM),
        syntax(),
        80,
    );

    assert_eq!(
        text(&lines),
        vec![
            "3. first",
            "4. second",
            "╭─ CODE · rust",
            "│ fn main() {}",
            "╰─",
        ]
    );

    let heading = render_markdown(
        "# accent\n\nplain",
        "",
        Style::default().fg(Color::White),
        Style::default().fg(Color::Cyan),
        Style::default().fg(Color::DarkGray),
        syntax(),
        80,
    );
    assert!(heading[0].spans[0]
        .style
        .add_modifier
        .contains(Modifier::UNDERLINED));
    assert_eq!(heading[1].spans[0].style.fg, Some(Color::White));
    assert!(!heading[1].spans[0]
        .style
        .add_modifier
        .contains(Modifier::UNDERLINED));

    let clipped = render_markdown(
        "```text\n\t界abcde\n```",
        "",
        Style::default(),
        Style::default(),
        Style::default(),
        syntax(),
        10,
    );
    assert_eq!(text(&clipped)[1], "│     界a…");
    assert_eq!(UnicodeWidthStr::width(text(&clipped)[1].as_str()), 10);
}

#[test]
fn fenced_code_highlights_known_languages_and_preserves_unknown_ones() {
    let known = render_markdown(
        "```rust\nfn answer() -> u64 { 42 }\n```",
        "",
        Style::default(),
        Style::default(),
        Style::default(),
        syntax(),
        80,
    );
    assert!(
        known[1].spans.len() > 2,
        "expected gutter plus styled tokens"
    );

    let unknown = render_markdown(
        "```garive-unknown\nvalue = 42\n```",
        "",
        Style::default(),
        Style::default(),
        Style::default(),
        syntax(),
        80,
    );
    assert_eq!(text(&unknown)[1], "│ value = 42");
    assert_eq!(unknown[1].spans.len(), 2, "gutter plus one plain span");
}

#[test]
fn tables_switch_between_styled_grid_and_narrow_records() {
    let source = "| Name | State |\n|:--|--:|\n| 界面 | **ready** |\n| API | idle |";
    let normal = Style::default().fg(Color::White);
    let accent = Style::default().fg(Color::Cyan);
    let muted = Style::default().fg(Color::DarkGray);

    let grid = render_markdown(source, "   ", normal, accent, muted, syntax(), 32);
    let grid_text = text(&grid);
    assert!(grid_text[0].contains("Name"));
    assert!(grid_text[0].contains("│"));
    assert!(grid_text[1].contains("┼"));
    assert!(grid.iter().all(|line| UnicodeWidthStr::width(
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
            .as_str()
    ) <= 32));
    let ready = grid
        .iter()
        .flat_map(|line| &line.spans)
        .find(|span| span.content.contains("ready"))
        .expect("ready cell");
    assert!(ready.style.add_modifier.contains(Modifier::BOLD));

    let records = render_markdown(source, "   ", normal, accent, muted, syntax(), 17);
    assert_eq!(
        text(&records),
        vec![
            "   Name: 界面",
            "   Sta…: ready",
            "   ···",
            "   Name: API",
            "   Sta…: idle",
        ]
    );
    assert!(!text(&records).iter().any(|line| line.contains('│')));
}
