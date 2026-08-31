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
    AppModel, BootState, ConnectionState, ConversationLandmark, InspectorVariant, Overlay,
    TerminalSize,
};
use garive_host_client::SuspensionView;
use ratatui::{buffer::Buffer, layout::Rect};
use unicode_width::UnicodeWidthStr;

#[test]
fn narrow_overlay_visual_matrix_keeps_selection_and_actions_reachable() {
    let surfaces = surfaces();
    for theme in [Theme::Dark, Theme::Light, Theme::Mono] {
        for (name, model, required) in &surfaces {
            let rendered = frame(model, theme);
            assert_eq!(rendered.lines().count(), 8, "{name} {theme:?}");
            assert!(
                rendered
                    .lines()
                    .all(|line| line.width() <= usize::from(WIDTH)),
                "{name} {theme:?} exceeded 40 columns"
            );
            for expected in *required {
                assert!(
                    rendered.contains(expected),
                    "{name} {theme:?} hid {expected:?}\n{rendered}"
                );
            }
        }
    }

    insta::assert_snapshot!(
        "overlay_accessibility_mono_40x8",
        surfaces
            .iter()
            .map(|(name, model, _)| format!("{name}\n{}", frame(model, Theme::Mono)))
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

#[test]
fn linear_projection_names_the_same_selection_and_actions() {
    let turn = turn_navigator();
    let spoken = view::linear_overlay(&turn);
    assert!(spoken.contains("> 12. Turn 12. release checkpoint 11"));
    assert!(spoken.contains("Enter to jump"));
    assert!(spoken.contains("Escape to close"));

    let inspector = recovery_inspector();
    let spoken = view::linear_overlay(&inspector);
    assert!(spoken.contains("> 3. Reconnecting · attempt 3/5. Updates remain paused"));
    assert!(spoken.contains("/status for details"));
    assert!(!spoken.contains("Enter to reconnect"));
    assert!(spoken.contains("Escape to close"));

    let suspension = suspension();
    let spoken = view::linear_overlay(&suspension);
    assert!(spoken.contains("Selected: false"));
    assert!(spoken.contains("Press Enter to submit response"));
    assert!(spoken.contains("Press Control Q to leave safely"));

    let recovery = unknown_recovery();
    let spoken = view::linear_overlay(&recovery);
    assert!(spoken.contains("Press Enter to exact retry"));
    assert!(spoken.contains("Press A to abandon local record"));
}

const WIDTH: u16 = 40;
const HEIGHT: u16 = 8;

type Surface = (&'static str, AppModel, &'static [&'static str]);

fn surfaces() -> Vec<Surface> {
    vec![
        (
            "TURN NAVIGATOR",
            turn_navigator(),
            &["› 12", "Enter jump", "Esc close"],
        ),
        (
            "RECOVERY INSPECTOR",
            recovery_inspector(),
            &["› ! Reconnecting · attempt 3/5", "Esc close"],
        ),
        (
            "DECISION SHEET",
            suspension(),
            &["› false", "Enter submit response", "Ctrl+Q leave safely"],
        ),
        (
            "UNKNOWN RECOVERY",
            unknown_recovery(),
            &[
                "Command result unknown",
                "Enter exact retry",
                "A abandon local record",
            ],
        ),
    ]
}

fn base(overlay: Overlay) -> AppModel {
    AppModel {
        boot: BootState::Ready,
        overlay: Some(overlay),
        terminal_size: TerminalSize {
            width: WIDTH,
            height: HEIGHT,
        },
        ..Default::default()
    }
}

fn turn_navigator() -> AppModel {
    AppModel {
        turn_selection: 11,
        conversation_landmarks: (0..12)
            .map(|index| ConversationLandmark {
                ordinal: index + 1,
                started_position: index as u64 + 1,
                prompt_preview: format!("release checkpoint {index:02}"),
            })
            .collect(),
        ..base(Overlay::TurnNavigator)
    }
}

fn recovery_inspector() -> AppModel {
    let mut model = AppModel {
        connection: ConnectionState::Reconnecting { attempt: 3 },
        ..base(Overlay::Inspector)
    };
    model.pending_recovery.current_session = true;
    model.pending_recovery.other_session = true;
    model.inspector.open = true;
    model.select_inspector_variant(InspectorVariant::Recovery);
    model.select_inspector_index(2);
    model
}

fn suspension() -> AppModel {
    let mut model = AppModel {
        selected_session: Some("session".into()),
        selected_turn: Some("turn".into()),
        suspension: Some(SuspensionView {
            suspension_id: "suspension".into(),
            session_version: 1,
            kind: "approval_required".into(),
            prompt_schema: "garive.public-suspension-prompt.v1".into(),
            prompt_json: r#"{"schema_version":1,"title_key":"approval.title","message_text":"Review a bounded public request.","action_label_key":"approval.allow"}"#.into(),
            prompt_digest: "0".repeat(64),
            response_schema_json: Some(r#"{"type":"boolean"}"#.into()),
            response_schema_digest: Some("1".repeat(64)),
        }),
        ..base(Overlay::Suspension)
    };
    model.reconcile_suspension_response();
    model.suspension_response.as_mut().unwrap().choice_selection = 1;
    model
}

fn unknown_recovery() -> AppModel {
    AppModel {
        notice: Some(
            "Durable outcome is unknown; review Host truth before any exact replay.".into(),
        ),
        ..base(Overlay::UnknownCommand)
    }
}

fn frame(model: &AppModel, theme: Theme) -> String {
    let area = Rect::new(0, 0, WIDTH, HEIGHT);
    let mut buffer = Buffer::empty(area);
    let _ = view::render_cached(
        model,
        theme,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    );
    (0..HEIGHT)
        .map(|row| {
            (0..WIDTH)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
