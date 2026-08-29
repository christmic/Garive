//! Validated Agent policy values; environment and file loading live in Runtime.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod definition;
mod intent;
mod snapshot;

pub use definition::{
    CapabilityKind, CapabilityReference, ContextPolicyReference, DefaultLimits, DefaultUnmatched,
    GovernancePolicy, InstructionReference, InteractionMode, ModelRoleRequirement, ResolutionError,
    ResolutionErrorCode,
};
pub use intent::AgentDefinition;
pub use snapshot::{
    CapabilityDescriptor, ContextPolicyCandidate, EffectiveAgentSnapshot,
    EffectiveCapabilitySnapshot, EffectiveGovernancePolicy, EffectiveLimits,
    GovernancePolicyCandidate, InstructionResource, ModelRoleCandidate, ProductPolicy,
    ResolutionRegistry, ResolvedContextPolicy, ResolvedInstruction, ResolvedModelRole,
};
