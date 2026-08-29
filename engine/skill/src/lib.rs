//! Skill discovery, selection, and invocation contracts for the Agent kernel.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod definition;

pub use definition::{
    ActivationPolicy, CapabilityReference, ContentBinding, ExactToolReference, SkillDefinition,
    SkillError, SkillErrorCode,
};
