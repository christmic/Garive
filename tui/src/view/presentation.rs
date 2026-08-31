use garive_host_client::SuspensionView;
use serde::Deserialize;

use crate::{
    application::{AppModel, Overlay, TimelineTone},
    input::describe_schema,
};

pub(crate) struct ActionOverlayCopy {
    pub(crate) title: &'static str,
    pub(crate) body: String,
}

pub(crate) const HELP_NOTES: &[&str] = &[
    "Durable truth comes from the local Garive Host.",
    "No function keys are required. Color and mouse are optional.",
    "Copy uses terminal OSC 52 support; otherwise select terminal text manually.",
];

pub(crate) fn action_overlay_copy(model: &AppModel, overlay: Overlay) -> Option<ActionOverlayCopy> {
    let copy = match overlay {
        Overlay::UnknownCommand => ActionOverlayCopy {
            title: "Command result unknown",
            body: model.notice.clone().unwrap_or_else(|| {
                "Durable outcome is unknown; nothing will be inferred or replayed automatically."
                    .into()
            }),
        },
        Overlay::AbandonConfirmation => ActionOverlayCopy {
            title: "Abandon recovery record?",
            body: "The durable Host outcome remains unknown. Abandoning removes only this local recovery record and cannot prove that the command did not commit.".into(),
        },
        Overlay::ErrorDetails => ActionOverlayCopy {
            title: "Status details",
            body: model
                .notice
                .clone()
                .unwrap_or_else(|| "No additional safe details.".into()),
        },
        Overlay::EphemeralConfirmation => ActionOverlayCopy {
            title: "Ephemeral mode",
            body: "A lost mutation response cannot be recovered after exit.".into(),
        },
        Overlay::QuitConfirmation => ActionOverlayCopy {
            title: "Quit Garive?",
            body: "Your Sessions stay durable in the Host.".into(),
        },
        _ => return None,
    };
    Some(copy)
}

pub(crate) struct SuspensionCopy {
    pub(crate) title: &'static str,
    pub(crate) message: Option<String>,
    pub(crate) context: &'static str,
    pub(crate) guidance: &'static str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicPrompt {
    schema_version: u64,
    title_key: String,
    message_text: Option<String>,
    action_label_key: String,
    cancel_label_key: Option<String>,
}

pub(crate) fn suspension_copy(value: Option<&SuspensionView>) -> SuspensionCopy {
    let kind = value.map(|item| item.kind.as_str()).unwrap_or_default();
    let prompt = value
        .filter(|item| item.prompt_schema == "garive.public-suspension-prompt.v1")
        .and_then(|item| parse_prompt(&item.prompt_json));
    let guidance = value
        .and_then(|item| item.response_schema_json.as_deref())
        .map(describe_schema)
        .unwrap_or("Enter a response to continue this Turn.");
    SuspensionCopy {
        title: match kind {
            "approval_required" => "Approval required",
            "external_input_required" => "Input required",
            "operator_reconciliation" => "Operator review required",
            "resource_unavailable" => "Resource unavailable",
            "partial_output" => "Partial output available",
            "delegation_pending" => "Delegated work pending",
            _ => "Action required",
        },
        message: prompt.and_then(|item| item.message_text),
        context: match kind {
            "approval_required" => "Review the public request before responding.",
            "external_input_required" => "Garive needs public input before it can continue.",
            "operator_reconciliation" => "Durable state requires operator reconciliation.",
            "resource_unavailable" => "The required resource is not currently available.",
            "partial_output" => "The Agent paused after committing partial output.",
            "delegation_pending" => "Delegated work has not reached a durable terminal state.",
            _ => "The selected Turn cannot continue without attention.",
        },
        guidance,
    }
}

fn parse_prompt(value: &str) -> Option<PublicPrompt> {
    let prompt: PublicPrompt = serde_json::from_str(value).ok()?;
    (prompt.schema_version == 1
        && !prompt.title_key.is_empty()
        && !prompt.action_label_key.is_empty()
        && prompt.message_text.as_deref() != Some("")
        && prompt.cancel_label_key.as_deref() != Some(""))
    .then_some(prompt)
}

pub(crate) fn activity_copy(
    kind: &str,
    label_key: &str,
    state: &str,
    safe_code: Option<&str>,
) -> (String, TimelineTone) {
    let label = match (kind, label_key) {
        ("interaction", "agent.activity.approval") => "Approval",
        ("interaction", "agent.activity.external_input") => "Input request",
        ("tool", _) => "Agent action",
        _ => "Activity updated",
    };
    let (state_label, tone) = match state {
        "prepared" | "authorized" => ("prepared", TimelineTone::Neutral),
        "waiting_for_input" => ("waiting for input", TimelineTone::Warning),
        "input_received" => ("input received", TimelineTone::Success),
        "running" => ("running", TimelineTone::Active),
        "completed" => ("completed", TimelineTone::Success),
        "denied" => ("denied", TimelineTone::Warning),
        "failed" => ("failed", TimelineTone::Danger),
        "cancelled" => ("cancelled", TimelineTone::Neutral),
        "attention_required" => ("attention required", TimelineTone::Warning),
        _ => ("updated", TimelineTone::Neutral),
    };
    let code = safe_code
        .map(|value| format!(" · {value}"))
        .unwrap_or_default();
    (format!("{label} · {state_label}{code}"), tone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_prompt_is_structured_without_rendering_localization_keys() {
        let value = SuspensionView {
            suspension_id: "s".into(),
            session_version: 1,
            kind: "approval_required".into(),
            prompt_schema: "garive.public-suspension-prompt.v1".into(),
            prompt_json: r#"{"schema_version":1,"title_key":"approval.title","message_text":"Create one file.","action_label_key":"approval.allow"}"#.into(),
            prompt_digest: "0".repeat(64),
            response_schema_json: Some(r#"{"type":"boolean"}"#.into()),
            response_schema_digest: Some("1".repeat(64)),
        };
        let copy = suspension_copy(Some(&value));
        assert_eq!(copy.title, "Approval required");
        assert_eq!(copy.message.as_deref(), Some("Create one file."));
        assert_eq!(copy.guidance, "Enter true or false.");
    }

    #[test]
    fn activity_copy_never_exposes_unknown_localization_keys() {
        let (text, tone) = activity_copy("future", "private.tool.name", "future", None);
        assert_eq!(text, "Activity updated · updated");
        assert_eq!(tone, TimelineTone::Neutral);
    }
}
