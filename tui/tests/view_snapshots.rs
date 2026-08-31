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
    AppModel, BootState, ConnectionState, ConversationLandmark, ExecutionState, Overlay,
    TimelineItem, TimelineRole, TimelineTone,
};
use garive_host_client::{AgentDefinitionSummary, SessionSummary, SuspensionView};
use ratatui::{buffer::Buffer, layout::Rect};
use unicode_width::UnicodeWidthStr;

#[test]
fn responsive_product_frames_match_reviewed_snapshots() {
    let model = product_model();
    insta::assert_snapshot!("compact_40x12", frame(&model, Theme::Mono, 40, 12));
    let mut wrapped = product_model();
    wrapped
        .composer
        .replace("Composer grows with soft wrapping and keeps every visible row.")
        .unwrap();
    insta::assert_snapshot!(
        "composer_soft_wrap_compact_mono_40x16",
        frame(&wrapped, Theme::Mono, 40, 16)
    );
    insta::assert_snapshot!("standard_100x24", frame(&model, Theme::Dark, 100, 24));
    let activities = activity_stack_model();
    insta::assert_snapshot!(
        "activity_stack_dark_100x24",
        frame(&activities, Theme::Dark, 100, 24)
    );
    insta::assert_snapshot!(
        "activity_stack_light_100x24",
        frame(&activities, Theme::Light, 100, 24)
    );
    insta::assert_snapshot!(
        "activity_stack_mono_100x24",
        frame(&activities, Theme::Mono, 100, 24)
    );
    insta::assert_snapshot!(
        "activity_stack_compact_mono_40x18",
        frame(&activities, Theme::Mono, 40, 18)
    );
    insta::assert_snapshot!(
        "motion_running_dark_100x24",
        motion_frame(&model, Theme::Dark, 4, 100, 24)
    );
    insta::assert_snapshot!(
        "motion_running_light_100x24",
        motion_frame(&model, Theme::Light, 4, 100, 24)
    );
    insta::assert_snapshot!(
        "motion_running_mono_100x24",
        motion_frame(&model, Theme::Mono, 4, 100, 24)
    );
    insta::assert_snapshot!("markdown_rich_dark", markdown_style_preview(Theme::Dark));
    insta::assert_snapshot!("markdown_rich_light", markdown_style_preview(Theme::Light));
    insta::assert_snapshot!("markdown_rich_mono", markdown_style_preview(Theme::Mono));
    insta::assert_snapshot!(
        "command_suggestions_dark_100x24",
        command_suggestion_frame(Theme::Dark)
    );
    insta::assert_snapshot!(
        "command_suggestions_light_100x24",
        command_suggestion_frame(Theme::Light)
    );
    insta::assert_snapshot!(
        "command_suggestions_mono_100x24",
        command_suggestion_frame(Theme::Mono)
    );
    insta::assert_snapshot!(
        "command_suggestions_compact_mono_40x12",
        command_suggestion_frame_at(Theme::Mono, 40, 12)
    );
    insta::assert_snapshot!(
        "composer_selection_dark",
        composer_selection_style_preview(Theme::Dark)
    );
    insta::assert_snapshot!(
        "composer_selection_light",
        composer_selection_style_preview(Theme::Light)
    );
    insta::assert_snapshot!(
        "composer_selection_mono",
        composer_selection_style_preview(Theme::Mono)
    );
    insta::assert_snapshot!(
        "markdown_table_narrow_dark",
        markdown_table_narrow_preview(Theme::Dark)
    );
    insta::assert_snapshot!(
        "markdown_table_narrow_light",
        markdown_table_narrow_preview(Theme::Light)
    );
    insta::assert_snapshot!(
        "markdown_table_narrow_mono",
        markdown_table_narrow_preview(Theme::Mono)
    );

    let mut wide = model;
    wide.overlay = Some(Overlay::CommandPalette);
    insta::assert_snapshot!("wide_palette_160x28", frame(&wide, Theme::Light, 160, 28));

    let mut help = product_model();
    help.overlay = Some(Overlay::Help);
    insta::assert_snapshot!("help_100x24", frame(&help, Theme::Dark, 100, 24));

    let turns = turn_navigator_model();
    insta::assert_snapshot!(
        "turn_navigator_dark_100x24",
        frame(&turns, Theme::Dark, 100, 24)
    );
    insta::assert_snapshot!(
        "turn_navigator_light_100x24",
        frame(&turns, Theme::Light, 100, 24)
    );
    insta::assert_snapshot!(
        "turn_navigator_mono_100x24",
        frame(&turns, Theme::Mono, 100, 24)
    );
    insta::assert_snapshot!(
        "turn_navigator_compact_mono_40x12",
        frame(&turns, Theme::Mono, 40, 12)
    );

    let mut recovery = product_model();
    recovery.overlay = Some(Overlay::UnknownCommand);
    recovery.notice = Some(
        "A prior command has an unknown durable outcome. Review Host truth before retrying.".into(),
    );
    insta::assert_snapshot!(
        "recovery_unknown_100x24",
        frame(&recovery, Theme::Dark, 100, 24)
    );

    let mut action = product_model();
    action.overlay = Some(Overlay::Suspension);
    action.suspension = Some(SuspensionView {
        suspension_id: "suspension-1".into(),
        session_version: 2,
        kind: "approval_required".into(),
        prompt_schema: "garive.public-suspension-prompt.v1".into(),
        prompt_json: r#"{"schema_version":1,"title_key":"approval.title","message_text":"Create one bounded local file.","action_label_key":"approval.allow"}"#.into(),
        prompt_digest: "0".repeat(64),
        response_schema_json: Some(r#"{"type":"boolean"}"#.into()),
        response_schema_digest: Some("1".repeat(64)),
    });
    insta::assert_snapshot!("action_100x24", frame(&action, Theme::Dark, 100, 24));

    let mut sessions = product_model();
    sessions.overlay = Some(Overlay::SessionPicker);
    sessions.sessions = (0..12)
        .map(|index| session(&format!("session-{index:06}"), "running", 1))
        .collect();
    sessions.session_count = 12;
    sessions.session_selection = 11;
    sessions.selected_session = Some("session-000011".into());
    insta::assert_snapshot!(
        "session_picker_scrolled_100x24",
        frame(&sessions, Theme::Mono, 100, 24)
    );
}

#[test]
fn responsive_column_boundaries_match_reviewed_snapshots() {
    let model = product_model();
    for width in [39, 40, 51, 52, 79, 80, 119, 120, 160] {
        let rendered = frame(&model, Theme::Mono, width, 18);
        assert_responsive_frame(&rendered, width, 18);
        insta::assert_snapshot!(format!("responsive_boundary_{width}x18"), rendered);
    }
}

fn assert_responsive_frame(rendered: &str, width: u16, height: u16) {
    let lines = rendered.split('\n').collect::<Vec<_>>();
    assert_eq!(lines.len(), usize::from(height));
    assert!(lines
        .iter()
        .all(|line| UnicodeWidthStr::width(*line) <= usize::from(width)));

    for forbidden in ["Sessions", "session-alpha", "#42", "Position"] {
        assert!(
            !rendered.contains(forbidden),
            "legacy rail detail {forbidden:?} leaked at {width} columns"
        );
    }
    if width < 40 {
        assert!(rendered.contains("Garive needs 40 columns"));
        assert!(rendered.contains("draft retained"));
        assert!(rendered.contains("Run continues · Esc cancel"));
        assert!(!rendered.contains("Summarize the release plan."));
        assert!(!rendered.contains('╭'));
        return;
    }
    for required in [
        "Session 1",
        "Summarize the release plan.",
        "Agent action · completed",
        "cargo test passes.",
        "Ask a follow-up…",
    ] {
        assert!(
            rendered.contains(required),
            "content {required:?} was clipped at {width} columns"
        );
    }

    let composer = lines
        .iter()
        .find(|line| line.contains('╭'))
        .expect("composer top border");
    let content_width = if width >= 80 { width.min(96) } else { width };
    let expected_x = width.saturating_sub(content_width) / 2;
    assert_eq!(
        composer.chars().take_while(|ch| *ch == ' ').count(),
        usize::from(expected_x)
    );
    assert_eq!(
        UnicodeWidthStr::width(*composer),
        usize::from(expected_x + content_width)
    );
}

#[test]
fn boundary_size_and_theme_matrix_never_panics() {
    let model = product_model();
    for theme in [Theme::System, Theme::Dark, Theme::Light, Theme::Mono] {
        for (width, height) in [
            (0, 0),
            (1, 1),
            (19, 7),
            (20, 8),
            (40, 8),
            (40, 12),
            (79, 23),
            (80, 24),
            (99, 24),
            (100, 24),
            (159, 28),
            (160, 28),
            (200, 40),
        ] {
            let rendered = frame(&model, theme, width, height);
            assert!(!rendered.contains('\u{1b}'));
        }
    }
}

fn product_model() -> AppModel {
    let mut model = AppModel {
        boot: BootState::Ready,
        definition_count: 1,
        definitions: vec![AgentDefinitionSummary {
            api_version: "v1".into(),
            definition_id: "research-agent".into(),
            definition_revision: "revision-1".into(),
            capabilities: vec!["web".into()],
        }],
        session_count: 2,
        sessions: vec![
            session("session-alpha-123456", "completed", 3),
            session("session-beta-987654", "running", 1),
        ],
        selected_session: Some("session-alpha-123456".into()),
        connection: ConnectionState::Online,
        execution: ExecutionState::Following,
        observed_position: 42,
        ..Default::default()
    };
    let mut activity = item(
        "activity",
        4,
        TimelineRole::Status,
        "Agent action · completed",
    );
    activity.tone = TimelineTone::Success;
    model.timeline = vec![
        item("user", 2, TimelineRole::User, "Summarize the release plan."),
        activity,
        item(
            "agent",
            6,
            TimelineRole::Agent,
            "## Release plan\n\n- Verify the Runtime\n- Ship with **evidence**\n\n`cargo test` passes.",
        ),
    ];
    model.composer.replace("Ask a follow-up…").unwrap();
    model
}

fn activity_stack_model() -> AppModel {
    let mut model = product_model();
    model.timeline = vec![
        item(
            "user",
            2,
            TimelineRole::User,
            "Verify the release candidate.",
        ),
        activity("read", 3, TimelineTone::Success, "Read project rules"),
        activity("tests", 4, TimelineTone::Success, "Checked focused tests"),
        activity("run", 5, TimelineTone::Active, "Running strict validation"),
    ];
    model
}

fn activity(key: &str, position: u64, tone: TimelineTone, text: &str) -> TimelineItem {
    let mut item = item(key, position, TimelineRole::Status, text);
    item.tone = tone;
    item
}

fn turn_navigator_model() -> AppModel {
    let mut model = product_model();
    model.overlay = Some(Overlay::TurnNavigator);
    model.turn_filter = "release".into();
    model.turn_selection = 2;
    model.conversation_landmarks = (0..12)
        .map(|index| ConversationLandmark {
            ordinal: index + 1,
            started_position: index as u64 * 3 + 1,
            prompt_preview: if index == 5 {
                "release 界面 keeps a display-width bounded preview across terminals".into()
            } else {
                format!("release checkpoint {index:02} with verified evidence")
            },
        })
        .collect();
    model
}

fn command_suggestion_frame(theme: Theme) -> String {
    command_suggestion_frame_at(theme, 100, 24)
}

fn command_suggestion_frame_at(theme: Theme, width: u16, height: u16) -> String {
    let mut model = product_model();
    model.execution = ExecutionState::Idle;
    model.terminal_size = application::TerminalSize { width, height };
    model.composer.replace("/theme ").unwrap();
    model.command_suggestion_selection = 1;
    frame(&model, theme, width, height)
}

fn composer_selection_style_preview(theme: Theme) -> String {
    let mut model = product_model();
    model.execution = ExecutionState::Idle;
    model.composer.replace("Selected tail").unwrap();
    model.composer.move_document_start(false);
    for _ in 0..8 {
        model.composer.move_right(true);
    }
    let area = Rect::new(0, 0, 40, 12);
    let mut buffer = Buffer::empty(area);
    let _ = view::render_cached(
        &model,
        theme,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    );
    let target = "Selected tail";
    let start = (0..area.height)
        .flat_map(|y| (0..=area.width - target.len() as u16).map(move |x| (x, y)))
        .find(|&(x, y)| {
            target.chars().enumerate().all(|(offset, character)| {
                buffer[(x + offset as u16, y)]
                    .symbol()
                    .chars()
                    .eq([character])
            })
        })
        .expect("composer selection start");
    let mut runs = Vec::<(
        String,
        ratatui::style::Color,
        ratatui::style::Color,
        ratatui::style::Modifier,
    )>::new();
    for column in start.0..start.0 + 13 {
        let cell = &buffer[(column, start.1)];
        if let Some((text, _, _, _)) = runs
            .last_mut()
            .filter(|run| run.1 == cell.fg && run.2 == cell.bg && run.3 == cell.modifier)
        {
            text.push_str(cell.symbol());
        } else {
            runs.push((cell.symbol().to_owned(), cell.fg, cell.bg, cell.modifier));
        }
    }
    runs.into_iter()
        .map(|(text, fg, bg, modifier)| format!("{text:?} <fg={fg:?} bg={bg:?} +{modifier:?}>"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn session(id: &str, state: &str, turns: u64) -> SessionSummary {
    SessionSummary {
        api_version: "v1".into(),
        session_id: id.into(),
        agent_instance_id: "agent-instance".into(),
        definition_id: "research-agent".into(),
        definition_revision: "revision-1".into(),
        opened_at: "2026-08-30T00:00:00Z".into(),
        latest_position: 42,
        latest_turn_id: Some("turn-1".into()),
        latest_turn_state: Some(state.into()),
        turn_count: turns,
    }
}

fn item(key: &str, position: u64, role: TimelineRole, text: &str) -> TimelineItem {
    TimelineItem {
        stable_key: key.into(),
        position,
        role,
        tone: Default::default(),
        text: text.into(),
    }
}

fn frame(model: &AppModel, theme: Theme, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    let _ = view::render_cached(
        model,
        theme,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    );
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn motion_frame(model: &AppModel, theme: Theme, tick: u64, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    let _ = view::render_cached_with_motion(
        model,
        theme,
        view::MotionFrame::animated(tick),
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    );
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn markdown_style_preview(theme: Theme) -> String {
    const SOURCE: &str = "# Delivery\n\n**outer *inner* tail**\n\n3. inspect\n4. ship\n\n[Guide](https://garive.local/guide)\n\n| Surface | State |\n|:--|--:|\n| macOS | **ready** |\n| Other | later |\n\n```rust\nfn main() {}\n```";
    markdown_runs(view::markdown_preview(SOURCE, theme))
}

fn markdown_table_narrow_preview(theme: Theme) -> String {
    const SOURCE: &str = "| Surface | State | Owner |\n|:--|--:|:--:|\n| macOS | **ready** | TUI |\n| Other | later | roadmap |";
    markdown_runs(view::markdown_preview_at_width(SOURCE, theme, 20))
}

fn markdown_runs(lines: Vec<ratatui::text::Line<'static>>) -> String {
    lines
        .into_iter()
        .map(|line| {
            line.spans
                .into_iter()
                .map(|span| {
                    format!(
                        "{:?} <fg={:?} bg={:?} +{:?} -{:?}>",
                        span.content,
                        span.style.fg,
                        span.style.bg,
                        span.style.add_modifier,
                        span.style.sub_modifier
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
