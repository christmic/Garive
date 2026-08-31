use super::{AppModel, Overlay};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionOverlayKey {
    Enter,
    Escape,
    CtrlQ,
    Character(char),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionOverlayIntent {
    Close,
    ConfirmQuit,
    AcceptEphemeral,
    ExactRetry,
    OpenAbandonConfirmation,
    ConfirmAbandon,
    ReturnToUnknown,
    SubmitSuspension,
    LeaveSafely,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ActionOverlayBinding {
    pub(crate) key: ActionOverlayKey,
    pub(crate) visual_key: &'static str,
    pub(crate) spoken_key: &'static str,
    pub(crate) action: &'static str,
    pub(crate) intent: ActionOverlayIntent,
}

const UNKNOWN_RESULT_BINDINGS: &[ActionOverlayBinding] = &[
    binding(
        ActionOverlayKey::Enter,
        "Enter",
        "Enter",
        "exact retry",
        ActionOverlayIntent::ExactRetry,
    ),
    binding(
        ActionOverlayKey::Character('a'),
        "A",
        "A",
        "abandon local record",
        ActionOverlayIntent::OpenAbandonConfirmation,
    ),
];
const ABANDON_CONFIRMATION_BINDINGS: &[ActionOverlayBinding] = &[
    binding(
        ActionOverlayKey::Enter,
        "Enter",
        "Enter",
        "abandon local record",
        ActionOverlayIntent::ConfirmAbandon,
    ),
    binding(
        ActionOverlayKey::Escape,
        "Esc",
        "Escape",
        "keep recovery record",
        ActionOverlayIntent::ReturnToUnknown,
    ),
];
const SUSPENSION_BINDINGS: &[ActionOverlayBinding] = &[
    binding(
        ActionOverlayKey::Enter,
        "Enter",
        "Enter",
        "submit response",
        ActionOverlayIntent::SubmitSuspension,
    ),
    binding(
        ActionOverlayKey::CtrlQ,
        "Ctrl+Q",
        "Control Q",
        "leave safely",
        ActionOverlayIntent::LeaveSafely,
    ),
];
const READ_ONLY_SUSPENSION_BINDINGS: &[ActionOverlayBinding] = &[binding(
    ActionOverlayKey::CtrlQ,
    "Ctrl+Q",
    "Control Q",
    "leave safely",
    ActionOverlayIntent::LeaveSafely,
)];
const CLOSE_BINDINGS: &[ActionOverlayBinding] = &[binding(
    ActionOverlayKey::Escape,
    "Esc",
    "Escape",
    "close",
    ActionOverlayIntent::Close,
)];
const EPHEMERAL_BINDINGS: &[ActionOverlayBinding] = &[
    binding(
        ActionOverlayKey::Enter,
        "Enter",
        "Enter",
        "accept for this run",
        ActionOverlayIntent::AcceptEphemeral,
    ),
    binding(
        ActionOverlayKey::Escape,
        "Esc",
        "Escape",
        "cancel",
        ActionOverlayIntent::Close,
    ),
];
const QUIT_BINDINGS: &[ActionOverlayBinding] = &[
    binding(
        ActionOverlayKey::Enter,
        "Enter",
        "Enter",
        "quit",
        ActionOverlayIntent::ConfirmQuit,
    ),
    binding(
        ActionOverlayKey::Escape,
        "Esc",
        "Escape",
        "keep working",
        ActionOverlayIntent::Close,
    ),
];

const fn binding(
    key: ActionOverlayKey,
    visual_key: &'static str,
    spoken_key: &'static str,
    action: &'static str,
    intent: ActionOverlayIntent,
) -> ActionOverlayBinding {
    ActionOverlayBinding {
        key,
        visual_key,
        spoken_key,
        action,
        intent,
    }
}

impl Overlay {
    pub(crate) fn action_bindings(self) -> Option<&'static [ActionOverlayBinding]> {
        match self {
            Self::UnknownCommand => Some(UNKNOWN_RESULT_BINDINGS),
            Self::AbandonConfirmation => Some(ABANDON_CONFIRMATION_BINDINGS),
            Self::ErrorDetails => Some(CLOSE_BINDINGS),
            Self::EphemeralConfirmation => Some(EPHEMERAL_BINDINGS),
            Self::QuitConfirmation => Some(QUIT_BINDINGS),
            _ => None,
        }
    }
}

impl AppModel {
    pub(crate) fn decision_bindings(
        &self,
        overlay: Overlay,
    ) -> Option<&'static [ActionOverlayBinding]> {
        if overlay == Overlay::Suspension {
            return Some(if self.suspension_is_interactive() {
                SUSPENSION_BINDINGS
            } else {
                READ_ONLY_SUSPENSION_BINDINGS
            });
        }
        overlay.action_bindings()
    }
}

#[cfg(test)]
mod tests {
    use garive_host_client::SuspensionView;

    use super::*;

    #[test]
    fn every_action_overlay_binding_round_trips_to_its_controller_intent() {
        for overlay in [
            Overlay::UnknownCommand,
            Overlay::AbandonConfirmation,
            Overlay::ErrorDetails,
            Overlay::EphemeralConfirmation,
            Overlay::QuitConfirmation,
        ] {
            let bindings = overlay.action_bindings().expect("action bindings");
            assert!(!bindings.is_empty());
            for binding in bindings {
                assert_eq!(
                    AppModel::default()
                        .decision_bindings(overlay)
                        .and_then(|bindings| bindings.iter().find(|item| item.key == binding.key))
                        .map(|item| item.intent),
                    Some(binding.intent)
                );
                assert!(!binding.visual_key.is_empty());
                assert!(!binding.spoken_key.is_empty());
                assert!(!binding.action.is_empty());
            }
        }
        assert!(Overlay::Help.action_bindings().is_none());
        let model = AppModel {
            suspension: Some(SuspensionView {
                suspension_id: "s".into(),
                session_version: 1,
                kind: "approval_required".into(),
                prompt_schema: "garive.public-suspension-prompt.v1".into(),
                prompt_json: "{}".into(),
                prompt_digest: "0".repeat(64),
                response_schema_json: Some(r#"{"type":"boolean"}"#.into()),
                response_schema_digest: Some("1".repeat(64)),
            }),
            ..Default::default()
        };
        assert_eq!(
            model.decision_bindings(Overlay::Suspension).unwrap().len(),
            2
        );
    }
}
