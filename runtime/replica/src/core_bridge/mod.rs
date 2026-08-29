//! Durable mapping between one disposable Core execution and Runtime facts.

mod encoding;
mod execution;
mod execution_types;
mod governed_effect;
mod governed_effect_types;
mod memory_retrieval;
mod memory_write;
mod model_lifecycle;
mod skill_activation;
mod terminal;

pub use encoding::canonical_model_request_digest;
pub use execution::{
    execute_durable_agent, execute_durable_agent_with_capabilities,
    execute_durable_agent_with_skill_activation, execute_durable_model_only,
    execute_durable_model_only_with_capabilities, execute_durable_model_only_with_skill_activation,
    PreparedAgentCapabilities,
};
pub use execution_types::{
    DurableExecutionConfig, DurableExecutionError, DurableExecutionResult,
    TerminalPublicationError, TerminalPublisher,
};
pub use governed_effect::SqliteGovernedEffectPort;
pub use governed_effect_types::{
    AuthorityDecision, AuthorityFuture, AuthorityPort, AuthorityRequest, ExecutorDispatch,
    ExecutorDispatchError, ExecutorFuture, ExecutorPort, GovernedEffectConfig,
    GovernedRuntimePortError, PreparedExecution,
};
pub use memory_retrieval::{plan_memory_retrieval, MemoryRetrievalContext, PlannedMemoryRetrieval};
pub use memory_write::{
    plan_memory_tombstone, plan_memory_write, MemoryTombstoneContext, MemoryTombstoneReason,
    MemoryWriteContext, MemoryWriteDecision, MemoryWriteRejection, PlannedMemoryTombstone,
    PlannedMemoryWrite,
};
pub use model_lifecycle::{
    plan_model_prepared, plan_model_started, plan_model_terminal, plan_model_uncertain,
    ModelLifecycleContext, PreparedModelRequest, RuntimeModelUncertainReason,
};
pub use skill_activation::{plan_skill_activation, PlannedSkillActivation, SkillActivationContext};
pub use terminal::{plan_core_terminal, CoreTerminalContext};
