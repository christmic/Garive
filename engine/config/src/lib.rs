//! Validated Agent policy values; environment and file loading live in Runtime.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod definition;

pub use definition::{
    CapabilityKind, CapabilityReference, ContextPolicyReference, DefaultLimits, DefaultUnmatched,
    GovernancePolicy, InstructionReference, InteractionMode, ModelRoleRequirement, ResolutionError,
    ResolutionErrorCode,
};
