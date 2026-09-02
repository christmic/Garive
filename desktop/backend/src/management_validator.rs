//! Validator bridging the Runtime management port to the Desktop registries.
//!
//! `BuiltinManagementValidator` is the strict counterpart of the runtime
//! crate's permissive default. It allows only the two current official
//! Provider profiles (matching `BuiltinDesktopProfileRegistry`) and the
//! two current official Agent definitions (matching the two built-in
//! `builtin_desktop_*_agent_installation` factories).
//!
//! New profiles / agents MUST add their identifiers here as part of the
//! same change that registers them in the corresponding Desktop
//! Registry; otherwise the management port silently rejects their
//! commits with `management_profile_unknown` /
//! `management_definition_unknown`.

use std::sync::Arc;

use garive_runtime::{ManagementCommitBody, ManagementConfigError, ManagementValidator};

use crate::desktop_agent::{DESKTOP_AGENT_REVISION, DESKTOP_WORKSPACE_AGENT_REVISION};
use crate::system_provider::{ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID};

/// Strict validator backed by the current Desktop built-in Registries.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltinManagementValidator;

impl ManagementValidator for BuiltinManagementValidator {
    fn validate(&self, body: &ManagementCommitBody) -> Result<(), ManagementConfigError> {
        match body.profile_id.as_str() {
            OPENAI_RESPONSES_PROFILE_ID | ANTHROPIC_MESSAGES_PROFILE_ID => {}
            _ => return Err(ManagementConfigError::ProfileUnknown),
        }
        match body.definition_id.as_str() {
            DESKTOP_AGENT_REVISION | DESKTOP_WORKSPACE_AGENT_REVISION => {}
            _ => return Err(ManagementConfigError::DefinitionUnknown),
        }
        Ok(())
    }
}

/// Boxed validator handle used by the binary when wiring a `LiveHost`.
pub fn builtin_management_validator() -> Arc<dyn ManagementValidator> {
    Arc::new(BuiltinManagementValidator)
}

// Suppress dead-code noise if a downstream crate imports the constant
// but not the function.
#[allow(dead_code)]
const _BUILTIN_REFERENCES_ARE_VALID: fn() = || {
    let _ = OPENAI_RESPONSES_PROFILE_ID;
    let _ = ANTHROPIC_MESSAGES_PROFILE_ID;
    let _ = DESKTOP_AGENT_REVISION;
    let _ = DESKTOP_WORKSPACE_AGENT_REVISION;
};
