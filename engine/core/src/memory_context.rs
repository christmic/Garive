use std::collections::BTreeSet;

use garive_llm::{ModelInputContent, ModelInputItem, ModelRole};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    derive_context, CandidateKind, ContextCandidate, ContextPurpose, ContextRequest,
    ContextSurface, FactRef, Retention, Visibility,
};

/// Committed recall product entering deterministic context derivation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MemoryRecallProduct {
    /// Redacted descriptors without stored content.
    Menu,
    /// Explicit resolved content.
    Detail,
}

/// Lifecycle state admitted to model-visible recall.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryContextState {
    /// Unverified hypothesis admitted only by explicit exploration.
    Candidate,
    /// Ordinary active hypothesis.
    Active,
    /// Deprioritized but recallable hypothesis.
    Cold,
    /// Detail-only retained hypothesis.
    Archived,
}

/// One exact item from a committed Memory recall fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryContextItem {
    /// Stable logical identity.
    pub record_id: String,
    /// Exact immutable revision.
    pub revision_id: String,
    /// Stable cognitive type wire name.
    pub memory_type: String,
    /// Stable semantic role wire name.
    pub role: String,
    /// Stable authority wire name.
    pub authority: String,
    /// Committed lifecycle state.
    pub state: MemoryContextState,
    /// Bounded non-sensitive descriptor.
    pub safe_label: String,
    /// SHA-256 digest of detail content.
    pub content_digest: String,
    /// Exact committed UTF-8 byte length.
    pub content_byte_length: u64,
    /// Runtime-resolved detail content; forbidden for menu.
    pub content_utf8: Option<String>,
}

/// Exact committed recall binding supplied by Runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRecallContextBatch {
    /// Durable recall fact reference.
    pub fact_ref: FactRef,
    /// Durable fact identity.
    pub fact_id: String,
    /// Canonical recall payload digest.
    pub payload_digest: String,
    /// Stable selection identity.
    pub selection_id: String,
    /// Digest over complete selection semantics.
    pub request_digest: String,
    /// Authorized namespace identity.
    pub namespace_id: String,
    /// Menu or detail product.
    pub product: MemoryRecallProduct,
    /// Exact selection policy revision.
    pub selection_policy_revision: String,
    /// Frozen source prefix used for recall.
    pub through_position: u64,
    /// Whether eligible results were omitted.
    pub truncated: bool,
    /// Ordered selected items.
    pub items: Vec<MemoryContextItem>,
}

/// Stable committed-recall integration failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryContextError {
    /// A recall binding or item is malformed.
    InvalidBinding,
    /// More than one menu/detail or a repeated memory identity was supplied.
    DuplicateRecall,
    /// The normal context derivation failed.
    Context(ContextErrorCode),
}

/// Stable reduced C2 failure nested in Memory integration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextErrorCode(pub &'static str);

/// Adapts committed recall into ordinary optional C2 candidates and derives one surface.
pub fn derive_context_with_memory(
    request: &ContextRequest,
    candidates: &[ContextCandidate],
    recalls: &[MemoryRecallContextBatch],
) -> Result<ContextSurface, MemoryContextError> {
    if candidates
        .windows(2)
        .any(|pair| pair[0].fact_ref.position >= pair[1].fact_ref.position)
    {
        let code = if candidates
            .windows(2)
            .any(|pair| pair[0].fact_ref.position == pair[1].fact_ref.position)
        {
            "duplicate-reference"
        } else {
            "non-increasing-position"
        };
        return Err(MemoryContextError::Context(ContextErrorCode(code)));
    }
    if recalls.len() > 2 {
        return Err(MemoryContextError::DuplicateRecall);
    }
    let mut products = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut combined = candidates.to_vec();
    for recall in recalls {
        if !products.insert(recall.product) {
            return Err(MemoryContextError::DuplicateRecall);
        }
        validate_batch(request, recall, &mut identities)?;
        if !recall.items.is_empty() {
            combined.push(candidate(recall));
        }
    }
    combined.sort_by_key(|value| value.fact_ref.position);
    derive_context(request, &combined)
        .map_err(|error| MemoryContextError::Context(ContextErrorCode(error.code())))
}

fn validate_batch(
    request: &ContextRequest,
    recall: &MemoryRecallContextBatch,
    identities: &mut BTreeSet<(String, String)>,
) -> Result<(), MemoryContextError> {
    if recall.fact_ref.session_id != request.session_id
        || recall.fact_ref.position <= recall.through_position
        || recall.fact_ref.position > request.through_position
        || recall.fact_id.is_empty()
        || !digest_valid(&recall.payload_digest)
        || recall.selection_id.is_empty()
        || !digest_valid(&recall.request_digest)
        || recall.namespace_id.is_empty()
        || recall.selection_policy_revision.is_empty()
    {
        return Err(MemoryContextError::InvalidBinding);
    }
    for item in &recall.items {
        if item.record_id.is_empty()
            || item.revision_id.is_empty()
            || !matches!(
                item.memory_type.as_str(),
                "semantic" | "episodic" | "lesson" | "procedural"
            )
            || !matches!(
                item.role.as_str(),
                "preference" | "constraint" | "decision" | "learned_fact" | "summary"
            )
            || !matches!(
                item.authority.as_str(),
                "user_declared" | "agent_learned" | "organisation_published"
            )
            || item.safe_label.is_empty()
            || !digest_valid(&item.content_digest)
            || item.content_byte_length == 0
            || !identities.insert((item.record_id.clone(), item.revision_id.clone()))
            || item.state == MemoryContextState::Archived
                && recall.product == MemoryRecallProduct::Menu
        {
            return Err(MemoryContextError::InvalidBinding);
        }
        match (&recall.product, &item.content_utf8) {
            (MemoryRecallProduct::Menu, None) => {}
            (MemoryRecallProduct::Detail, Some(content))
                if !content.is_empty()
                    && content.len() as u64 == item.content_byte_length
                    && sha256(content) == item.content_digest => {}
            _ => return Err(MemoryContextError::InvalidBinding),
        }
    }
    Ok(())
}

fn candidate(recall: &MemoryRecallContextBatch) -> ContextCandidate {
    ContextCandidate {
        fact_ref: recall.fact_ref.clone(),
        kind: CandidateKind::Memory,
        retention: Retention::Optional,
        visibility: Visibility::Purposes(BTreeSet::from([ContextPurpose::Inference])),
        items: recall
            .items
            .iter()
            .map(|item| render(recall, item))
            .collect(),
    }
}

fn render(recall: &MemoryRecallContextBatch, item: &MemoryContextItem) -> ModelInputItem {
    let content = json!({
        "type": "garive.memory.recall",
        "selection_id": recall.selection_id,
        "request_digest": recall.request_digest,
        "namespace_id": recall.namespace_id,
        "product": match recall.product { MemoryRecallProduct::Menu => "menu", MemoryRecallProduct::Detail => "detail" },
        "selection_policy_revision": recall.selection_policy_revision,
        "recall_fact": { "session_id": recall.fact_ref.session_id, "position": recall.fact_ref.position,
            "fact_id": recall.fact_id, "payload_digest": recall.payload_digest },
        "record_id": item.record_id, "revision_id": item.revision_id,
        "memory_type": item.memory_type, "role": item.role, "authority": item.authority,
        "state": state_name(item.state), "safe_label": item.safe_label,
        "content_digest": item.content_digest, "content_byte_length": item.content_byte_length,
        "content": item.content_utf8,
    });
    ModelInputItem::Message {
        role: ModelRole::User,
        content: vec![ModelInputContent::Text(content.to_string())],
    }
}

const fn state_name(value: MemoryContextState) -> &'static str {
    match value {
        MemoryContextState::Candidate => "candidate",
        MemoryContextState::Active => "active",
        MemoryContextState::Cold => "cold",
        MemoryContextState::Archived => "archived",
    }
}

fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
