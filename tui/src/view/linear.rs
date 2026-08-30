use crate::{
    application::{AppModel, Overlay},
    input::COMMAND_PALETTE,
};

use super::{presentation::suspension_copy, primitives::selection_window, short_id, short_tail};

const LIST_CAPACITY: usize = 10;

pub(crate) fn overlay_text(model: &AppModel) -> String {
    let Some(overlay) = model.overlay else {
        return String::new();
    };
    let value = match overlay {
        Overlay::CommandPalette => command_palette(model),
        Overlay::Help => help(),
        Overlay::SessionPicker => session_picker(model),
        Overlay::PromptHistory => prompt_history(model),
        Overlay::Suspension => {
            let copy = suspension_copy(model.suspension.as_ref());
            let message = copy.message.unwrap_or_default();
            format!(
                "{}. {} {}\n{}\nPress Enter to respond now.",
                copy.title, copy.context, message, copy.guidance
            )
        }
        Overlay::UnknownCommand => "Command result unknown. Press Enter for exact retry, or A to abandon the local recovery record.".into(),
        Overlay::ErrorDetails => format!(
            "Status details. {} Press Escape to close.",
            model.notice.as_deref().unwrap_or("No safe details available.")
        ),
        Overlay::EphemeralConfirmation => "Ephemeral mode cannot recover a lost mutation response. Press Enter to accept for this run, or Escape to cancel.".into(),
        Overlay::QuitConfirmation => {
            "Quit Garive? Press Enter to quit, or Escape to keep working.".into()
        }
    };
    safe(&value)
}

pub(crate) fn safe(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\n' | '\t' => character,
            value
                if value.is_control()
                    || matches!(value, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
            {
                '�'
            }
            value => value,
        })
        .collect()
}

fn command_palette(model: &AppModel) -> String {
    let matches = model.matching_command_indices();
    let rows = window(matches.len(), model.command_selection)
        .map(|index| {
            let command = COMMAND_PALETTE[matches[index]];
            let unavailable = command
                .unavailable_reason(model.command_context())
                .map_or_else(String::new, |reason| format!(". Unavailable: {reason}"));
            numbered(
                index,
                model.command_selection,
                format!("{}: {}{unavailable}", command.input, command.help),
            )
        })
        .collect::<Vec<_>>();
    list_prompt(
        "Command palette",
        &model.command_filter,
        rows,
        "No matching commands.",
        "Use arrows and Enter, or Escape to close.",
    )
}

fn session_picker(model: &AppModel) -> String {
    let matches = model.matching_sessions().collect::<Vec<_>>();
    let rows = window(matches.len(), model.session_selection)
        .map(|index| {
            let session = matches[index];
            numbered(
                index,
                model.session_selection,
                format!(
                    "{} Session ending {}, {}.",
                    short_id(&session.definition_id),
                    short_tail(&session.session_id),
                    session.latest_turn_state.as_deref().unwrap_or("new")
                ),
            )
        })
        .collect::<Vec<_>>();
    list_prompt(
        "Switch Session",
        &model.session_filter,
        rows,
        "No matching Sessions.",
        "Use arrows and Enter, or Escape to close.",
    )
}

fn prompt_history(model: &AppModel) -> String {
    let matches = model.matching_history().collect::<Vec<_>>();
    let rows = window(matches.len(), model.history_selection)
        .map(|index| {
            numbered(
                index,
                model.history_selection,
                matches[index].lines().next().unwrap_or_default().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    list_prompt(
        "Prompt history",
        &model.history_filter,
        rows,
        "No matching prompt history.",
        "Use arrows and Enter, or Escape to close.",
    )
}

fn help() -> String {
    [
        "Keyboard guide.",
        "Enter sends. Control J is the portable newline when Shift Enter is unavailable.",
        "Control S opens Sessions. Control P opens commands. Control R opens prompt history.",
        "Control C cancels a running Turn. Control Q asks to quit. Escape closes a nonblocking prompt.",
        "No function keys are required. Color and mouse are optional; every state and action has text and a keyboard control.",
        "Copy uses terminal OSC 52 support; otherwise select terminal text manually.",
    ]
    .join(" ")
}

fn window(total: usize, selected: usize) -> std::ops::Range<usize> {
    let (start, end) = selection_window(total, selected, LIST_CAPACITY);
    start..end
}

fn numbered(index: usize, selected: usize, content: String) -> String {
    let marker = if index == selected { ">" } else { " " };
    format!("{marker} {}. {content}", index + 1)
}

fn list_prompt(
    title: &str,
    filter: &str,
    rows: Vec<String>,
    empty: &str,
    guidance: &str,
) -> String {
    let filter = if filter.is_empty() {
        String::new()
    } else {
        format!(" Filter: {filter}.")
    };
    let rows = if rows.is_empty() {
        empty.to_owned()
    } else {
        rows.join("\n")
    };
    format!("{title}.{filter}\n{rows}\n{guidance}")
}
