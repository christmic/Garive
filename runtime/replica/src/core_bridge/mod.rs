//! Durable mapping between one disposable Core execution and Runtime facts.

mod encoding;
mod execution;
mod execution_types;
mod governed_effect;
mod governed_effect_types;
mod knowledge_connector;
mod knowledge_lifecycle;
mod knowledge_recovery;
mod memory_authority;
mod memory_hypothesis;
mod memory_hypothesis_recovery;
mod memory_maintenance;
mod memory_maintenance_projection;
mod memory_maintenance_recovery;
mod memory_recovery;
mod memory_repository_recovery;
mod memory_retrieval;
mod memory_write;
mod model_lifecycle;
mod scheduler_lifecycle;
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
pub use knowledge_connector::{
    KnowledgeAccessGrant, KnowledgeConnector, KnowledgeConnectorFuture, KnowledgeConnectorOutcome,
    PreparedKnowledgeCapability,
};
pub use knowledge_lifecycle::{
    plan_knowledge_completed, plan_knowledge_dispatched, plan_knowledge_failed,
    plan_knowledge_requested, KnowledgeFailurePhase, KnowledgeFailureReason,
    KnowledgeLifecycleContext, PlannedKnowledgeCompletion, PreparedKnowledgeRequest,
};
pub use knowledge_recovery::{
    derive_knowledge_recovery, KnowledgeRecoveryAction, KnowledgeRecoveryContext,
};
pub use memory_authority::{authorize_memory_query, authorize_memory_write, MemoryAccessGrant};
pub use memory_hypothesis::{
    decode_committed_memory_recall, plan_memory_obligation, plan_memory_observation,
    plan_memory_recall, MemoryObligationContext, MemoryObservationContext, MemoryRecallContext,
    PlannedMemoryObservation, PlannedMemoryRecall,
};
pub use memory_hypothesis_recovery::{
    reconstruct_memory_hypothesis_projection, MemoryHypothesisProjection, RecordedMemoryRecall,
};
pub use memory_maintenance::{
    plan_memory_audit, plan_memory_distillation_checkpoint, plan_memory_erasure_receipt,
    plan_memory_forget, plan_memory_maintenance_decision, plan_memory_promotion_receipt,
    plan_memory_promotion_request, MemoryMaintenanceContext, PlannedMemoryForget,
    PlannedMemoryPromotion,
};
pub use memory_maintenance_projection::{
    MemoryMaintenanceProjection, RecordedMemoryDecision, RecordedMemoryErasure,
};
pub use memory_maintenance_recovery::reconstruct_memory_maintenance_projection;
pub use memory_recovery::{reconstruct_memory_state, verify_memory_evidence, MemoryPrefix};
pub use memory_repository_recovery::{
    reconstruct_memory_repository, reconstruct_memory_repository_projection,
    RecoveredMemoryRepository,
};
pub use memory_retrieval::{plan_memory_retrieval, MemoryRetrievalContext, PlannedMemoryRetrieval};
pub use memory_write::{
    plan_classified_memory_write, plan_memory_tombstone, plan_memory_write, MemoryTombstoneContext,
    MemoryTombstoneReason, MemoryWriteContext, MemoryWriteDecision, MemoryWriteRejection,
    PlannedMemoryTombstone, PlannedMemoryWrite,
};
pub use model_lifecycle::{
    plan_model_prepared, plan_model_started, plan_model_terminal, plan_model_uncertain,
    ModelLifecycleContext, PreparedModelRequest, RuntimeModelUncertainReason,
};
pub use scheduler_lifecycle::{
    plan_schedule_cancelled, plan_schedule_claimed, plan_schedule_created, plan_schedule_exhausted,
    plan_schedule_failed, plan_schedule_fired, plan_schedule_skipped, ScheduleCancelReason,
    ScheduleDispatchDisposition, ScheduleLifecycleContext,
};
pub use skill_activation::{plan_skill_activation, PlannedSkillActivation, SkillActivationContext};
pub use terminal::{plan_core_terminal, CoreTerminalContext};
