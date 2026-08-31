use crate::{
    application::{AppModel, Overlay},
    input::COMMAND_PALETTE,
};

use super::{
    presentation::{action_overlay_copy, suspension_copy, HELP_NOTES},
    primitives::selection_window,
    short_id,
};
use crate::input::help_hints;

const LIST_CAPACITY: usize = 10;

pub(crate) fn overlay_text(model: &AppModel) -> String {
    let Some(overlay) = model.overlay else {
        return String::new();
    };
    let value = match overlay {
        Overlay::CommandPalette => command_palette(model),
        Overlay::Help => help(),
        Overlay::SessionPicker => session_picker(model),
        Overlay::TurnNavigator => turn_navigator(model),
        Overlay::PromptHistory => prompt_history(model),
        Overlay::Inspector => super::inspector::linear_text(model),
        Overlay::Suspension => {
            let copy = suspension_copy(model.suspension.as_ref());
            let message = copy.message.unwrap_or_default();
            format!(
                "{}. {} {}\n{}\nPress Enter to respond now.",
                copy.title, copy.context, message, copy.guidance
            )
        }
        Overlay::UnknownCommand
        | Overlay::ErrorDetails
        | Overlay::EphemeralConfirmation
        | Overlay::QuitConfirmation => linear_action_overlay(model, overlay),
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
            let ordinal = model
                .sessions
                .iter()
                .position(|item| item.session_id == session.session_id)
                .map(|position| position + 1)
                .unwrap_or(index + 1);
            numbered(
                index,
                model.session_selection,
                format!(
                    "Session {ordinal}, {}, {}.",
                    short_id(&session.definition_id),
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

fn turn_navigator(model: &AppModel) -> String {
    let matches = model.matching_landmark_indices();
    let rows = window(matches.len(), model.turn_selection)
        .map(|selection_index| {
            let landmark = &model.conversation_landmarks[matches[selection_index]];
            numbered(
                selection_index,
                model.turn_selection,
                format!("Turn {}. {}", landmark.ordinal, landmark.prompt_preview),
            )
        })
        .collect::<Vec<_>>();
    list_prompt(
        "Jump to a Turn",
        &model.turn_filter,
        rows,
        "No matching Turns.",
        "Use arrows and Enter to jump, or Escape to close.",
    )
}

fn help() -> String {
    let actions = help_hints().map(|hint| format!("{}: {}.", hint.spoken_key, hint.action));
    std::iter::once("Keyboard guide.".to_owned())
        .chain(actions)
        .chain(HELP_NOTES.iter().map(|note| (*note).to_owned()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn linear_action_overlay(model: &AppModel, overlay: Overlay) -> String {
    let copy = action_overlay_copy(model, overlay)
        .expect("action overlay variants always have shared presentation");
    let guidance = copy
        .hints
        .iter()
        .map(|hint| format!("Press {} to {}.", hint.spoken_key, hint.action))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{}. {} {}", copy.title, copy.body, guidance)
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
