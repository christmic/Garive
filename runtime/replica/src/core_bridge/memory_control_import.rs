use garive_ledger::{DurableFact, FactDraft};
use garive_memory::{
    ContentBinding, MemoryAuthority, MemoryAuthorityBinding, MemoryCommit, MemoryControlDocument,
    MemoryDocumentLimits, MemoryErasureRequest, MemoryImportOperation, MemoryProposal,
    MemoryRevisionClassification, MemoryRevisionScope, MemoryScope, MemoryScopeClass, MemoryState,
    MemoryTombstone, MemoryTypeRegistry,
};

use crate::{
    MemoryControlAction, MemoryControlGrant, MemoryImportCommand, MemoryRepositoryError,
    MemoryRepositoryImportContext, SqliteLedger,
};

use super::{
    encoding::digest, plan_classified_memory_write, plan_memory_archive, plan_memory_forget,
    reconstruct_memory_repository, reconstruct_memory_state, MemoryMaintenanceContext,
    MemoryWriteContext,
};

/// Exact M0/M1 batch and reducer result for one confirmed M2 import.
pub struct PlannedMemoryRepositoryImport {
    /// Complete ordered fact batch committed with the M2 journal and projection.
    pub facts: Vec<FactDraft>,
    /// M0 state after every ordered operation.
    pub next_state: MemoryState,
    /// Physical erasure requests emitted by Erase operations in plan order.
    pub erasure_requests: Vec<MemoryErasureRequest>,
}

/// Revalidates one fixed repository prefix and plans every M2 operation as normal M0/M1 facts.
pub fn plan_memory_repository_import(
    ledger: &SqliteLedger,
    context: &MemoryRepositoryImportContext,
    command: &MemoryImportCommand,
    grant: &MemoryControlGrant,
    registry: &MemoryTypeRegistry,
    limits: MemoryDocumentLimits,
) -> Result<PlannedMemoryRepositoryImport, MemoryRepositoryError> {
    command
        .plan()
        .verify()
        .map_err(|_| MemoryRepositoryError::Corrupt)?;
    let plan = command.plan();
    if plan.expected_repository_revision != plan.through_revision {
        return Err(MemoryRepositoryError::Stale);
    }
    if !grant.admits_action(&plan.namespace_id, MemoryControlAction::Import) {
        return Err(MemoryRepositoryError::Unauthorized);
    }
    verify_authorization_fact(ledger, context)?;
    let recovered = reconstruct_memory_repository(
        ledger,
        &context.repository_prefixes,
        &plan.namespace_id,
        limits,
    )
    .map_err(|_| MemoryRepositoryError::Corrupt)?;
    if recovered.projection.repository_revision != plan.expected_repository_revision {
        return Err(MemoryRepositoryError::Stale);
    }
    let mut state = reconstruct_memory_state(ledger, &context.repository_prefixes)
        .map_err(|_| MemoryRepositoryError::Corrupt)?;
    let mut facts = Vec::new();
    let mut erasure_requests = Vec::new();
    for (ordinal, operation) in plan.operations.iter().enumerate() {
        let document = command
            .document_for_operation(operation)
            .map_err(|_| MemoryRepositoryError::Corrupt)?;
        let scope = garive_memory::MemoryAuthorizedScope {
            scope: document.scope(),
            owner_id: document.scope_owner_id().into(),
        };
        if !grant.admits(&plan.namespace_id, MemoryControlAction::Import, &scope) {
            return Err(MemoryRepositoryError::Unauthorized);
        }
        let through = context
            .through_position
            .checked_add(u64::try_from(facts.len()).map_err(|_| MemoryRepositoryError::Corrupt)?)
            .ok_or(MemoryRepositoryError::Corrupt)?;
        match operation {
            MemoryImportOperation::Add {
                record_id,
                revision_id,
                ..
            } => {
                let planned = plan_revision(
                    context,
                    command,
                    registry,
                    &state,
                    document,
                    record_id,
                    revision_id,
                    None,
                    ordinal,
                    through,
                )?;
                state = planned.next_state;
                facts.extend(planned.facts);
            }
            MemoryImportOperation::Supersede {
                record_id,
                expected_active_revision_id,
                new_revision_id,
                supersedes_learned_revision_id,
                ..
            } => {
                let current = recovered
                    .projection
                    .documents
                    .iter()
                    .find(|value| value.record_ref().record_id() == Some(record_id.as_str()))
                    .ok_or(MemoryRepositoryError::Stale)?;
                let learned = (current.authority() == MemoryAuthority::AgentLearned)
                    .then_some(expected_active_revision_id.as_str());
                if supersedes_learned_revision_id.as_deref() != learned {
                    return Err(MemoryRepositoryError::Corrupt);
                }
                let planned = plan_revision(
                    context,
                    command,
                    registry,
                    &state,
                    document,
                    record_id,
                    new_revision_id,
                    Some(expected_active_revision_id),
                    ordinal,
                    through,
                )?;
                state = planned.next_state;
                facts.extend(planned.facts);
            }
            MemoryImportOperation::Archive {
                record_id,
                expected_active_revision_id,
                ..
            } => {
                let lifecycle = recovered
                    .lifecycle(record_id, expected_active_revision_id)
                    .ok_or(MemoryRepositoryError::Stale)?;
                let position = through
                    .checked_add(1)
                    .ok_or(MemoryRepositoryError::Corrupt)?;
                let transition_id = operation_identity("m2-archive", command, ordinal);
                let planned = plan_memory_archive(
                    &transition_id,
                    &plan.namespace_id,
                    record_id,
                    expected_active_revision_id,
                    position,
                    &context.turn_id,
                    &context.execution_id,
                    &context.recorded_at,
                    lifecycle,
                )
                .map_err(|_| MemoryRepositoryError::Corrupt)?;
                facts.push(planned.fact);
            }
            MemoryImportOperation::Erase {
                record_id,
                expected_active_revision_id,
                ..
            } => {
                let policy = context
                    .policy
                    .erasure
                    .as_ref()
                    .ok_or(MemoryRepositoryError::Unauthorized)?;
                let planned = plan_memory_forget(
                    &MemoryMaintenanceContext {
                        session_id: context.session_id.clone(),
                        namespace_id: plan.namespace_id.clone(),
                        recorded_at: context.recorded_at.clone(),
                    },
                    through,
                    &operation_identity("m2-forget", command, ordinal),
                    &operation_identity("m2-erasure", command, ordinal),
                    &state,
                    &MemoryTombstone {
                        record_id: record_id.clone(),
                        revision_id: expected_active_revision_id.clone(),
                    },
                    &policy.policy_revision,
                    policy.targets.clone(),
                )
                .map_err(|_| MemoryRepositoryError::Corrupt)?;
                state = planned.next_state;
                erasure_requests.push(planned.request);
                facts.extend(planned.facts.into_iter().map(|mut fact| {
                    fact.turn_id = Some(context.turn_id.clone());
                    fact.execution_id = Some(context.execution_id.clone());
                    fact
                }));
            }
        }
    }
    Ok(PlannedMemoryRepositoryImport {
        facts,
        next_state: state,
        erasure_requests,
    })
}

#[allow(clippy::too_many_arguments)]
fn plan_revision(
    context: &MemoryRepositoryImportContext,
    command: &MemoryImportCommand,
    registry: &MemoryTypeRegistry,
    state: &MemoryState,
    document: &MemoryControlDocument,
    record_id: &str,
    revision_id: &str,
    prior_revision_id: Option<&String>,
    ordinal: usize,
    through_position: u64,
) -> Result<super::PlannedMemoryWrite, MemoryRepositoryError> {
    let bound = document
        .bind_existing_identity(record_id, revision_id, command.max_id_bytes())
        .map_err(|_| MemoryRepositoryError::Corrupt)?;
    if bound.authority() != MemoryAuthority::UserDeclared
        || bound.lifecycle() != garive_memory::HypothesisState::Active
    {
        return Err(MemoryRepositoryError::Corrupt);
    }
    let proposal = MemoryProposal::new(
        operation_identity("m2-proposal", command, ordinal),
        command.plan().namespace_id.clone(),
        source_scope(&bound),
        bound.memory_role(),
        ContentBinding::from_inline(bound.content()),
        vec![context.authorization_fact.clone()],
        bound.sensitivity(),
        context.policy.user_declared_confidence_basis_points,
        prior_revision_id.cloned(),
    )
    .map_err(|_| MemoryRepositoryError::Corrupt)?;
    let commit = MemoryCommit::new(
        record_id,
        revision_id,
        &context.policy.retention_policy_digest,
        through_position
            .checked_add(2)
            .ok_or(MemoryRepositoryError::Corrupt)?,
        None,
        prior_revision_id.cloned(),
    )
    .map_err(|_| MemoryRepositoryError::Corrupt)?;
    let aggregation = if bound.scope() == MemoryScopeClass::Platform {
        Some(
            context
                .policy
                .platform_aggregation_policy_digest
                .clone()
                .ok_or(MemoryRepositoryError::Unauthorized)?,
        )
    } else {
        None
    };
    let classification = MemoryRevisionClassification::new(
        bound.memory_role(),
        MemoryAuthorityBinding::new(
            MemoryAuthority::UserDeclared,
            Some(context.authority_receipt_digest.clone()),
        )
        .map_err(|_| MemoryRepositoryError::Unauthorized)?,
        MemoryRevisionScope::new(bound.scope(), bound.scope_owner_id(), aggregation)
            .map_err(|_| MemoryRepositoryError::Unauthorized)?,
        garive_memory::HypothesisState::Active,
        &context.policy.classification_policy_revision,
        registry,
    )
    .map_err(|_| MemoryRepositoryError::Corrupt)?;
    plan_classified_memory_write(
        &MemoryWriteContext {
            turn_id: context.turn_id.clone(),
            execution_id: context.execution_id.clone(),
            through_position,
            recorded_at: context.recorded_at.clone(),
        },
        state,
        &proposal,
        commit,
        &context.session_id,
        &operation_identity("m2-classification", command, ordinal),
        &classification,
    )
    .map_err(|_| MemoryRepositoryError::Corrupt)
}

fn source_scope(document: &MemoryControlDocument) -> MemoryScope {
    match document.scope() {
        MemoryScopeClass::Session => MemoryScope::Session {
            owner_id: document.scope_owner_id().into(),
        },
        MemoryScopeClass::AgentInstance => MemoryScope::AgentInstance {
            owner_id: document.scope_owner_id().into(),
        },
        MemoryScopeClass::User | MemoryScopeClass::Project | MemoryScopeClass::Platform => {
            MemoryScope::Namespace
        }
    }
}

fn verify_authorization_fact(
    ledger: &SqliteLedger,
    context: &MemoryRepositoryImportContext,
) -> Result<DurableFact, MemoryRepositoryError> {
    let facts = ledger
        .read_facts(
            &context.session_id,
            context.authorization_fact.position() - 1,
            context.authorization_fact.position(),
            None,
        )
        .map_err(|_| MemoryRepositoryError::Corrupt)?;
    let fact = facts
        .into_iter()
        .next()
        .ok_or(MemoryRepositoryError::Unauthorized)?;
    if fact.fact_id.as_str() != context.authorization_fact.fact_id()
        || fact.payload.sha256() != context.authorization_fact.payload_digest()
        || fact.turn_id.as_ref() != Some(&context.turn_id)
        || fact.execution_id.as_ref() != Some(&context.execution_id)
    {
        return Err(MemoryRepositoryError::Unauthorized);
    }
    Ok(fact)
}

fn operation_identity(prefix: &str, command: &MemoryImportCommand, ordinal: usize) -> String {
    format!(
        "{prefix}-{}",
        digest(format!("{}:{ordinal}", command.command_id()).as_bytes())
    )
}
