use serde::Serialize;
use sha2::{Digest, Sha256};

pub(crate) const MAX_ID_BYTES: usize = 128;
pub(crate) const MAX_REFERENCE_BYTES: usize = 512;
const SHA256_HEX_BYTES: usize = 64;

/// Stable M0 validation, authority, version, or durability failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryErrorCode {
    /// A value, identity, bound, timestamp, or relation is invalid.
    InvalidMemory,
    /// Runtime denied the opaque namespace or scope.
    NamespaceDenied,
    /// Referenced durable evidence does not exist in the fixed prefix.
    EvidenceNotFound,
    /// Evidence identity or payload digest does not match.
    EvidenceMismatch,
    /// Optimistic active revision or exact tombstone target conflicts.
    RevisionConflict,
    /// Runtime retention policy rejected the proposal.
    RetentionRejected,
    /// Restricted content lacks exact frozen authority.
    SensitivityDenied,
    /// A record or query exceeds an admitted bound.
    LimitExceeded,
    /// The requested memory operation is not admitted.
    Unsupported,
    /// A required durable commit failed.
    DurabilityFailure,
    /// Persisted memory state violates M0 invariants.
    CorruptMemoryState,
    /// M1 type, role, or registry revision is not admitted.
    UnknownMemoryType,
    /// Non-agent authority lacks a frozen receipt digest.
    AuthorityReceiptRequired,
    /// Platform scope lacks its aggregation policy binding.
    ScopePolicyDenied,
    /// Lifecycle event is not admitted from the exact prior state.
    InvalidTransition,
    /// Observation position is not strictly newer than the projection.
    DuplicateObservation,
    /// Promotion lacks a valid Knowledge publication receipt.
    PromotionReceiptRequired,
    /// Selection lacks an admitted deterministic or seeded replay contract.
    SelectionUnreplayable,
}

impl MemoryErrorCode {
    /// Returns the stable portable wire name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::InvalidMemory => "invalid_memory",
            Self::NamespaceDenied => "namespace_denied",
            Self::EvidenceNotFound => "evidence_not_found",
            Self::EvidenceMismatch => "evidence_mismatch",
            Self::RevisionConflict => "revision_conflict",
            Self::RetentionRejected => "retention_rejected",
            Self::SensitivityDenied => "sensitivity_denied",
            Self::LimitExceeded => "limit_exceeded",
            Self::Unsupported => "unsupported",
            Self::DurabilityFailure => "durability_failure",
            Self::CorruptMemoryState => "corrupt_memory_state",
            Self::UnknownMemoryType => "unknown_memory_type",
            Self::AuthorityReceiptRequired => "authority_receipt_required",
            Self::ScopePolicyDenied => "scope_policy_denied",
            Self::InvalidTransition => "invalid_transition",
            Self::DuplicateObservation => "duplicate_observation",
            Self::PromotionReceiptRequired => "promotion_receipt_required",
            Self::SelectionUnreplayable => "selection_unreplayable",
        }
    }
}

/// Typed M0 failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryError {
    code: MemoryErrorCode,
}

impl MemoryError {
    pub(crate) const fn new(code: MemoryErrorCode) -> Self {
        Self { code }
    }

    /// Returns the stable failure classification.
    pub const fn code(&self) -> MemoryErrorCode {
        self.code
    }
}

/// Exact inline or Runtime-resolvable content with a SHA-256 binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContentBinding {
    digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_utf8: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference: Option<String>,
}

impl ContentBinding {
    /// Constructs trusted inline UTF-8 and computes its exact digest.
    pub fn from_inline(inline_utf8: impl Into<String>) -> Self {
        let inline_utf8 = inline_utf8.into();
        Self {
            digest: sha256(inline_utf8.as_bytes()),
            inline_utf8: Some(inline_utf8),
            reference: None,
        }
    }

    /// Validates exact inline UTF-8 against a supplied digest.
    pub fn inline(
        digest: impl Into<String>,
        inline_utf8: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            digest: digest.into(),
            inline_utf8: Some(inline_utf8.into()),
            reference: None,
        };
        if !valid_digest(&value.digest)
            || sha256(value.inline_utf8.as_deref().unwrap().as_bytes()) != value.digest
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }

    /// Validates a Runtime-resolvable opaque reference and asserted digest.
    pub fn referenced(
        digest: impl Into<String>,
        reference: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            digest: digest.into(),
            inline_utf8: None,
            reference: Some(reference.into()),
        };
        if !valid_digest(&value.digest)
            || !valid_text(value.reference.as_deref().unwrap(), MAX_REFERENCE_BYTES)
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }

    /// Returns the exact content digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Returns inline content when carried directly.
    pub fn inline_utf8(&self) -> Option<&str> {
        self.inline_utf8.as_deref()
    }

    /// Returns the opaque reference when content is external.
    pub fn reference(&self) -> Option<&str> {
        self.reference.as_deref()
    }
}

/// Authorized scope of one memory record or query.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryScope {
    /// Facts and retrieval remain within one Session.
    Session {
        /// Exact owning Session identity.
        owner_id: String,
    },
    /// Facts and retrieval belong to one installed Agent instance.
    AgentInstance {
        /// Exact owning Agent instance identity.
        owner_id: String,
    },
    /// Runtime-authorized namespace scope without a user identity.
    Namespace,
}

impl MemoryScope {
    /// Validates a Session scope.
    pub fn session(owner_id: impl Into<String>) -> Result<Self, MemoryError> {
        owned_scope(owner_id, |owner_id| Self::Session { owner_id })
    }

    /// Validates an Agent-instance scope.
    pub fn agent_instance(owner_id: impl Into<String>) -> Result<Self, MemoryError> {
        owned_scope(owner_id, |owner_id| Self::AgentInstance { owner_id })
    }
}

/// Exact durable evidence binding verified by Runtime.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DurableFactReference {
    session_id: String,
    position: u64,
    fact_id: String,
    payload_digest: String,
}

impl DurableFactReference {
    /// Validates all four fixed-prefix evidence coordinates.
    pub fn new(
        session_id: impl Into<String>,
        position: u64,
        fact_id: impl Into<String>,
        payload_digest: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            session_id: session_id.into(),
            position,
            fact_id: fact_id.into(),
            payload_digest: payload_digest.into(),
        };
        if !valid_id(&value.session_id)
            || value.position == 0
            || !valid_id(&value.fact_id)
            || !valid_digest(&value.payload_digest)
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }

    /// Returns the Session containing the referenced fact.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Returns the exact non-zero Session-local position.
    pub const fn position(&self) -> u64 {
        self.position
    }

    /// Returns the referenced durable fact identity.
    pub fn fact_id(&self) -> &str {
        &self.fact_id
    }

    /// Returns the expected canonical payload digest.
    pub fn payload_digest(&self) -> &str {
        &self.payload_digest
    }
}

/// Portable semantic class of a memory record.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// User or actor preference evidence.
    Preference,
    /// Constraint that remains subordinate to current policy.
    Constraint,
    /// Durable decision evidence.
    Decision,
    /// Learned fact with explicit provenance.
    LearnedFact,
    /// Bounded derived summary.
    Summary,
}

/// Lifecycle status of one immutable revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    /// Eligible for authorized retrieval.
    Active,
    /// Replaced by a later exact revision.
    Superseded,
    /// Excluded from retrieval by a durable tombstone.
    Tombstoned,
}

/// Portable sensitivity class interpreted by Runtime authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySensitivity {
    /// Ordinary authorized memory.
    Ordinary,
    /// Requires an explicit frozen restricted grant.
    Restricted,
}

pub(crate) fn valid_id(value: &str) -> bool {
    valid_text(value, MAX_ID_BYTES)
}

pub(crate) fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= max_bytes && value.trim() == value
}

pub(crate) fn valid_digest(value: &str) -> bool {
    value.len() == SHA256_HEX_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn owned_scope(
    owner_id: impl Into<String>,
    constructor: impl FnOnce(String) -> MemoryScope,
) -> Result<MemoryScope, MemoryError> {
    let owner_id = owner_id.into();
    if !valid_id(&owner_id) {
        Err(MemoryError::new(MemoryErrorCode::InvalidMemory))
    } else {
        Ok(constructor(owner_id))
    }
}

fn sha256(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
