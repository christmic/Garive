use serde::Serialize;
use sha2::{Digest, Sha256};

const MAX_ID_BYTES: usize = 128;
const MAX_REFERENCE_BYTES: usize = 512;

/// Stable MA0 validation, authority, budget, child, result, or durability failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DelegationErrorCode {
    /// A value or relation violates the portable contract.
    InvalidDelegation,
    /// The requested existing child cannot be resolved.
    ChildNotFound,
    /// The resolved child definition revision differs.
    ChildRevisionMismatch,
    /// Runtime authority denied the delegation.
    AuthorityDenied,
    /// Remaining aggregate budget cannot cover the reservation.
    BudgetExhausted,
    /// Checked budget arithmetic overflowed.
    BudgetOverflow,
    /// The current delegation depth is not admitted.
    DepthExceeded,
    /// V1 already has an active delegation for the parent Turn.
    ConcurrencyExceeded,
    /// Completed child content violates the frozen result schema.
    ResultSchemaMismatch,
    /// A delegation identity or terminal result conflicts.
    DelegationConflict,
    /// Durable child lifecycle state is impossible.
    ChildStateCorrupt,
    /// A required durable operation failed.
    DurabilityFailure,
    /// Persisted delegation facts violate MA0 invariants.
    CorruptDelegationState,
}

impl DelegationErrorCode {
    /// Returns the exact stable wire name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::InvalidDelegation => "invalid_delegation",
            Self::ChildNotFound => "child_not_found",
            Self::ChildRevisionMismatch => "child_revision_mismatch",
            Self::AuthorityDenied => "authority_denied",
            Self::BudgetExhausted => "budget_exhausted",
            Self::BudgetOverflow => "budget_overflow",
            Self::DepthExceeded => "depth_exceeded",
            Self::ConcurrencyExceeded => "concurrency_exceeded",
            Self::ResultSchemaMismatch => "result_schema_mismatch",
            Self::DelegationConflict => "delegation_conflict",
            Self::ChildStateCorrupt => "child_state_corrupt",
            Self::DurabilityFailure => "durability_failure",
            Self::CorruptDelegationState => "corrupt_delegation_state",
        }
    }
}

/// Typed portable MA0 failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationError {
    code: DelegationErrorCode,
}

impl DelegationError {
    pub(crate) const fn new(code: DelegationErrorCode) -> Self {
        Self { code }
    }
    /// Returns the stable failure classification.
    pub const fn code(&self) -> DelegationErrorCode {
        self.code
    }
}

/// Exact inline or Runtime-resolvable content binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContentBinding {
    digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_utf8: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference: Option<String>,
}

impl ContentBinding {
    /// Creates an inline UTF-8 binding and computes its SHA-256 digest.
    pub fn from_inline(value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            digest: sha256(value.as_bytes()),
            inline_utf8: Some(value),
            reference: None,
        }
    }
    /// Creates a bounded Runtime-resolvable reference with an asserted digest.
    pub fn referenced(
        digest: impl Into<String>,
        reference: impl Into<String>,
    ) -> Result<Self, DelegationError> {
        let value = Self {
            digest: digest.into(),
            inline_utf8: None,
            reference: Some(reference.into()),
        };
        if !valid_digest(&value.digest)
            || !valid_text(value.reference.as_deref().unwrap(), MAX_REFERENCE_BYTES)
        {
            Err(DelegationError::new(DelegationErrorCode::InvalidDelegation))
        } else {
            Ok(value)
        }
    }
    /// Returns the exact content digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }
    /// Returns inline UTF-8 when carried by the binding.
    pub fn inline_utf8(&self) -> Option<&str> {
        self.inline_utf8.as_deref()
    }
    /// Returns the opaque Runtime reference when present.
    pub fn reference(&self) -> Option<&str> {
        self.reference.as_deref()
    }
}

/// Exact durable evidence reference admitted from the fixed Session prefix.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct FactReference {
    session_id: String,
    position: u64,
    fact_id: String,
    payload_digest: String,
}

impl FactReference {
    /// Validates one non-zero durable fact coordinate and payload binding.
    pub fn new(
        session_id: impl Into<String>,
        position: u64,
        fact_id: impl Into<String>,
        payload_digest: impl Into<String>,
    ) -> Result<Self, DelegationError> {
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
            Err(DelegationError::new(DelegationErrorCode::InvalidDelegation))
        } else {
            Ok(value)
        }
    }
    /// Returns owning Session identity.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// Returns Session-local durable position.
    pub const fn position(&self) -> u64 {
        self.position
    }
    /// Returns exact durable fact identity.
    pub fn fact_id(&self) -> &str {
        &self.fact_id
    }
    /// Returns exact referenced payload digest.
    pub fn payload_digest(&self) -> &str {
        &self.payload_digest
    }
}

/// Complete finite delegation reservation and content bounds.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DelegationBudget {
    /// Exactly one child Turn in v1.
    pub max_child_turns: u64,
    /// Maximum child Execution attempts including recovery.
    pub max_child_executions: u64,
    /// Maximum cumulative child Kernel iterations.
    pub max_iterations: u64,
    /// Maximum accounted child input tokens.
    pub max_input_tokens: u64,
    /// Maximum accounted child output tokens.
    pub max_output_tokens: u64,
    /// Maximum elapsed child lifecycle milliseconds.
    pub deadline_budget_ms: u64,
    /// Maximum admitted delegation depth.
    pub max_depth: u64,
    /// Maximum resolved objective UTF-8 bytes.
    pub max_objective_bytes: u64,
    /// Maximum ordered input evidence references.
    pub max_input_evidence: u64,
    /// Maximum canonical result-schema bytes.
    pub max_result_schema_bytes: u64,
    /// Maximum completed result content bytes.
    pub max_result_bytes: u64,
    /// Maximum completed result evidence references.
    pub max_result_evidence: u64,
}

impl DelegationBudget {
    /// Validates every non-zero bound and the v1 one-child-Turn restriction.
    pub fn validate(&self) -> Result<(), DelegationError> {
        let values = [
            self.max_child_executions,
            self.max_iterations,
            self.max_input_tokens,
            self.max_output_tokens,
            self.deadline_budget_ms,
            self.max_depth,
            self.max_objective_bytes,
            self.max_input_evidence,
            self.max_result_schema_bytes,
            self.max_result_bytes,
            self.max_result_evidence,
        ];
        if self.max_child_turns != 1 || values.contains(&0) {
            Err(DelegationError::new(DelegationErrorCode::InvalidDelegation))
        } else {
            Ok(())
        }
    }
}

pub(crate) fn valid_id(value: &str) -> bool {
    valid_text(value, MAX_ID_BYTES)
}
pub(crate) fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && value.trim() == value
}
pub(crate) fn sha256(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
