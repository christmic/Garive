use std::path::PathBuf;

use garive_tui::{parse_launch_config, LaunchParseError, MouseMode, Theme};

#[test]
fn parses_the_complete_explicit_launch_contract() {
    let config = parse_launch_config([
        "garive-tui",
        "--host",
        "http://127.0.0.1:4317/",
        "--session",
        "session-1",
        "--definition",
        "definition-1",
        "--state-dir",
        "/tmp/garive-state",
        "--theme",
        "mono",
        "--screen-reader",
        "--reduced-motion",
        "--mouse",
        "off",
        "--ephemeral",
        "--no-prompt-history",
    ])
    .unwrap();

    assert_eq!(config.host, "http://127.0.0.1:4317/");
    assert_eq!(config.session.as_deref(), Some("session-1"));
    assert_eq!(config.definition.as_deref(), Some("definition-1"));
    assert_eq!(config.state_dir, Some(PathBuf::from("/tmp/garive-state")));
    assert_eq!(config.theme, Theme::Mono);
    assert!(config.theme_explicit);
    assert!(config.screen_reader);
    assert!(config.reduced_motion);
    assert!(config.reduced_motion_explicit);
    assert_eq!(config.mouse, MouseMode::Off);
    assert!(config.mouse_explicit);
    assert!(config.ephemeral);
    assert!(config.no_prompt_history);
}

#[test]
fn omitted_and_explicit_auto_values_remain_distinguishable() {
    let omitted = parse_launch_config(["garive-tui", "--host", "http://127.0.0.1:4317/"]).unwrap();
    assert_eq!(omitted.theme, Theme::System);
    assert!(!omitted.theme_explicit);
    assert_eq!(omitted.mouse, MouseMode::Auto);
    assert!(!omitted.mouse_explicit);

    let explicit = parse_launch_config([
        "garive-tui",
        "--host",
        "http://127.0.0.1:4317/",
        "--theme",
        "system",
        "--mouse",
        "auto",
    ])
    .unwrap();
    assert!(explicit.theme_explicit);
    assert!(explicit.mouse_explicit);
}

#[test]
fn rejects_legacy_positionals_duplicates_and_unsafe_locations() {
    let invalid = [
        vec!["garive-tui", "http://127.0.0.1:4317/", "agent", "hello"],
        vec![
            "garive-tui",
            "--host",
            "http://127.0.0.1:1/",
            "--host",
            "http://127.0.0.1:2/",
        ],
        vec!["garive-tui", "--host", "https://127.0.0.1:4317/"],
        vec!["garive-tui", "--host", "http://example.com:4317/"],
        vec![
            "garive-tui",
            "--host",
            "http://127.0.0.1:4317/",
            "--state-dir",
            "relative",
        ],
    ];

    for arguments in invalid {
        assert_eq!(
            parse_launch_config(arguments).unwrap_err(),
            LaunchParseError::InvalidArguments
        );
    }
}

#[test]
fn help_is_safe_and_does_not_require_a_host() {
    let LaunchParseError::Display(help) =
        parse_launch_config(["garive-tui", "--help"]).unwrap_err()
    else {
        panic!("help must be a display outcome");
    };
    assert!(help.contains("--screen-reader"));
    assert!(help.contains("--state-dir"));
    assert!(!help.contains("test-crash"));
}
