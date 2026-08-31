#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::Theme;
#[path = "../src/application/mod.rs"]
mod application;
#[path = "../src/input/mod.rs"]
mod input;
#[path = "../src/view/mod.rs"]
mod view;

use application::{
    AppModel, BootState, ConversationLandmark, ExecutionState, FocusTarget, Overlay, TerminalSize,
};
use ratatui::{
    buffer::{Buffer, Cell},
    layout::Rect,
    style::Modifier,
};

#[test]
fn only_selected_command_and_navigator_rows_receive_accent_emphasis() {
    let mut evidence = Vec::new();
    for theme in [Theme::Dark, Theme::Light, Theme::Mono] {
        let suggestions = render(&suggestion_model(), theme, 100, 24);
        let suggestion_plain = cell_at(&suggestions, 100, 24, "/theme system", 1, 0);
        let suggestion_selected = cell_at(&suggestions, 100, 24, "/theme dark", 1, 0);
        assert_hierarchy(theme, suggestion_plain, suggestion_selected);

        let palette = render(&palette_model(), theme, 40, 8);
        let palette_plain = cell_at(&palette, 40, 8, "/help", 1, 0);
        let palette_selected = cell_at(&palette, 40, 8, "/quit", 1, 0);
        assert_hierarchy(theme, palette_plain, palette_selected);

        let turns = render(&turn_model(), theme, 100, 24);
        let turn_plain = cell_at(&turns, 100, 24, "release checkpoint 00", -3, 0);
        let turn_selected = cell_at(&turns, 100, 24, "release checkpoint 02", -3, 0);
        assert_eq!(turn_plain.symbol(), "1");
        assert_eq!(turn_selected.symbol(), "3");
        assert_hierarchy(theme, turn_plain, turn_selected);

        evidence.push(format!(
            "{theme:?}\n  suggestions plain={} selected={}\n  palette     plain={} selected={}\n  navigator   plain={} selected={}",
            style(suggestion_plain),
            style(suggestion_selected),
            style(palette_plain),
            style(palette_selected),
            style(turn_plain),
            style(turn_selected),
        ));
    }
    insta::assert_snapshot!("accent_hierarchy", evidence.join("\n\n"));
}

fn assert_hierarchy(theme: Theme, plain: &Cell, selected: &Cell) {
    assert!(!plain.modifier.contains(Modifier::BOLD));
    assert!(!plain.modifier.contains(Modifier::REVERSED));
    assert!(selected.modifier.contains(Modifier::BOLD));
    if theme == Theme::Mono {
        assert!(selected.modifier.contains(Modifier::REVERSED));
    } else {
        assert_ne!(selected.bg, plain.bg);
    }
}

fn suggestion_model() -> AppModel {
    let mut model = base();
    model.focus = FocusTarget::Composer;
    model.execution = ExecutionState::Idle;
    model.composer.replace("/theme ").unwrap();
    model.command_suggestion_selection = 1;
    model
}

fn palette_model() -> AppModel {
    AppModel {
        overlay: Some(Overlay::CommandPalette),
        command_selection: input::COMMAND_PALETTE.len() - 1,
        terminal_size: TerminalSize {
            width: 40,
            height: 8,
        },
        ..base()
    }
}

fn turn_model() -> AppModel {
    AppModel {
        overlay: Some(Overlay::TurnNavigator),
        turn_filter: "release".into(),
        turn_selection: 2,
        conversation_landmarks: (0..12)
            .map(|index| ConversationLandmark {
                ordinal: index + 1,
                started_position: index as u64 + 1,
                prompt_preview: format!("release checkpoint {index:02}"),
            })
            .collect(),
        ..base()
    }
}

fn base() -> AppModel {
    AppModel {
        boot: BootState::Ready,
        terminal_size: TerminalSize {
            width: 100,
            height: 24,
        },
        ..Default::default()
    }
}

fn render(model: &AppModel, theme: Theme, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    let _ = view::render_cached(
        model,
        theme,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    );
    buffer
}

fn cell_at<'a>(
    buffer: &'a Buffer,
    width: u16,
    height: u16,
    needle: &str,
    x_offset: i16,
    y_offset: i16,
) -> &'a Cell {
    for row in 0..height {
        let text = (0..width)
            .map(|column| buffer[(column, row)].symbol())
            .collect::<String>();
        if let Some(byte) = text.find(needle) {
            let column = i16::try_from(text[..byte].chars().count()).unwrap() + x_offset;
            let row = i16::try_from(row).unwrap() + y_offset;
            return &buffer[(u16::try_from(column).unwrap(), u16::try_from(row).unwrap())];
        }
    }
    panic!("missing {needle:?}");
}

fn style(cell: &Cell) -> String {
    format!(
        "fg={:?} bg={:?} modifier={:?}",
        cell.fg, cell.bg, cell.modifier
    )
}
