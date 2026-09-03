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

use application::{AppModel, ConnectionState, InspectorVariant, Overlay, TerminalSize};
use ratatui::{buffer::Buffer, layout::Rect};

#[test]
fn connection_hint_keeps_truth_and_safe_action_at_full_and_narrow_widths() {
    let mut model = AppModel {
        connection: ConnectionState::Disconnected { attempt: 2 },
        ..Default::default()
    };
    assert_eq!(
        hint(&model, 100),
        "/reconnect resume events  ·  Updates paused · attempt 2/5"
    );
    let narrow = hint(&model, 40);
    assert!(
        narrow.starts_with("/reconnect resume events"),
        "narrow footer: {narrow:?}"
    );

    model.connection = ConnectionState::Reconnecting { attempt: 3 };
    assert_eq!(
        hint(&model, 100),
        "/status view details  ·  Updates paused · attempt 3/5"
    );
    assert!(hint(&model, 40).starts_with("/status view details"));

    model.connection = ConnectionState::Unavailable {
        safe_code: "safe-code",
    };
    assert!(hint(&model, 100).contains("Durable Session truth unavailable"));
    assert!(hint(&model, 40).starts_with("/reconnect try again safely"));
}

#[test]
fn title_and_linear_inspector_share_attempt_and_consequence_truth() {
    let mut model = AppModel {
        connection: ConnectionState::Disconnected { attempt: 2 },
        overlay: Some(Overlay::Inspector),
        ..Default::default()
    };
    model.inspector.open = true;
    model.select_inspector_variant(InspectorVariant::Recovery);

    assert!(view::terminal_title(&model).contains("Disconnected 2/5"));
    let disconnected = view::linear_overlay(&model);
    assert!(disconnected.contains("Updates paused · attempt 2/5"));
    assert!(disconnected.contains("Durable Turn state may be newer"));
    assert!(disconnected.contains("Enter to resume events safely"));

    model.connection = ConnectionState::Reconnecting { attempt: 3 };
    assert!(view::terminal_title(&model).contains("Reconnecting 3/5"));
    let reconnecting = view::linear_overlay(&model);
    assert!(reconnecting.contains("Reconnecting · attempt 3/5"));
    assert!(reconnecting.contains("Updates remain paused"));
    assert!(reconnecting.contains("/status for details"));
    assert!(!reconnecting.contains("Enter to reconnect"));

    model.connection = ConnectionState::Unavailable {
        safe_code: "safe-code-canary",
    };
    let unavailable = view::linear_overlay(&model);
    assert!(unavailable.contains("Durable Session truth cannot be loaded"));
    assert!(unavailable.contains("Enter to try /reconnect safely"));
    assert!(!unavailable.contains("safe-code-canary"));
    assert!(!unavailable.contains("attempt"));
}

fn hint(model: &AppModel, width: u16) -> String {
    let height = 12;
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    let _ = view::render_cached(
        model,
        Theme::Mono,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    );
    let hint = view::test_hint_area(model, area);
    (hint.x..hint.right())
        .map(|column| buffer[(column, hint.y)].symbol())
        .collect::<String>()
        .trim()
        .to_owned()
}
