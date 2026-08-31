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

use application::{AppModel, InspectorVariant, Overlay, TerminalSize};
use garive_host_client::SuspensionView;
use ratatui::{buffer::Buffer, layout::Rect, style::Modifier};

#[test]
fn decision_choice_uses_the_complete_mono_selection_row() {
    let model = suspension_model(r#"{"type":"boolean"}"#);
    let buffer = render(&model, Theme::Mono, 100, 24);
    let marker = find_symbol(&buffer, "›").expect("selected choice marker");
    assert!(buffer[marker].modifier.contains(Modifier::REVERSED));

    let row_end = (0..buffer.area.width)
        .rev()
        .find(|column| {
            buffer[(*column, marker.1)]
                .modifier
                .contains(Modifier::REVERSED)
        })
        .expect("selected row style");
    assert!(row_end.saturating_sub(marker.0) > 20);
}

#[test]
fn scalar_editor_caret_has_a_distinct_mono_style() {
    let mut model = suspension_model(r#"{"type":"string","maxLength":20}"#);
    model
        .suspension_response
        .as_mut()
        .unwrap()
        .editor
        .insert("abcd")
        .unwrap();
    model
        .suspension_response
        .as_mut()
        .unwrap()
        .editor
        .place_cursor(2, false);
    let buffer = render(&model, Theme::Mono, 52, 12);
    let caret = find_symbol(&buffer, "▏").expect("response caret");
    assert!(buffer[caret].modifier.contains(Modifier::BOLD));
    assert!(!buffer[(caret.0 + 1, caret.1)]
        .modifier
        .contains(Modifier::BOLD));
}

fn suspension_model(response_schema_json: &str) -> AppModel {
    let mut model = AppModel {
        terminal_size: TerminalSize {
            width: 100,
            height: 24,
        },
        selected_session: Some("session".into()),
        selected_turn: Some("turn".into()),
        overlay: Some(Overlay::Suspension),
        suspension: Some(SuspensionView {
            suspension_id: "suspension".into(),
            session_version: 2,
            kind: "external_input_required".into(),
            prompt_schema: "garive.public-suspension-prompt.v1".into(),
            prompt_json: r#"{"schema_version":1,"title_key":"title","message_text":"Continue?","action_label_key":"send"}"#.into(),
            prompt_digest: "0".repeat(64),
            response_schema_json: Some(response_schema_json.into()),
            response_schema_digest: Some("1".repeat(64)),
        }),
        ..Default::default()
    };
    model.reconcile_suspension_response();
    model
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

fn find_symbol(buffer: &Buffer, symbol: &str) -> Option<(u16, u16)> {
    (0..buffer.area.height)
        .flat_map(|row| (0..buffer.area.width).map(move |column| (column, row)))
        .find(|position| buffer[*position].symbol() == symbol)
}
