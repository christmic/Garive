use std::{ffi::OsString, path::PathBuf};

use clap::{error::ErrorKind, Parser, ValueEnum};
use garive_host_client::{ClientLimits, LiveHostClient};
use serde::{Deserialize, Serialize};

const VALIDATION_LIMITS: ClientLimits = ClientLimits {
    max_command_bytes: 1,
    max_event_bytes: 1,
    max_events: 1,
    follow_deadline_ms: 1,
};

/// Terminal color preference supplied at launch.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Defer to terminal capabilities and local preferences.
    #[default]
    System,
    /// Use the dark-background palette.
    Dark,
    /// Use the light-background palette.
    Light,
    /// Use text attributes without semantic color dependence.
    Mono,
}

/// Mouse capture preference supplied at launch.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum MouseMode {
    /// Defer to terminal capabilities and local preferences.
    #[default]
    Auto,
    /// Enable mouse capture after terminal acquisition.
    On,
    /// Keep mouse capture disabled.
    Off,
}

/// Abrupt process boundaries available only to crash-recovery tests.
#[cfg(feature = "test-hooks")]
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum TestCrashHook {
    /// Panic after terminal modes are acquired so unwind restoration is exercised.
    TerminalAcquiredPanic,
    /// Pause after the exact pending command is durable and before Host I/O.
    PendingPersisted,
    /// Pause after a mutation response is validated and before pending removal.
    ResponseAccepted,
    /// Pause after pending removal and before convenience-state updates.
    PendingRemoved,
}

/// Validated configuration for one resident TUI process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchConfig {
    /// Explicit credential-free loopback Host root URL.
    pub host: String,
    /// Optional Session selected after boot.
    pub session: Option<String>,
    /// Optional preferred installed Agent definition.
    pub definition: Option<String>,
    /// Optional absolute local presentation-state directory.
    pub state_dir: Option<PathBuf>,
    /// Color preference for this process.
    pub theme: Theme,
    /// Whether `--theme` was present and therefore overrides saved state.
    pub theme_explicit: bool,
    /// Whether to use the linear accessible presentation.
    pub screen_reader: bool,
    /// Whether to suppress nonessential animation.
    pub reduced_motion: bool,
    /// Whether reduced motion was explicitly requested for this process.
    pub reduced_motion_explicit: bool,
    /// Mouse capture preference for this process.
    pub mouse: MouseMode,
    /// Whether `--mouse` was present and therefore overrides saved state.
    pub mouse_explicit: bool,
    /// Whether to disable all local presentation writes.
    pub ephemeral: bool,
    /// Whether to disable prompt-history writes.
    pub no_prompt_history: bool,
    /// Optional abrupt boundary used only by process crash-recovery tests.
    #[cfg(feature = "test-hooks")]
    pub test_crash_hook: Option<TestCrashHook>,
}

/// Non-launch outcome produced while parsing process arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchParseError {
    /// Safe help or version text that should be printed to stdout with exit zero.
    Display(String),
    /// Invalid arguments; the message deliberately excludes supplied values.
    InvalidArguments,
}

pub(crate) fn apply_terminal_environment(
    config: &mut LaunchConfig,
    term: Option<&str>,
    no_color: bool,
) {
    if !config.theme_explicit && no_color {
        config.theme = Theme::Mono;
    }
    if term.is_some_and(|value| value.eq_ignore_ascii_case("dumb")) {
        config.screen_reader = true;
    }
}

pub(crate) fn mouse_capture_enabled(
    mode: MouseMode,
    screen_reader: bool,
    full_screen: bool,
) -> bool {
    if screen_reader {
        return false;
    }
    match mode {
        MouseMode::On => true,
        MouseMode::Off => false,
        MouseMode::Auto => full_screen,
    }
}

impl std::fmt::Display for LaunchParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Display(text) => formatter.write_str(text),
            Self::InvalidArguments => formatter.write_str("invalid arguments; use --help"),
        }
    }
}

impl std::error::Error for LaunchParseError {}

/// Parses and validates the resident TUI command line without acquiring a terminal.
pub fn parse_launch_config<I, T>(arguments: I) -> Result<LaunchConfig, LaunchParseError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let raw = RawArgs::try_parse_from(arguments).map_err(|error| match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            LaunchParseError::Display(error.to_string())
        }
        _ => LaunchParseError::InvalidArguments,
    })?;
    if raw.session.as_deref().is_some_and(str::is_empty)
        || raw.definition.as_deref().is_some_and(str::is_empty)
        || raw
            .state_dir
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        || LiveHostClient::new(&raw.host, VALIDATION_LIMITS).is_err()
    {
        return Err(LaunchParseError::InvalidArguments);
    }
    Ok(LaunchConfig {
        host: raw.host,
        session: raw.session,
        definition: raw.definition,
        state_dir: raw.state_dir,
        theme: raw.theme.unwrap_or_default(),
        theme_explicit: raw.theme.is_some(),
        screen_reader: raw.screen_reader,
        reduced_motion: raw.reduced_motion,
        reduced_motion_explicit: raw.reduced_motion,
        mouse: raw.mouse.unwrap_or_default(),
        mouse_explicit: raw.mouse.is_some(),
        ephemeral: raw.ephemeral,
        no_prompt_history: raw.no_prompt_history,
        #[cfg(feature = "test-hooks")]
        test_crash_hook: raw.test_crash_hook,
    })
}

#[derive(Debug, Parser)]
#[command(
    name = "garive-tui",
    version,
    about = "Resident Garive terminal client"
)]
struct RawArgs {
    #[arg(long)]
    host: String,
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    definition: Option<String>,
    #[arg(long)]
    state_dir: Option<PathBuf>,
    #[arg(long, value_enum)]
    theme: Option<Theme>,
    #[arg(long)]
    screen_reader: bool,
    #[arg(long)]
    reduced_motion: bool,
    #[arg(long, value_enum)]
    mouse: Option<MouseMode>,
    #[arg(long)]
    ephemeral: bool,
    #[arg(long)]
    no_prompt_history: bool,
    #[cfg(feature = "test-hooks")]
    #[arg(long, value_enum, hide = true)]
    test_crash_hook: Option<TestCrashHook>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_environment_selects_accessible_fallbacks_without_overriding_cli_theme() {
        let mut config =
            parse_launch_config(["garive-tui", "--host", "http://127.0.0.1:4317/"]).unwrap();
        apply_terminal_environment(&mut config, Some("dumb"), true);
        assert_eq!(config.theme, Theme::Mono);
        assert!(config.screen_reader);
        assert_eq!(config.mouse, MouseMode::Auto);

        let mut explicit = parse_launch_config([
            "garive-tui",
            "--host",
            "http://127.0.0.1:4317/",
            "--theme",
            "light",
        ])
        .unwrap();
        apply_terminal_environment(&mut explicit, Some("xterm-256color"), true);
        assert_eq!(explicit.theme, Theme::Light);
        assert!(!explicit.screen_reader);
    }

    #[test]
    fn mouse_capture_resolution_preserves_preference_and_requires_accessible_full_screen() {
        assert!(mouse_capture_enabled(MouseMode::On, false, false));
        assert!(!mouse_capture_enabled(MouseMode::Off, false, true));
        assert!(mouse_capture_enabled(MouseMode::Auto, false, true));
        assert!(!mouse_capture_enabled(MouseMode::Auto, false, false));
        for mode in [MouseMode::Auto, MouseMode::On, MouseMode::Off] {
            assert!(!mouse_capture_enabled(mode, true, true));
        }
    }
}
