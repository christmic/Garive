#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::{MouseMode, Theme};
#[path = "../src/input/commands.rs"]
mod commands;

use commands::{
    command_matches, parse_command, Command, CommandContext, CommandParse, COMMAND_PALETTE,
};

#[test]
fn non_commands_remain_host_text_and_known_commands_are_exact() {
    assert_eq!(parse_command("hello /help"), CommandParse::NotCommand);
    assert_eq!(parse_command(" /help"), CommandParse::Valid(Command::Help));
    assert_eq!(
        parse_command("/new \"definition one\""),
        CommandParse::Valid(Command::New {
            definition: Some("definition one".into())
        })
    );
    assert_eq!(
        parse_command("/theme mono"),
        CommandParse::Valid(Command::Theme(Theme::Mono))
    );
    assert_eq!(
        parse_command("/jump \"release blocker\""),
        CommandParse::Valid(Command::Jump {
            filter: Some("release blocker".into())
        })
    );
}

#[test]
fn malformed_or_ambiguous_commands_never_fall_through_to_host() {
    for value in [
        "/unknown",
        "/help extra",
        "/new a b",
        "/theme blue",
        "/copy",
        "/new \"unterminated",
        "/new \"bad\\n\"",
        "/help\nsecond line",
    ] {
        assert_eq!(parse_command(value), CommandParse::Invalid, "{value}");
    }
}

#[test]
fn palette_search_matches_all_terms_across_name_and_help() {
    assert!(command_matches(
        "/copy last",
        "Copy last completion",
        "copy completion"
    ));
    assert!(command_matches("/status", "Connection details", "STATUS"));
    assert!(!command_matches(
        "/copy session-id",
        "Copy Session ID",
        "completion"
    ));
}

#[test]
fn every_catalog_entry_is_parseable_and_parser_variants_are_discoverable() {
    for command in COMMAND_PALETTE {
        assert!(
            matches!(parse_command(command.input), CommandParse::Valid(_)),
            "catalog entry must parse: {}",
            command.input
        );
    }
    for command in [
        "/theme system",
        "/theme dark",
        "/theme light",
        "/theme mono",
        "/mouse on",
        "/mouse off",
    ] {
        assert!(
            COMMAND_PALETTE.iter().any(|entry| entry.input == command),
            "parser variant must be discoverable: {command}"
        );
    }
}

#[test]
fn palette_requirements_explain_every_contextual_command() {
    let empty = CommandContext::default();
    let cases = [
        ("/new", "no Agent is installed"),
        ("/retry", "no pending command"),
        ("/cancel", "no Turn is running"),
        ("/copy last", "no completion is visible"),
        ("/copy selection", "no composer text is selected"),
        ("/copy session-id", "no Session is selected"),
        ("/jump", "fewer than two Turns are loaded"),
        ("/edit-prompt", "the draft is frozen"),
    ];
    for (input, expected) in cases {
        let command = COMMAND_PALETTE
            .iter()
            .find(|command| command.input == input)
            .unwrap();
        assert_eq!(command.unavailable_reason(empty), Some(expected), "{input}");
    }

    let ready = CommandContext {
        has_installed_agent: true,
        has_pending_command: true,
        has_running_turn: true,
        has_visible_completion: true,
        has_selected_session: true,
        has_navigable_turns: true,
        has_composer_selection: true,
        composer_is_editable: true,
    };
    assert!(COMMAND_PALETTE
        .iter()
        .all(|command| command.unavailable_reason(ready).is_none()));
}
