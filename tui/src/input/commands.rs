use crate::args::{MouseMode, Theme};

pub(crate) const COMMAND_PALETTE: &[CommandSpec] = &[
    CommandSpec::new("/new", "Create session", CommandRequirement::InstalledAgent),
    CommandSpec::new("/sessions", "Switch session", CommandRequirement::Always),
    CommandSpec::new("/status", "Connection details", CommandRequirement::Always),
    CommandSpec::new(
        "/retry",
        "Retry unknown command",
        CommandRequirement::PendingCommand,
    ),
    CommandSpec::new(
        "/reconnect",
        "Resume event stream",
        CommandRequirement::Always,
    ),
    CommandSpec::new(
        "/cancel",
        "Cancel running turn",
        CommandRequirement::RunningTurn,
    ),
    CommandSpec::new(
        "/theme system",
        "Follow terminal theme",
        CommandRequirement::Always,
    ),
    CommandSpec::new(
        "/mouse off",
        "Disable mouse capture next launch",
        CommandRequirement::Always,
    ),
    CommandSpec::new(
        "/copy last",
        "Copy last completion",
        CommandRequirement::VisibleCompletion,
    ),
    CommandSpec::new(
        "/copy session-id",
        "Copy Session ID",
        CommandRequirement::SelectedSession,
    ),
    CommandSpec::new("/help", "Keyboard guide", CommandRequirement::Always),
    CommandSpec::new("/quit", "Exit safely", CommandRequirement::Always),
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CommandContext {
    pub(crate) has_installed_agent: bool,
    pub(crate) has_pending_command: bool,
    pub(crate) has_running_turn: bool,
    pub(crate) has_visible_completion: bool,
    pub(crate) has_selected_session: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommandSpec {
    pub(crate) input: &'static str,
    pub(crate) help: &'static str,
    requirement: CommandRequirement,
}

impl CommandSpec {
    const fn new(input: &'static str, help: &'static str, requirement: CommandRequirement) -> Self {
        Self {
            input,
            help,
            requirement,
        }
    }

    pub(crate) fn unavailable_reason(self, context: CommandContext) -> Option<&'static str> {
        match self.requirement {
            CommandRequirement::Always => None,
            CommandRequirement::InstalledAgent if !context.has_installed_agent => {
                Some("no Agent is installed")
            }
            CommandRequirement::PendingCommand if !context.has_pending_command => {
                Some("no pending command")
            }
            CommandRequirement::RunningTurn if !context.has_running_turn => {
                Some("no Turn is running")
            }
            CommandRequirement::VisibleCompletion if !context.has_visible_completion => {
                Some("no completion is visible")
            }
            CommandRequirement::SelectedSession if !context.has_selected_session => {
                Some("no Session is selected")
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandRequirement {
    Always,
    InstalledAgent,
    PendingCommand,
    RunningTurn,
    VisibleCompletion,
    SelectedSession,
}

pub(crate) fn command_matches(name: &str, help: &str, filter: &str) -> bool {
    let searchable = format!("{name} {help}").to_lowercase();
    filter
        .split_whitespace()
        .all(|term| searchable.contains(&term.to_lowercase()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    New { definition: Option<String> },
    Sessions { filter: Option<String> },
    Help,
    Status,
    Retry,
    Reconnect,
    Cancel,
    Theme(Theme),
    Mouse(MouseMode),
    CopyLast,
    CopySessionId,
    Quit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CommandParse {
    NotCommand,
    Valid(Command),
    Invalid,
}

pub(crate) fn parse_command(text: &str) -> CommandParse {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('/') {
        return CommandParse::NotCommand;
    }
    if trimmed.contains('\n') || trimmed.len() > 4_096 {
        return CommandParse::Invalid;
    }
    let Some(words) = words(trimmed) else {
        return CommandParse::Invalid;
    };
    let Some((name, arguments)) = words.split_first() else {
        return CommandParse::Invalid;
    };
    let command = match (name.as_str(), arguments) {
        ("/new", []) => Command::New { definition: None },
        ("/new", [definition]) => Command::New {
            definition: Some(definition.clone()),
        },
        ("/sessions", []) => Command::Sessions { filter: None },
        ("/sessions", values) => Command::Sessions {
            filter: Some(values.join(" ")),
        },
        ("/help", []) => Command::Help,
        ("/status", []) => Command::Status,
        ("/retry", []) => Command::Retry,
        ("/reconnect", []) => Command::Reconnect,
        ("/cancel", []) => Command::Cancel,
        ("/theme", [value]) => match value.as_str() {
            "system" => Command::Theme(Theme::System),
            "dark" => Command::Theme(Theme::Dark),
            "light" => Command::Theme(Theme::Light),
            "mono" => Command::Theme(Theme::Mono),
            _ => return CommandParse::Invalid,
        },
        ("/mouse", [value]) => match value.as_str() {
            "on" => Command::Mouse(MouseMode::On),
            "off" => Command::Mouse(MouseMode::Off),
            _ => return CommandParse::Invalid,
        },
        ("/copy", [value]) if value == "last" => Command::CopyLast,
        ("/copy", [value]) if value == "session-id" => Command::CopySessionId,
        ("/quit", []) => Command::Quit,
        _ => return CommandParse::Invalid,
    };
    CommandParse::Valid(command)
}

fn words(value: &str) -> Option<Vec<String>> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            if !matches!(character, '"' | '\\') {
                return None;
            }
            current.push(character);
            escaped = false;
        } else {
            match character {
                '\\' if quoted => escaped = true,
                '"' => quoted = !quoted,
                value if value.is_whitespace() && !quoted => {
                    if !current.is_empty() {
                        result.push(std::mem::take(&mut current));
                    }
                }
                value => current.push(value),
            }
        }
    }
    if quoted || escaped {
        return None;
    }
    if !current.is_empty() {
        result.push(current);
    }
    Some(result)
}
