use crate::{
    application::{ActionOverlayBinding, AppModel, Overlay},
    input::{response_schema_control, SchemaControl},
};

use super::presentation::{action_overlay_copy, suspension_copy};

pub(crate) struct DecisionSheetSpec {
    pub(crate) title: String,
    pub(crate) body: Vec<String>,
    pub(crate) response: Option<DecisionResponseSpec>,
    pub(crate) tone: DecisionSheetTone,
    pub(crate) actions: Vec<ActionOverlayBinding>,
}

pub(crate) enum DecisionResponseSpec {
    Editor {
        guidance: &'static str,
        draft: String,
    },
    Choices {
        guidance: &'static str,
        choices: Vec<String>,
        selected: usize,
    },
    ReadOnly {
        guidance: &'static str,
    },
}

pub(crate) enum DecisionSheetTone {
    Neutral,
    Warning,
    Danger,
}

pub(crate) fn project(model: &AppModel, overlay: Overlay) -> Option<DecisionSheetSpec> {
    if overlay == Overlay::Suspension {
        return Some(suspension(model));
    }
    let copy = action_overlay_copy(model, overlay)?;
    Some(DecisionSheetSpec {
        title: copy.title.into(),
        body: copy.body.lines().map(str::to_owned).collect(),
        response: None,
        tone: match overlay {
            Overlay::UnknownCommand | Overlay::AbandonConfirmation => DecisionSheetTone::Danger,
            Overlay::EphemeralConfirmation => DecisionSheetTone::Warning,
            _ => DecisionSheetTone::Neutral,
        },
        actions: model.decision_bindings(overlay)?.to_vec(),
    })
}

fn suspension(model: &AppModel) -> DecisionSheetSpec {
    let copy = suspension_copy(model.suspension.as_ref());
    let mut body = vec![copy.context.into()];
    if let Some(message) = copy.message {
        body.push(message);
    }
    DecisionSheetSpec {
        title: copy.title.into(),
        body,
        response: Some(
            if let Some(control) = model
                .suspension_is_interactive()
                .then(|| {
                    model.suspension.as_ref().and_then(|suspension| {
                        suspension
                            .response_schema_json
                            .as_deref()
                            .and_then(response_schema_control)
                    })
                })
                .flatten()
            {
                match control {
                    SchemaControl::Editor => DecisionResponseSpec::Editor {
                        guidance: copy.guidance,
                        draft: model
                            .suspension_response
                            .as_ref()
                            .map(|state| state.editor.text().to_owned())
                            .unwrap_or_default(),
                    },
                    SchemaControl::Choices(choices) => DecisionResponseSpec::Choices {
                        guidance: copy.guidance,
                        selected: model
                            .suspension_response
                            .as_ref()
                            .map_or(0, |state| state.choice_selection),
                        choices,
                    },
                }
            } else {
                DecisionResponseSpec::ReadOnly {
                    guidance: "This suspension is status-only; no response can be submitted here.",
                }
            },
        ),
        tone: DecisionSheetTone::Warning,
        actions: model
            .decision_bindings(Overlay::Suspension)
            .unwrap_or_default()
            .to_vec(),
    }
}
