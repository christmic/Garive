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
    reduce, AppAction, AppModel, BootState, ConversationLandmark, ExecutionState, FocusTarget,
    InspectorVariant, Overlay, TimelineItem, TimelineRole,
};
use garive_host_client::{SessionSummary, SuspensionView};
use ratatui::{buffer::Buffer, layout::Rect, style::Modifier};

fn frame(model: &AppModel, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    let _ = view::render_cached(
        model,
        Theme::Mono,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    );
    (0..height)
        .map(|y| {
            let line: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn minimum_and_compact_frames_are_truthful() {
    let model = AppModel::default();
    assert!(frame(&model, 19, 7).contains("20×8"));
    let compact = frame(&model, 60, 12);
    assert!(compact.contains("connecting"));
    assert!(compact.contains("Ask Garive anything"));
    assert!(!compact.contains("Connecting to your durable workspace"));
    assert!(!compact.contains("Enter send"));
    assert!(!compact.contains("Sessions ("));
}

#[test]
fn standard_frame_has_conversation_context_and_safe_text() {
    let mut model = AppModel {
        boot: BootState::Ready,
        session_count: 3,
        selected_session: Some("session-1234567890".into()),
        ..Default::default()
    };
    model.push_test_timeline_item(TimelineItem {
        stable_key: "answer".into(),
        position: 7,
        role: TimelineRole::Agent,
        tone: Default::default(),
        text: "answer\u{1b}[31m\u{2066}x\u{2069}".into(),
    });
    let standard = frame(&model, 120, 18);
    assert!(standard.contains("• "));
    assert!(standard.contains("Current session"));
    assert!(!standard.contains("Sessions 3"));
    assert!(standard.contains("answer�[31m⟦LRI⟧x⟦PDI⟧"));
    assert!(!standard.contains('\u{1b}'));
}

#[test]
fn overlay_is_rendered_above_without_mutating_model() {
    let model = AppModel {
        overlay: Some(Overlay::QuitConfirmation),
        ..Default::default()
    };
    let before = format!("{model:?}");
    let rendered = frame(&model, 80, 16);
    assert!(rendered.contains("Quit Garive?"));
    assert_eq!(format!("{model:?}"), before);
}

#[test]
fn linear_inspector_uses_the_same_safe_projection_and_activation_guidance() {
    let mut model = AppModel::default();
    model.push_test_timeline_item(TimelineItem {
        stable_key: "turn".into(),
        position: 6,
        role: TimelineRole::User,
        tone: Default::default(),
        text: "Inspect".into(),
    });
    model.push_test_timeline_item(TimelineItem {
        stable_key: "private-id-canary".into(),
        position: 7,
        role: TimelineRole::Status,
        tone: Default::default(),
        text: "Checked public state".into(),
    });
    model.open_inspector(InspectorVariant::Activity);
    model.overlay = Some(Overlay::Inspector);
    let linear = view::linear_overlay(&model);
    assert!(linear.contains("Inspector, Activity"));
    assert!(linear.contains("Checked public state"));
    assert!(linear.contains("Enter to jump"));
    assert!(!linear.contains("private-id-canary"));
}

#[test]
fn recovery_overlay_shares_truthful_copy_and_actions_across_presentations() {
    let model = AppModel {
        overlay: Some(Overlay::UnknownCommand),
        notice: Some("A prior command has an unknown durable outcome.".into()),
        ..Default::default()
    };
    let visual = frame(&model, 100, 24);
    let linear = view::linear_overlay(&model);
    for expected in [
        "Command result unknown",
        "A prior command has an unknown durable outcome.",
        "exact retry",
        "abandon local record",
    ] {
        assert!(visual.contains(expected), "visual missed {expected}");
        assert!(linear.contains(expected), "linear missed {expected}");
    }
    assert!(!visual.contains("Unknown command"));
}

#[test]
fn action_overlay_geometry_preserves_actions_after_wrapped_multiline_details() {
    let model = AppModel {
        overlay: Some(Overlay::ErrorDetails),
        notice: Some("Host: online\nSession: durable-session\nCursor: 42".into()),
        ..Default::default()
    };
    let compact = frame(&model, 40, 16);
    assert!(compact.contains("Host: online"));
    assert!(compact.contains("Session: durable-session"));
    assert!(compact.contains("Esc close"));
}

#[test]
fn modal_geometry_never_erases_a_grown_composer() {
    let mut model = AppModel {
        overlay: Some(Overlay::Help),
        ..Default::default()
    };
    model
        .composer
        .replace("one\ntwo\nthree\nfour\ncomposer-owned-tail")
        .unwrap();
    let rendered = frame(&model, 100, 16);
    assert!(rendered.contains("Keyboard guide"));
    assert!(rendered.contains("composer-owned-tail"));
}

#[test]
fn suspension_is_a_structured_action_card_not_raw_json() {
    let model = AppModel {
        overlay: Some(Overlay::Suspension),
        suspension: Some(SuspensionView {
            suspension_id: "suspension-1".into(),
            session_version: 2,
            kind: "approval_required".into(),
            prompt_schema: "garive.public-suspension-prompt.v1".into(),
            prompt_json: r#"{"schema_version":1,"title_key":"approval.title","message_text":"Create one file.","action_label_key":"approval.allow"}"#.into(),
            prompt_digest: "0".repeat(64),
            response_schema_json: Some(r#"{"type":"boolean"}"#.into()),
            response_schema_digest: Some("1".repeat(64)),
        }),
        ..Default::default()
    };
    let rendered = frame(&model, 100, 24);
    assert!(rendered.contains("Approval required"));
    assert!(rendered.contains("Create one file."));
    assert!(rendered.contains("Enter true or false."));
    assert!(!rendered.contains("title_key"));
}

#[test]
fn decision_sheet_is_read_only_for_noninteractive_and_unsupported_suspensions() {
    for (kind, schema) in [
        ("resource_unavailable", r#"{"type":"boolean"}"#),
        ("approval_required", r#"{"type":"null"}"#),
    ] {
        let model = AppModel {
            overlay: Some(Overlay::Suspension),
            suspension: Some(SuspensionView {
                suspension_id: "s".into(),
                session_version: 2,
                kind: kind.into(),
                prompt_schema: "garive.public-suspension-prompt.v1".into(),
                prompt_json: r#"{"schema_version":1,"title_key":"title","message_text":"Wait safely.","action_label_key":"action"}"#.into(),
                prompt_digest: "0".repeat(64),
                response_schema_json: Some(schema.into()),
                response_schema_digest: Some("1".repeat(64)),
            }),
            ..Default::default()
        };
        let visual = frame(&model, 100, 24);
        let linear = view::linear_overlay(&model);
        assert!(visual.contains("Read only"));
        assert!(linear.contains("Read only"));
        assert!(!visual.contains("Enter submit response"));
    }
}

#[test]
fn suspension_response_identity_preserves_only_the_exact_schema_bound_editor() {
    let mut model = AppModel {
        selected_session: Some("session".into()),
        selected_turn: Some("turn".into()),
        suspension: Some(SuspensionView {
            suspension_id: "s".into(),
            session_version: 2,
            kind: "approval_required".into(),
            prompt_schema: "garive.public-suspension-prompt.v1".into(),
            prompt_json: "{}".into(),
            prompt_digest: "0".repeat(64),
            response_schema_json: Some(r#"{"type":"boolean"}"#.into()),
            response_schema_digest: Some("1".repeat(64)),
        }),
        ..Default::default()
    };
    model.reconcile_suspension_response();
    model
        .suspension_response
        .as_mut()
        .unwrap()
        .editor
        .insert("true")
        .unwrap();
    model.reconcile_suspension_response();
    let response = model.suspension_response.as_ref().unwrap();
    assert_eq!(response.editor.text(), "true");
    model.suspension.as_mut().unwrap().response_schema_digest = Some("2".repeat(64));
    model.reconcile_suspension_response();
    let response = model.suspension_response.as_ref().unwrap();
    assert_eq!(response.editor.text(), "");
}

#[test]
fn boolean_suspension_projects_one_shared_keyboard_and_screen_reader_choice() {
    let mut model = AppModel {
        terminal_size: application::TerminalSize {
            width: 100,
            height: 24,
        },
        selected_session: Some("session".into()),
        selected_turn: Some("turn".into()),
        overlay: Some(Overlay::Suspension),
        suspension: Some(SuspensionView {
            suspension_id: "s".into(),
            session_version: 2,
            kind: "approval_required".into(),
            prompt_schema: "garive.public-suspension-prompt.v1".into(),
            prompt_json: r#"{"schema_version":1,"title_key":"title","message_text":"Continue?","action_label_key":"allow"}"#.into(),
            prompt_digest: "0".repeat(64),
            response_schema_json: Some(r#"{"type":"boolean"}"#.into()),
            response_schema_digest: Some("1".repeat(64)),
        }),
        ..Default::default()
    };
    model.reconcile_suspension_response();
    let first = frame(&model, 40, 8);
    assert!(first.contains("› true · 1/2 ↑↓"));
    model.suspension_response.as_mut().unwrap().choice_selection = 1;
    let visual = frame(&model, 100, 24);
    let linear = view::linear_overlay(&model);
    assert!(visual.contains("› false"));
    assert!(visual.contains("Enter submit response"));
    assert!(linear.contains("Selected: false"));
    assert!(linear.contains("Use Up or Down to select"));
    let short = frame(&model, 40, 8);
    assert!(short.contains("› false"));
    assert!(short.contains("2/2 ↑↓"));
    assert!(short.contains("Enter submit response"));

    let choice_row = short
        .lines()
        .position(|line| line.contains("› false"))
        .expect("selected compact choice row");
    model.terminal_size = application::TerminalSize {
        width: 40,
        height: 8,
    };
    assert_eq!(
        view::decision_choice_hit_test(&model, 5, choice_row as u16),
        Some(1)
    );
    assert_eq!(
        view::decision_choice_hit_test(&model, 5, choice_row.saturating_sub(1) as u16),
        None
    );
}

#[test]
fn scalar_suspension_caret_tracks_the_independent_editor_cursor() {
    let mut model = AppModel {
        selected_session: Some("session".into()),
        selected_turn: Some("turn".into()),
        overlay: Some(Overlay::Suspension),
        suspension: Some(SuspensionView {
            suspension_id: "s".into(),
            session_version: 2,
            kind: "external_input_required".into(),
            prompt_schema: "garive.public-suspension-prompt.v1".into(),
            prompt_json: r#"{"schema_version":1,"title_key":"title","message_text":"Name?","action_label_key":"send"}"#.into(),
            prompt_digest: "0".repeat(64),
            response_schema_json: Some(r#"{"type":"string","maxLength":20}"#.into()),
            response_schema_digest: Some("1".repeat(64)),
        }),
        ..Default::default()
    };
    model.reconcile_suspension_response();
    let editor = &mut model.suspension_response.as_mut().unwrap().editor;
    editor.insert("abcd").unwrap();
    editor.place_cursor(2, false);
    assert!(frame(&model, 52, 12).contains("ab▏cd"));
    let editor = &mut model.suspension_response.as_mut().unwrap().editor;
    editor.replace(&"界".repeat(30)).unwrap();
    editor.place_cursor(15, false);
    let narrow = frame(&model, 40, 12);
    assert!(narrow.contains("▏"));
    assert!(narrow.matches('界').count() >= 4);
}

#[test]
fn resolved_suspension_cannot_return_from_quit_to_a_stale_sheet() {
    let mut model = AppModel {
        overlay: Some(Overlay::QuitConfirmation),
        return_overlay: Some(Overlay::Suspension),
        ..Default::default()
    };
    model.reconcile_suspension_response();
    reduce(&mut model, AppAction::OverlayClosed);
    assert_eq!(model.overlay, None);
    assert_ne!(model.focus, FocusTarget::Overlay);
}

#[test]
fn composition_bottom_panes_preserve_workspace_axis_and_selection() {
    let area = Rect::new(0, 0, 100, 24);
    for (overlay, title) in [
        (Overlay::CommandPalette, "Command palette"),
        (Overlay::SessionPicker, "Switch session"),
        (Overlay::TurnNavigator, "Jump to a Turn"),
        (Overlay::PromptHistory, "Prompt history"),
    ] {
        let mut model = AppModel {
            overlay: Some(overlay),
            prompt_history: vec!["review the release".into()],
            conversation_landmarks: vec![ConversationLandmark {
                ordinal: 1,
                started_position: 1,
                prompt_preview: "review the release".into(),
            }],
            sessions: vec![session("session-visible-000001", "agent")],
            ..Default::default()
        };
        model.terminal_size = application::TerminalSize {
            width: area.width,
            height: area.height,
        };
        let mut buffer = Buffer::empty(area);
        let _ = view::render_cached(
            &model,
            Theme::Dark,
            area,
            &mut buffer,
            &mut view::RenderCache::default(),
        );
        assert!(!buffer[(0, 0)].modifier.contains(Modifier::DIM));
        let rendered = frame(&model, area.width, area.height);
        let title_column = rendered
            .lines()
            .find_map(|line| line.find(title))
            .expect("bottom pane title");
        assert!(
            title_column <= 1,
            "{title} drifted to column {title_column}"
        );
        let selected = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .find(|&(x, y)| buffer[(x, y)].symbol() == "›")
            .expect("selected row marker");
        assert_eq!(selected.0, 0, "{title} selection marker drifted");
    }
}

#[test]
fn only_the_composer_owns_the_terminal_cursor() {
    let mut model = AppModel::default();
    let area = Rect::new(0, 0, 100, 24);
    let mut buffer = Buffer::empty(area);
    assert!(view::render_cached(
        &model,
        Theme::Dark,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    )
    .is_some());

    model.focus = FocusTarget::Conversation;
    let mut buffer = Buffer::empty(area);
    assert!(view::render_cached(
        &model,
        Theme::Dark,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    )
    .is_none());

    model.focus = FocusTarget::Composer;
    model.composer_is_frozen = true;
    let mut buffer = Buffer::empty(area);
    assert!(view::render_cached(
        &model,
        Theme::Dark,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    )
    .is_none());
}

#[test]
fn frozen_composer_is_read_only_in_visual_cursor_and_pointer_surfaces() {
    let mut model = AppModel {
        composer_is_frozen: true,
        focus: FocusTarget::Composer,
        terminal_size: application::TerminalSize {
            width: 100,
            height: 24,
        },
        ..Default::default()
    };
    model.composer.replace("/theme ").unwrap();

    let rendered = frame(&model, 100, 24);
    assert!(rendered.contains("Draft locked"));
    assert!(rendered.contains("read only"));
    assert!(rendered.contains("Waiting for durable command truth"));
    assert!(!rendered.contains("Commands"));
    assert_eq!(view::composer_hit_test(&model, 4, 21, false), None);
    assert_eq!(view::command_suggestion_hit_test(&model, 4, 18), None);

    model.composer.clear();
    let empty = frame(&model, 40, 12);
    assert!(empty.contains("Draft retained"));
    assert!(!empty.contains("/ for commands"));
}

#[test]
fn screen_reader_names_frozen_running_and_ready_composer_contracts() {
    let mut model = AppModel {
        composer_is_frozen: true,
        ..Default::default()
    };
    assert_eq!(
        view::linear_composer_status(&model),
        "Composer locked. Draft retained. Editing is unavailable until durable command truth."
    );
    model.composer_is_frozen = false;
    model.execution = ExecutionState::Following;
    assert_eq!(
        view::linear_composer_status(&model),
        "Current Turn running. Draft retained. Enter reports the boundary; Escape requests cancellation."
    );
    model.overlay = Some(Overlay::Help);
    assert_eq!(
        view::linear_composer_status(&model),
        "Current Turn running. Draft retained. The active overlay owns input."
    );
    assert!(view::linear_overlay(&model).contains("Escape: close guide."));
    assert!(!view::linear_overlay(&model).contains("cancel the running Turn"));
    model.overlay = None;
    model.execution = ExecutionState::Idle;
    assert_eq!(
        view::linear_composer_status(&model),
        "Composer ready. Editing is available."
    );
}

#[test]
fn composer_selection_is_visible_by_grapheme_without_color() {
    let mut model = AppModel::default();
    model.composer.replace("a界b").unwrap();
    model.composer.move_left(true);
    model.composer.move_left(true);
    let area = Rect::new(0, 0, 100, 24);
    let mut buffer = Buffer::empty(area);
    let _ = view::render_cached(
        &model,
        Theme::Mono,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    );
    let selected = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .find(|&(x, y)| buffer[(x, y)].symbol() == "界")
        .expect("selected CJK composer grapheme");
    assert!(!buffer[(selected.0 - 1, selected.1)]
        .modifier
        .contains(Modifier::REVERSED));
    assert!(buffer[selected].modifier.contains(Modifier::REVERSED));
    assert!(buffer[(selected.0 + 2, selected.1)]
        .modifier
        .contains(Modifier::REVERSED));
    assert!(frame(&model, 100, 24).contains("Alt+C copy"));
}

#[test]
fn searchable_overlays_show_only_matching_rows() {
    let mut model = AppModel {
        overlay: Some(Overlay::CommandPalette),
        command_filter: "copy completion".into(),
        ..Default::default()
    };
    let palette = frame(&model, 100, 24);
    assert!(palette.contains("/copy last"));
    assert!(!palette.contains("/status"));

    model.overlay = Some(Overlay::PromptHistory);
    model.history_filter = "deploy".into();
    model.prompt_history = vec!["deploy release".into(), "write tests".into()];
    let history = frame(&model, 100, 24);
    assert!(history.contains("deploy release"));
    assert!(!history.contains("write tests"));
}

#[test]
fn turn_navigator_is_bounded_filtered_safe_and_matches_linear_presentation() {
    let model = AppModel {
        overlay: Some(Overlay::TurnNavigator),
        turn_filter: "beta".into(),
        conversation_landmarks: vec![
            ConversationLandmark {
                ordinal: 1,
                started_position: 10,
                prompt_preview: "alpha hidden".into(),
            },
            ConversationLandmark {
                ordinal: 2,
                started_position: 20,
                prompt_preview: "beta \u{1b}[31m safe".into(),
            },
        ],
        ..Default::default()
    };
    let visual = frame(&model, 100, 24);
    assert!(visual.contains("Jump to a Turn"));
    assert!(visual.contains("Search  beta"));
    assert!(visual.contains("2  beta �[31m safe"));
    assert!(!visual.contains("alpha hidden"));
    assert!(!visual.contains("started_position"));
    assert!(!visual.contains('\u{1b}'));

    let linear = view::linear_overlay(&model);
    assert!(linear.contains("Turn 2. beta �[31m safe"));
    assert!(!linear.contains("alpha hidden"));
}

#[test]
fn inline_command_suggestions_are_prefix_scoped_and_dismissible() {
    let mut model = AppModel {
        focus: FocusTarget::Composer,
        terminal_size: application::TerminalSize {
            width: 100,
            height: 24,
        },
        ..Default::default()
    };
    model.composer.replace("/theme ").unwrap();
    let matches = model.matching_command_suggestion_indices();
    assert_eq!(matches.len(), 4);
    assert!(matches
        .iter()
        .all(|index| input::COMMAND_PALETTE[*index].input.starts_with("/theme ")));
    assert!(model.command_suggestions_active());

    model.command_suggestion_dismissed = Some("/theme ".into());
    assert!(!model.command_suggestions_active());
    model.composer.replace("/theme d").unwrap();
    assert!(model.command_suggestions_active());

    model.composer.replace("explain /theme").unwrap();
    assert!(!model.command_suggestions_active());
    model.composer.replace("/new agent-id").unwrap();
    assert!(!model.command_suggestions_active());
}

#[test]
fn inline_command_suggestions_render_above_composer_without_a_modal_backdrop() {
    let mut model = AppModel {
        focus: FocusTarget::Composer,
        terminal_size: application::TerminalSize {
            width: 100,
            height: 24,
        },
        ..Default::default()
    };
    model.composer.replace("/theme ").unwrap();
    let rendered = frame(&model, 100, 24);
    assert!(rendered.contains("Commands"));
    assert!(rendered.contains("/theme dark"));
    assert!(rendered.contains("/theme light"));
    assert!(rendered.contains("Tab complete"));
    assert!(!rendered.contains("↑/↓ select"));
    assert!(!rendered.contains("Esc close"));
    assert!(!rendered.contains("Enter send"));
    assert!(!rendered.contains("Search"));

    model.terminal_size = application::TerminalSize {
        width: 40,
        height: 12,
    };
    model.composer.replace("/theme d").unwrap();
    let compact = frame(&model, 40, 12);
    assert!(compact.contains("Tab complete"));
    assert!(!compact.contains("Esc close"));
    assert!(!compact.contains("↑/↓ select"));
}

#[test]
fn compact_list_overlays_keep_their_selection_visible() {
    let quit = input::COMMAND_PALETTE
        .iter()
        .position(|command| command.input == "/quit")
        .unwrap();
    let mut model = AppModel {
        overlay: Some(Overlay::CommandPalette),
        command_selection: quit,
        ..Default::default()
    };
    let palette = frame(&model, 100, 12);
    assert!(palette.contains("› /quit"));
    assert!(!palette.contains("/new"));

    model.overlay = Some(Overlay::PromptHistory);
    model.prompt_history = (0..20).map(|index| format!("prompt {index:02}")).collect();
    model.history_selection = 19;
    let history = frame(&model, 100, 12);
    assert!(history.contains("› prompt 19"));
    assert!(!history.contains("prompt 00"));
}

#[test]
fn linear_overlays_share_filtered_results_and_selection_windows() {
    let mut model = AppModel {
        overlay: Some(Overlay::CommandPalette),
        command_filter: "copy completion".into(),
        ..Default::default()
    };
    let commands = view::linear_overlay(&model);
    assert!(commands.contains("Selected 1 of 1: /copy last. Copy last completion"));
    assert!(commands.contains("Unavailable: no completion is visible"));
    assert!(!commands.contains("/status"));

    model.overlay = Some(Overlay::PromptHistory);
    model.history_filter.clear();
    model.prompt_history = (0..20).map(|index| format!("prompt {index:02}")).collect();
    model.history_selection = 19;
    let history = view::linear_overlay(&model);
    assert!(history.contains("> 20. prompt 19"));
    assert!(!history.contains("prompt 00"));

    model.overlay = Some(Overlay::SessionPicker);
    model.session_filter = "needle-agent".into();
    model.sessions = vec![
        session("session-hidden-000000", "other-agent"),
        session("session-visible-000001", "needle-agent"),
    ];
    model.session_selection = 0;
    let sessions = view::linear_overlay(&model);
    assert!(sessions.contains("> 1. Session 2, Agent"));
    assert!(!sessions.contains("Session 2, needle-agent"));
    assert!(!sessions.contains("000001"));
    assert!(!sessions.contains("other-agent"));
}

#[test]
fn compact_command_palette_keeps_visual_and_accessible_windows_in_lockstep() {
    let mut model = AppModel {
        overlay: Some(Overlay::CommandPalette),
        command_selection: input::COMMAND_PALETTE.len() - 1,
        terminal_size: application::TerminalSize {
            width: 40,
            height: 8,
        },
        ..Default::default()
    };
    let visual = frame(&model, 40, 8);
    let linear = view::linear_overlay(&model);
    assert!(visual.contains("Search  type to search"));
    assert!(visual.contains("Showing 19–21 / 21 · ↑18"));
    assert!(visual.contains("› /quit"));
    assert!(visual.contains("Enter run"));
    assert!(visual.contains("Esc close"));
    assert!(linear.contains("Showing commands 19 through 21 of 21"));
    assert!(linear.contains("Selected 21 of 21: /quit. Exit safely."));

    model.command_filter = "not present anywhere".into();
    model.command_selection = 0;
    let empty_visual = frame(&model, 40, 8);
    let empty_linear = view::linear_overlay(&model);
    assert!(empty_visual.contains("No matching commands"));
    assert!(empty_linear.contains("0 matching commands"));
}

#[test]
fn command_palette_shares_safe_unavailable_reasons_with_activation_context() {
    let mut model = AppModel {
        overlay: Some(Overlay::CommandPalette),
        command_filter: "copy last".into(),
        ..Default::default()
    };
    let unavailable = frame(&model, 120, 24);
    assert!(unavailable.contains("no completion is visible"));
    assert!(view::linear_overlay(&model).contains("Unavailable: no completion is visible"));

    model.push_test_timeline_item(TimelineItem {
        stable_key: "completion".into(),
        position: 1,
        role: TimelineRole::Agent,
        tone: Default::default(),
        text: "ready".into(),
    });
    assert!(!frame(&model, 120, 24).contains("no completion is visible"));
    assert!(!view::linear_overlay(&model).contains("Unavailable:"));
}

#[test]
fn session_picker_filter_and_selection_share_one_visible_result_set() {
    let mut model = AppModel {
        overlay: Some(Overlay::SessionPicker),
        session_filter: "needle-agent".into(),
        sessions: vec![
            session("session-hidden-000000", "other-agent"),
            session("session-visible-000001", "needle-agent"),
        ],
        ..Default::default()
    };
    let filtered = frame(&model, 80, 24);
    assert!(filtered.contains("Agent"));
    assert!(!filtered.contains("Session 2 · needle-agent"));
    assert!(filtered.contains("Session 2"));
    assert!(!filtered.contains("000001"));
    assert!(!filtered.contains("000000"));

    model.session_filter.clear();
    model.sessions = (0..12)
        .map(|index| session(&format!("session-{index:06}"), &format!("agent-{index:06}")))
        .collect();
    model.session_selection = 11;
    let scrolled = frame(&model, 80, 24);
    assert!(scrolled.contains("› Session 12 · Agent"));
    assert!(!scrolled.contains("agent-000011"));
    assert!(!scrolled.contains("agent-000000"));
}

#[test]
fn unicode_filtered_lists_keep_visual_mouse_and_linear_selection_in_lockstep() {
    let mut model = AppModel {
        overlay: Some(Overlay::PromptHistory),
        terminal_size: application::TerminalSize {
            width: 40,
            height: 8,
        },
        prompt_history: vec![
            "first".into(),
            format!("{} CJK提示 e\u{301}", "👨‍👩‍👧‍👦界".repeat(20)),
            "third".into(),
        ],
        history_selection: 1,
        ..Default::default()
    };

    let visual = frame(&model, 40, 8);
    assert!(visual.contains('›'), "{visual}");
    assert!(visual.contains("Enter restore"));
    assert!(visual.contains("Esc close"), "{visual}");
    let hit_rows = (0..8)
        .filter(|row| (0..40).any(|column| view::overlay_hit_test(&model, column, *row) == Some(1)))
        .collect::<Vec<_>>();
    assert_eq!(
        hit_rows.len(),
        1,
        "a grapheme-rich item must own exactly one hit row"
    );
    let linear = view::linear_overlay(&model);
    assert!(linear.contains("> 2. 👨‍👩‍👧‍👦界"));

    model.overlay = Some(Overlay::SessionPicker);
    model.sessions = vec![
        session("session-0", "other"),
        session("session-1", &"会话🦀".repeat(20)),
    ];
    model.session_selection = 1;
    let sessions = frame(&model, 40, 8);
    assert!(sessions.contains("› Session 2"), "{sessions}");
    assert!(sessions.contains("Enter open"));
    let hit_rows = (0..8)
        .filter(|row| (0..40).any(|column| view::overlay_hit_test(&model, column, *row) == Some(1)))
        .collect::<Vec<_>>();
    assert_eq!(
        hit_rows.len(),
        1,
        "a wide session label must not wrap its hit target"
    );
}

#[test]
fn agent_markdown_is_structured_and_terminal_safe() {
    let mut model = AppModel {
        boot: BootState::Ready,
        ..Default::default()
    };
    model.push_test_timeline_item(TimelineItem {
        stable_key: "markdown".into(),
        position: 1,
        role: TimelineRole::Agent,
        tone: Default::default(),
        text: "## Result\n\n- **done**\n- `cargo test`\n\n> <script>bad</script>\u{1b}[2J".into(),
    });

    let rendered = frame(&model, 100, 20);
    assert!(rendered.contains("## Result"));
    assert!(rendered.contains("• done"));
    assert!(rendered.contains("• cargo test"));
    assert!(rendered.contains("│ <script>bad</script>�[2J"));
    assert!(!rendered.contains('\u{1b}'));
}

#[test]
fn multiline_composer_keeps_the_cursor_inside_its_scrolled_viewport() {
    let mut model = AppModel::default();
    model
        .composer
        .replace("one\ntwo\nthree\nfour\nfive")
        .unwrap();
    let area = Rect::new(0, 0, 40, 12);
    let mut buffer = Buffer::empty(area);
    let cursor = view::render_cached(
        &model,
        Theme::Mono,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    )
    .unwrap();
    assert!(cursor.1 < area.height - 1);
    assert!(frame(&model, 40, 12).contains("five"));
}

fn session(id: &str, definition: &str) -> SessionSummary {
    SessionSummary {
        api_version: "v1".into(),
        session_id: id.into(),
        agent_instance_id: "agent-instance".into(),
        definition_id: definition.into(),
        definition_revision: "revision-1".into(),
        opened_at: "2026-08-30T00:00:00Z".into(),
        latest_position: 1,
        latest_turn_id: Some("turn-1".into()),
        latest_turn_state: Some("running".into()),
        turn_count: 1,
    }
}
