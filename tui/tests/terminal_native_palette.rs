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

use application::{AppModel, BootState, TimelineItem, TimelineRole, TimelineTone};
use ratatui::{
    buffer::{Buffer, Cell},
    layout::Rect,
    style::{Color, Modifier},
};

#[test]
fn prose_and_passive_hierarchy_follow_the_terminal_palette() {
    let mut model = AppModel {
        boot: BootState::Ready,
        ..Default::default()
    };
    for item in [
        timeline("user", TimelineRole::User, TimelineTone::Neutral, "Request"),
        timeline(
            "activity",
            TimelineRole::Status,
            TimelineTone::Neutral,
            "Inspecting",
        ),
        timeline(
            "answer",
            TimelineRole::Agent,
            TimelineTone::Neutral,
            "Answer body",
        ),
    ] {
        model.push_test_timeline_item(item);
    }

    let buffer = render(&model, Theme::Dark, 100, 18);
    let answer = cell_at(&buffer, "Answer body", 0);
    assert_eq!(answer.fg, Color::Reset);

    let passive_marker = cell_at(&buffer, "• Activity", 0);
    assert_eq!(passive_marker.fg, Color::Reset);
    assert!(passive_marker.modifier.contains(Modifier::DIM));

    let placeholder = cell_at(&buffer, "Ask Garive anything", 0);
    assert_eq!(placeholder.fg, Color::Reset);
    assert!(placeholder.modifier.contains(Modifier::DIM));
    assert!(placeholder.modifier.contains(Modifier::ITALIC));
}

fn timeline(key: &str, role: TimelineRole, tone: TimelineTone, text: &str) -> TimelineItem {
    TimelineItem {
        stable_key: key.into(),
        position: 1,
        role,
        tone,
        text: text.into(),
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

fn cell_at<'a>(buffer: &'a Buffer, needle: &str, offset: u16) -> &'a Cell {
    for row in 0..buffer.area.height {
        let text = (0..buffer.area.width)
            .map(|column| buffer[(column, row)].symbol())
            .collect::<String>();
        if let Some(byte) = text.find(needle) {
            let column = u16::try_from(text[..byte].chars().count()).unwrap();
            return &buffer[(column + offset, row)];
        }
    }
    panic!("missing {needle:?}");
}
