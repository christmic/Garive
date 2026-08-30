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
    /// Whether to use the linear accessible presentation.
    pub screen_reader: bool,
    /// Whether to suppress nonessential animation.
    pub reduced_motion: bool,
    /// Mouse capture preference for this process.
    pub mouse: MouseMode,
    /// Whether to disable all local presentation writes.
    pub ephemeral: bool,
    /// Whether to disable prompt-history writes.
    pub no_prompt_history: bool,
}

/// Non-launch outcome produced while parsing process arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchParseError {
    /// Safe help or version text that should be printed to stdout with exit zero.
    Display(String),
    /// Invalid arguments; the message deliberately excludes supplied values.
    InvalidArguments,
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
        theme: raw.theme,
        screen_reader: raw.screen_reader,
        reduced_motion: raw.reduced_motion,
        mouse: raw.mouse,
        ephemeral: raw.ephemeral,
        no_prompt_history: raw.no_prompt_history,
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
    #[arg(long, value_enum, default_value_t)]
    theme: Theme,
    #[arg(long)]
    screen_reader: bool,
    #[arg(long)]
    reduced_motion: bool,
    #[arg(long, value_enum, default_value_t)]
    mouse: MouseMode,
    #[arg(long)]
    ephemeral: bool,
    #[arg(long)]
    no_prompt_history: bool,
}
