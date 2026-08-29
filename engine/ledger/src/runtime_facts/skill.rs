use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{digest, enumeration, fields, non_empty, object, unsigned, EMPTY};

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    if kind != "skill.activated" {
        return Err(LedgerError::InvalidFact);
    }
    fields(
        value,
        &[
            "activation_id",
            "request_digest",
            "mode",
            "through_position",
            "skills",
            "truncated",
        ],
        EMPTY,
    )?;
    non_empty(value, "activation_id")?;
    digest(value, "request_digest")?;
    let mode = enumeration(value, "mode", &["explicit", "tagged"])?;
    unsigned(value, "through_position", false)?;
    let skills = value
        .get("skills")
        .and_then(Value::as_array)
        .ok_or(LedgerError::InvalidFact)?;
    for skill in skills {
        let skill = object(skill)?;
        fields(
            skill,
            &[
                "skill_id",
                "skill_revision",
                "definition_digest",
                "instruction_digest",
                "reason",
            ],
            EMPTY,
        )?;
        non_empty(skill, "skill_id")?;
        non_empty(skill, "skill_revision")?;
        digest(skill, "definition_digest")?;
        digest(skill, "instruction_digest")?;
        let reason = enumeration(skill, "reason", &["explicit", "tag_match"])?;
        if (mode == "explicit") != (reason == "explicit") {
            return Err(LedgerError::InvalidFact);
        }
    }
    let truncated = value
        .get("truncated")
        .and_then(Value::as_bool)
        .ok_or(LedgerError::InvalidFact)?;
    if mode == "explicit" && (skills.len() != 1 || truncated) {
        return Err(LedgerError::InvalidFact);
    }
    Ok(())
}
