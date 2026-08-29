//! Skill discovery, selection, and invocation contracts for the Agent kernel.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod definition;
mod reducer;

pub use definition::{
    ActivationPolicy, CapabilityReference, ContentBinding, ExactToolReference, SkillDefinition,
    SkillError, SkillErrorCode,
};
pub use reducer::{
    activate_skills, ActivatedSkill, ActivationMode, ActivationReason, SkillActivationRequest,
    SkillActivationResult,
};
