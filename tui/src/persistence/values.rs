use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{MouseMode, Theme};

use super::store::StateError;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Draft {
    pub(crate) session_id: String,
    pub(crate) text: String,
    pub(crate) updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Preferences {
    pub(crate) schema_version: u8,
    pub(crate) revision: u64,
    pub(crate) theme: Theme,
    pub(crate) reduced_motion: bool,
    pub(crate) mouse: MouseMode,
    pub(crate) selected_session_id: Option<String>,
    pub(crate) session_rail: bool,
    pub(crate) activity_inspector: bool,
    pub(crate) bell: bool,
    pub(crate) persist_drafts: bool,
    pub(crate) drafts: Vec<Draft>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            theme: Theme::System,
            reduced_motion: false,
            mouse: MouseMode::Auto,
            selected_session_id: None,
            session_rail: true,
            activity_inspector: false,
            bell: true,
            persist_drafts: true,
            drafts: Vec::new(),
        }
    }
}

impl Preferences {
    pub(crate) fn validate(&self) -> Result<(), StateError> {
        let bytes = self
            .drafts
            .iter()
            .map(|value| value.text.len())
            .sum::<usize>();
        let mut sessions = std::collections::BTreeSet::new();
        if self.schema_version != 1
            || self.drafts.len() > 32
            || bytes > 65_536
            || self.drafts.iter().any(|value| {
                value.session_id.is_empty()
                    || value.text.len() > 4_096
                    || !valid_time(&value.updated_at)
                    || !sessions.insert(&value.session_id)
            })
        {
            return Err(StateError::InvalidData);
        }
        Ok(())
    }

    pub(crate) fn draft(&self, session_id: &str) -> Option<&str> {
        self.drafts
            .iter()
            .find(|value| value.session_id == session_id)
            .map(|value| value.text.as_str())
    }

    pub(crate) fn set_draft(&mut self, session_id: &str, text: &str) {
        self.drafts.retain(|value| value.session_id != session_id);
        if !text.is_empty() && self.persist_drafts {
            self.drafts.push(Draft {
                session_id: session_id.into(),
                text: text.into(),
                updated_at: now(),
            });
            self.drafts
                .sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
            self.drafts.truncate(32);
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PendingKind {
    CreateSession,
    StartTurn,
    CancelTurn,
    ContinueTurn,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingCommand {
    pub(crate) schema_version: u8,
    pub(crate) command_id: String,
    pub(crate) kind: PendingKind,
    pub(crate) session_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) suspension_id: Option<String>,
    pub(crate) expected_session_version: Option<u64>,
    pub(crate) requested_through_position: Option<u64>,
    pub(crate) request_payload: Value,
    pub(crate) request_digest: String,
    pub(crate) created_at: String,
}

impl PendingCommand {
    pub(crate) fn seal(mut self) -> Result<Self, StateError> {
        self.request_digest.clear();
        self.request_digest = digest_pending(&self)?;
        self.validate()?;
        Ok(self)
    }

    pub(crate) fn validate(&self) -> Result<(), StateError> {
        let valid_identity =
            |value: &str| !value.is_empty() && value.len() <= 128 && value.is_ascii();
        if self.schema_version != 1
            || !valid_identity(&self.command_id)
            || !valid_time(&self.created_at)
            || self.request_digest.len() != 64
            || self.request_digest != digest_pending(self)?
        {
            return Err(StateError::InvalidData);
        }
        Ok(())
    }
}

fn digest_pending(value: &PendingCommand) -> Result<String, StateError> {
    let mut digest_value = value.clone();
    digest_value.request_digest.clear();
    digest_value.created_at.clear();
    let canonical = serde_jcs::to_vec(&digest_value).map_err(|_| StateError::InvalidData)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromptHistoryEntry {
    pub(crate) schema_version: u8,
    pub(crate) entry_id: String,
    pub(crate) session_id: String,
    pub(crate) submitted_text: String,
    pub(crate) submitted_at: String,
}

impl PromptHistoryEntry {
    pub(crate) fn validate(&self) -> Result<(), StateError> {
        if self.schema_version != 1
            || self.entry_id.is_empty()
            || self.session_id.is_empty()
            || self.submitted_text.is_empty()
            || self.submitted_text.len() > 4_096
            || !valid_time(&self.submitted_at)
        {
            return Err(StateError::InvalidData);
        }
        Ok(())
    }
}

pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn valid_time(value: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(value).is_ok()
}
