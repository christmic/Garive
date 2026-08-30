use crate::args::{MouseMode, Theme};

pub(crate) const COMMAND_PALETTE: [(&str, &str); 8] = [
    ("/new", "Create session"),
    ("/sessions", "Switch session"),
    ("/status", "Connection details"),
    ("/retry", "Retry unknown command"),
    ("/reconnect", "Resume event stream"),
    ("/cancel", "Cancel running turn"),
    ("/help", "Keyboard guide"),
    ("/quit", "Exit safely"),
];

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
