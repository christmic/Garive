use crate::{
    values::{valid_digest, valid_id, valid_text, MAX_REFERENCE_BYTES},
    HypothesisState, LifecycleEvent, MemoryError, MemoryErrorCode, MemoryLifecycle, MemoryType,
};

/// Frozen policy that admits a Memory-to-Knowledge promotion request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryPromotionPolicy {
    revision: String,
    allowed_types: Vec<MemoryType>,
    min_verified: u64,
    max_falsified: u64,
    min_helpful_uses: u64,
}

impl MemoryPromotionPolicy {
    /// Validates a non-empty canonical type set and explicit thresholds.
    pub fn new(
        revision: impl Into<String>,
        allowed_types: Vec<MemoryType>,
        min_verified: u64,
        max_falsified: u64,
        min_helpful_uses: u64,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            revision: revision.into(),
            allowed_types,
            min_verified,
            max_falsified,
            min_helpful_uses,
        };
        if !valid_text(&value.revision, MAX_REFERENCE_BYTES)
            || value.allowed_types.is_empty()
            || !value.allowed_types.windows(2).all(|pair| pair[0] < pair[1])
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }
    /// Returns the immutable policy revision.
    pub fn revision(&self) -> &str {
        &self.revision
    }
}

/// Opaque request binding one eligible Memory revision to a Knowledge proposal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryPromotionRequest {
    request_id: String,
    namespace_id: String,
    record_id: String,
    revision_id: String,
    memory_type: MemoryType,
    policy_revision: String,
    knowledge_proposal_id: String,
    evidence_digest: String,
}

impl MemoryPromotionRequest {
    /// Returns the request identity.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    /// Returns the authorized namespace.
    pub fn namespace_id(&self) -> &str {
        &self.namespace_id
    }
    /// Returns the exact record identity.
    pub fn record_id(&self) -> &str {
        &self.record_id
    }
    /// Returns the exact revision identity.
    pub fn revision_id(&self) -> &str {
        &self.revision_id
    }
    /// Returns the cognitive memory type.
    pub const fn memory_type(&self) -> MemoryType {
        self.memory_type
    }
    /// Returns the frozen promotion policy revision.
    pub fn policy_revision(&self) -> &str {
        &self.policy_revision
    }
    /// Returns the opaque Knowledge proposal identity.
    pub fn knowledge_proposal_id(&self) -> &str {
        &self.knowledge_proposal_id
    }
    /// Returns the evidence package digest.
    pub fn evidence_digest(&self) -> &str {
        &self.evidence_digest
    }
}

/// Checks policy eligibility and produces a request without publishing Knowledge.
#[allow(clippy::too_many_arguments)]
pub fn request_memory_promotion(
    request_id: impl Into<String>,
    namespace_id: impl Into<String>,
    record_id: impl Into<String>,
    revision_id: impl Into<String>,
    memory_type: MemoryType,
    lifecycle: &MemoryLifecycle,
    helpful_uses: u64,
    policy: &MemoryPromotionPolicy,
    knowledge_proposal_id: impl Into<String>,
    evidence_digest: impl Into<String>,
) -> Result<MemoryPromotionRequest, MemoryError> {
    let request = MemoryPromotionRequest {
        request_id: request_id.into(),
        namespace_id: namespace_id.into(),
        record_id: record_id.into(),
        revision_id: revision_id.into(),
        memory_type,
        policy_revision: policy.revision.clone(),
        knowledge_proposal_id: knowledge_proposal_id.into(),
        evidence_digest: evidence_digest.into(),
    };
    if [
        &request.request_id,
        &request.namespace_id,
        &request.record_id,
        &request.revision_id,
        &request.knowledge_proposal_id,
    ]
    .iter()
    .any(|value| !valid_id(value))
        || !valid_digest(&request.evidence_digest)
    {
        return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
    }
    let tally = lifecycle.tally();
    if !matches!(
        lifecycle.state(),
        HypothesisState::Active | HypothesisState::Cold
    ) || !policy.allowed_types.contains(&memory_type)
        || tally.verified < policy.min_verified
        || tally.falsified > policy.max_falsified
        || helpful_uses < policy.min_helpful_uses
    {
        return Err(MemoryError::new(MemoryErrorCode::PromotionNotEligible));
    }
    Ok(request)
}

/// Receipt-shaped proof that Knowledge published the exact proposal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryPromotionReceipt {
    request_id: String,
    knowledge_proposal_id: String,
    knowledge_record_id: String,
    knowledge_revision_id: String,
    receipt_digest: String,
}

impl MemoryPromotionReceipt {
    /// Validates receipt identity and digest shape; Runtime verifies authenticity.
    pub fn new(
        request_id: impl Into<String>,
        knowledge_proposal_id: impl Into<String>,
        knowledge_record_id: impl Into<String>,
        knowledge_revision_id: impl Into<String>,
        receipt_digest: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            request_id: request_id.into(),
            knowledge_proposal_id: knowledge_proposal_id.into(),
            knowledge_record_id: knowledge_record_id.into(),
            knowledge_revision_id: knowledge_revision_id.into(),
            receipt_digest: receipt_digest.into(),
        };
        if [
            &value.request_id,
            &value.knowledge_proposal_id,
            &value.knowledge_record_id,
            &value.knowledge_revision_id,
        ]
        .iter()
        .any(|item| !valid_id(item))
            || !valid_digest(&value.receipt_digest)
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }
    /// Returns the originating Memory promotion request identity.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    /// Returns the exact Knowledge proposal identity.
    pub fn knowledge_proposal_id(&self) -> &str {
        &self.knowledge_proposal_id
    }
    /// Returns the Knowledge record identity.
    pub fn knowledge_record_id(&self) -> &str {
        &self.knowledge_record_id
    }
    /// Returns the Knowledge revision identity.
    pub fn knowledge_revision_id(&self) -> &str {
        &self.knowledge_revision_id
    }
    /// Returns the verified publication receipt digest.
    pub fn receipt_digest(&self) -> &str {
        &self.receipt_digest
    }
}

/// Verifies receipt bindings and produces the Promoted lifecycle projection.
pub fn complete_memory_promotion(
    request: &MemoryPromotionRequest,
    receipt: &MemoryPromotionReceipt,
    lifecycle: &MemoryLifecycle,
    position: u64,
) -> Result<MemoryLifecycle, MemoryError> {
    if receipt.request_id != request.request_id
        || receipt.knowledge_proposal_id != request.knowledge_proposal_id
    {
        return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
    }
    if position <= lifecycle.last_observed_position() {
        return Err(MemoryError::new(MemoryErrorCode::InvalidTransition));
    }
    lifecycle.apply(LifecycleEvent::Promote {
        position,
        receipt_digest: Some(receipt.receipt_digest.clone()),
    })
}
