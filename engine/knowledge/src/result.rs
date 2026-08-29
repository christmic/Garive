use std::collections::BTreeSet;

use crate::{
    FreshnessRequirement, KnowledgeError, KnowledgeErrorCode, KnowledgeEvidence,
    KnowledgeFreshness, KnowledgeRequest, KnowledgeSourceDescriptor,
};

/// Bounded normalized completed Knowledge result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeCompleted {
    /// Ordered exact evidence admitted by both request bounds.
    pub evidence: Vec<KnowledgeEvidence>,
    /// Whether a canonical eligible suffix was omitted.
    pub truncated: bool,
}

/// Validates source/freshness bindings, normalizes order and applies prefix bounds.
pub fn complete_knowledge(
    request: &KnowledgeRequest,
    source: &KnowledgeSourceDescriptor,
    mut evidence: Vec<KnowledgeEvidence>,
    connector_order_stable: bool,
) -> Result<KnowledgeCompleted, KnowledgeError> {
    request.validate_source(source)?;
    let mut identities = BTreeSet::new();
    for item in &evidence {
        if !identities.insert(item.evidence_id())
            || item.source_id() != request.source_id()
            || item.source_revision() != request.source_revision()
            || item.trust_class() != source.trust_class()
            || item.citation().locator_kind() != source.citation_scheme()
            || !freshness_allowed(request.freshness_requirement(), item)
        {
            return Err(KnowledgeError::new(KnowledgeErrorCode::InvalidQuery));
        }
    }
    if !connector_order_stable {
        evidence.sort_by(|left, right| {
            right
                .rank_basis_points()
                .cmp(&left.rank_basis_points())
                .then_with(|| left.citation().locator().cmp(right.citation().locator()))
                .then_with(|| left.evidence_id().cmp(right.evidence_id()))
        });
    }
    let mut admitted = Vec::new();
    let mut bytes = 0_u64;
    let mut truncated = false;
    for item in evidence {
        let Some(next) = bytes.checked_add(item.content_byte_length()) else {
            truncated = true;
            break;
        };
        if admitted.len() == request.max_chunks() as usize || next > request.max_total_bytes() {
            truncated = true;
            break;
        }
        bytes = next;
        admitted.push(item);
    }
    Ok(KnowledgeCompleted {
        evidence: admitted,
        truncated,
    })
}

fn freshness_allowed(requirement: &FreshnessRequirement, value: &KnowledgeEvidence) -> bool {
    match requirement {
        FreshnessRequirement::CachedAllowed => true,
        FreshnessRequirement::Revalidate => value.freshness() == KnowledgeFreshness::Fresh,
        FreshnessRequirement::ExactSnapshot { snapshot_digest } => {
            value.freshness() != KnowledgeFreshness::Stale
                && value.source_snapshot_digest() == Some(snapshot_digest)
        }
    }
}
