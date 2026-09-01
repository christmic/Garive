use crate::application::{AppModel, ExecutionState, Overlay};

use super::{agent_label, decision_sheet, presentation::HELP_NOTES, primitives::selection_window};
use crate::input::help_hints;

const LIST_CAPACITY: usize = 10;

pub(crate) fn composer_status(model: &AppModel) -> &'static str {
    if let Some(status) = super::composer_run_rail::linear_status(model) {
        status
    } else if model.composer_is_frozen {
        "Composer locked. Draft retained. Editing is unavailable until durable command truth."
    } else if model.execution == ExecutionState::Following {
        if model.overlay.is_some() {
            "Current Turn running. Draft retained. The active overlay owns input."
        } else {
            "Current Turn running. Draft retained. Enter reports the boundary; Escape requests cancellation."
        }
    } else {
        "Composer ready. Editing is available."
    }
}

pub(crate) fn overlay_text(model: &AppModel) -> String {
    let Some(overlay) = model.overlay else {
        return String::new();
    };
    let value = match overlay {
        Overlay::CommandPalette => super::overlay::command_palette::linear_text(model),
        Overlay::Help => help(),
        Overlay::SessionPicker => session_picker(model),
        Overlay::TurnNavigator => turn_navigator(model),
        Overlay::PromptHistory => prompt_history(model),
        Overlay::Inspector => super::inspector::linear_text(model),
        Overlay::Suspension => linear_decision_sheet(model, overlay),
        Overlay::UnknownCommand
        | Overlay::AbandonConfirmation
        | Overlay::ErrorDetails
        | Overlay::EphemeralConfirmation
        | Overlay::QuitConfirmation => linear_decision_sheet(model, overlay),
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
                    agent_label(),
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

fn linear_decision_sheet(model: &AppModel, overlay: Overlay) -> String {
    let spec = decision_sheet::project(model, overlay).expect("decision overlay has a spec");
    let response = spec
        .response
        .map_or_else(String::new, |response| match response {
            decision_sheet::DecisionResponseSpec::Editor {
                guidance, draft, ..
            } => {
                let value = if draft.is_empty() { "empty" } else { "entered" };
                format!(" Response ({value}): {guidance}")
            }
            decision_sheet::DecisionResponseSpec::ReadOnly { guidance } => {
                format!(" Read only: {guidance}")
            }
            decision_sheet::DecisionResponseSpec::Choices {
                guidance,
                choices,
                selected,
            } => {
                let navigation = if choices.len() > 1 {
                    " Use Up or Down to select."
                } else {
                    ""
                };
                format!(
                    " Choices: {}. Selected: {}.{navigation} {guidance}",
                    choices.join(", "),
                    choices.get(selected).map(String::as_str).unwrap_or("none")
                )
            }
        });
    let guidance = spec
        .actions
        .iter()
        .map(|hint| format!("Press {} to {}.", hint.spoken_key, hint.action))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{}. {}{response} {guidance}",
        spec.title,
        spec.body.join(" ")
    )
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
