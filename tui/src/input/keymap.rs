use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShortcutIntent {
    Quit,
    ClearOrCancel,
    NewSession,
    OpenSessions,
    OpenCommands,
    OpenHistory,
    OpenExternalEditor,
    InsertNewline,
    Redraw,
    DocumentStart,
    DocumentEnd,
    Undo,
    Redo,
    KillStart,
    KillEnd,
    Yank,
    CopySelection,
    LogicalLineStart,
    LogicalLineEnd,
    GraphemeLeft,
    GraphemeRight,
    WordLeft,
    WordRight,
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
}

#[derive(Clone, Copy)]
struct Shortcut {
    code: KeyCode,
    modifiers: KeyModifiers,
    intent: ShortcutIntent,
}

#[derive(Clone, Copy)]
pub(crate) struct ShortcutHelp {
    pub(crate) visual_key: &'static str,
    pub(crate) spoken_key: &'static str,
    pub(crate) action: &'static str,
    intents: &'static [ShortcutIntent],
}

macro_rules! shortcut {
    ($code:expr, $modifiers:expr, $intent:ident) => {
        Shortcut {
            code: $code,
            modifiers: $modifiers,
            intent: ShortcutIntent::$intent,
        }
    };
}

const SHORTCUTS: &[Shortcut] = &[
    shortcut!(KeyCode::Char('q'), KeyModifiers::CONTROL, Quit),
    shortcut!(KeyCode::Char('c'), KeyModifiers::CONTROL, ClearOrCancel),
    shortcut!(KeyCode::Char('n'), KeyModifiers::CONTROL, NewSession),
    shortcut!(KeyCode::Char('s'), KeyModifiers::CONTROL, OpenSessions),
    shortcut!(KeyCode::Char('p'), KeyModifiers::CONTROL, OpenCommands),
    shortcut!(KeyCode::Char('r'), KeyModifiers::CONTROL, OpenHistory),
    shortcut!(
        KeyCode::Char('g'),
        KeyModifiers::CONTROL,
        OpenExternalEditor
    ),
    shortcut!(KeyCode::Char('j'), KeyModifiers::CONTROL, InsertNewline),
    shortcut!(KeyCode::Char('l'), KeyModifiers::CONTROL, Redraw),
    shortcut!(KeyCode::Home, KeyModifiers::CONTROL, DocumentStart),
    shortcut!(KeyCode::End, KeyModifiers::CONTROL, DocumentEnd),
    shortcut!(KeyCode::Char('z'), KeyModifiers::CONTROL, Undo),
    shortcut!(KeyCode::Char('z'), KeyModifiers::ALT, Redo),
    shortcut!(KeyCode::Char('u'), KeyModifiers::CONTROL, KillStart),
    shortcut!(KeyCode::Char('k'), KeyModifiers::CONTROL, KillEnd),
    shortcut!(KeyCode::Char('y'), KeyModifiers::CONTROL, Yank),
    shortcut!(KeyCode::Char('c'), KeyModifiers::ALT, CopySelection),
    shortcut!(KeyCode::Char('a'), KeyModifiers::CONTROL, LogicalLineStart),
    shortcut!(KeyCode::Char('e'), KeyModifiers::CONTROL, LogicalLineEnd),
    shortcut!(KeyCode::Char('b'), KeyModifiers::CONTROL, GraphemeLeft),
    shortcut!(KeyCode::Char('f'), KeyModifiers::CONTROL, GraphemeRight),
    shortcut!(KeyCode::Char('b'), KeyModifiers::ALT, WordLeft),
    shortcut!(KeyCode::Char('f'), KeyModifiers::ALT, WordRight),
    shortcut!(KeyCode::Left, KeyModifiers::ALT, WordLeft),
    shortcut!(KeyCode::Right, KeyModifiers::ALT, WordRight),
    shortcut!(KeyCode::Char('h'), KeyModifiers::CONTROL, DeleteBackward),
    shortcut!(KeyCode::Char('d'), KeyModifiers::CONTROL, DeleteForward),
    shortcut!(
        KeyCode::Char('w'),
        KeyModifiers::CONTROL,
        DeleteWordBackward
    ),
    shortcut!(KeyCode::Char('d'), KeyModifiers::ALT, DeleteWordForward),
    shortcut!(KeyCode::Backspace, KeyModifiers::ALT, DeleteWordBackward),
    shortcut!(KeyCode::Delete, KeyModifiers::ALT, DeleteWordForward),
];

const HELP_HINTS: &[ShortcutHelp] = &[
    help("Enter", "Enter", "send", &[]),
    help(
        "Ctrl+J",
        "Control J",
        "new line",
        &[ShortcutIntent::InsertNewline],
    ),
    help(
        "Ctrl+N",
        "Control N",
        "new Session",
        &[ShortcutIntent::NewSession],
    ),
    help(
        "Ctrl+S",
        "Control S",
        "open Sessions",
        &[ShortcutIntent::OpenSessions],
    ),
    help(
        "Ctrl+P",
        "Control P",
        "open commands",
        &[ShortcutIntent::OpenCommands],
    ),
    help(
        "Ctrl+R",
        "Control R",
        "open prompt history",
        &[ShortcutIntent::OpenHistory],
    ),
    help(
        "Ctrl+G",
        "Control G",
        "edit externally",
        &[ShortcutIntent::OpenExternalEditor],
    ),
    help(
        "Ctrl+U/K",
        "Control U or Control K",
        "kill to line edge",
        &[ShortcutIntent::KillStart, ShortcutIntent::KillEnd],
    ),
    help(
        "Ctrl+Y",
        "Control Y",
        "yank killed text",
        &[ShortcutIntent::Yank],
    ),
    help("Esc", "Escape", "close guide", &[]),
    help(
        "Ctrl+Q",
        "Control Q",
        "ask to quit",
        &[ShortcutIntent::Quit],
    ),
];

const fn help(
    visual_key: &'static str,
    spoken_key: &'static str,
    action: &'static str,
    intents: &'static [ShortcutIntent],
) -> ShortcutHelp {
    ShortcutHelp {
        visual_key,
        spoken_key,
        action,
        intents,
    }
}

pub(crate) fn resolve_shortcut(key: KeyEvent) -> Option<ShortcutIntent> {
    SHORTCUTS
        .iter()
        .find(|binding| binding.code == key.code && binding.modifiers == key.modifiers)
        .map(|binding| binding.intent)
}

pub(crate) fn help_hints() -> impl Iterator<Item = &'static ShortcutHelp> {
    HELP_HINTS.iter().filter(|hint| {
        hint.intents
            .iter()
            .all(|intent| SHORTCUTS.iter().any(|binding| binding.intent == *intent))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_are_unique_and_every_help_intent_is_bound() {
        for (index, binding) in SHORTCUTS.iter().enumerate() {
            assert!(!SHORTCUTS[..index].iter().any(|prior| {
                prior.code == binding.code && prior.modifiers == binding.modifiers
            }));
        }
        for hint in HELP_HINTS {
            assert!(!hint.visual_key.is_empty() && !hint.spoken_key.is_empty());
            for intent in hint.intents {
                assert!(SHORTCUTS.iter().any(|binding| binding.intent == *intent));
            }
        }
    }
}
