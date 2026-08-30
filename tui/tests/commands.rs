#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::{MouseMode, Theme};
#[path = "../src/input/commands.rs"]
mod commands;

use commands::{command_matches, parse_command, Command, CommandParse};

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
