//! Validated Agent policy values; environment and file loading live in Runtime.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod definition;
mod intent;
mod resolve;
mod snapshot;

pub use definition::{
    CapabilityKind, CapabilityReference, ContextPolicyReference, DefaultLimits, DefaultUnmatched,
    GovernancePolicy, InstructionReference, InteractionMode, ModelRoleRequirement, ResolutionError,
    ResolutionErrorCode,
};
pub use intent::AgentDefinition;
pub use resolve::{digest_canonical_value, resolve_definition};
pub use snapshot::{
    CapabilityDescriptor, ContextPolicyCandidate, EffectiveAgentSnapshot,
    EffectiveCapabilitySnapshot, EffectiveGovernancePolicy, EffectiveLimits,
    GovernancePolicyCandidate, InstructionResource, ModelRoleCandidate, ProductPolicy,
    PublicToolActivityCatalogue, PublicToolActivityDescriptor, ResolutionRegistry,
    ResolvedContextPolicy, ResolvedInstruction, ResolvedModelRole,
};
