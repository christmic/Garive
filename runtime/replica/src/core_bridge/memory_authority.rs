use garive_memory::{MemoryErrorCode, MemoryProposal, MemoryQuery, MemoryScope, MemorySensitivity};

use crate::SqliteLedger;

use super::{verify_memory_evidence, MemoryPrefix};

/// Frozen Runtime authorization for one Memory namespace operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryAccessGrant {
    namespace_id: String,
    allowed_scopes: Vec<MemoryScope>,
    prefixes: Vec<MemoryPrefix>,
    restricted_grant_digest: Option<String>,
}

impl MemoryAccessGrant {
    /// Validates one explicit namespace/scope/prefix/sensitivity decision.
    pub fn new(
        namespace_id: impl Into<String>,
        allowed_scopes: Vec<MemoryScope>,
        prefixes: Vec<MemoryPrefix>,
        restricted_grant_digest: Option<String>,
    ) -> Result<Self, MemoryErrorCode> {
        let value = Self {
            namespace_id: namespace_id.into(),
            allowed_scopes,
            prefixes,
            restricted_grant_digest,
        };
        if value.namespace_id.is_empty()
            || value.allowed_scopes.is_empty()
            || !value
                .allowed_scopes
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            || value.prefixes.is_empty()
            || value
                .prefixes
                .iter()
                .any(|prefix| prefix.through_position == 0)
            || !value
                .prefixes
                .windows(2)
                .all(|pair| pair[0].session_id < pair[1].session_id)
            || value
                .restricted_grant_digest
                .as_deref()
                .is_some_and(|digest| !valid_digest(digest))
        {
            Err(MemoryErrorCode::InvalidMemory)
        } else {
            Ok(value)
        }
    }

    /// Returns the exact authorized fixed prefix set.
    pub fn prefixes(&self) -> &[MemoryPrefix] {
        &self.prefixes
    }
}

/// Verifies write namespace, scope, sensitivity and evidence under one grant.
pub fn authorize_memory_write(
    ledger: &SqliteLedger,
    grant: &MemoryAccessGrant,
    proposal: &MemoryProposal,
) -> Result<(), MemoryErrorCode> {
    if proposal.namespace_id() != grant.namespace_id
        || !grant.allowed_scopes.contains(proposal.scope())
    {
        return Err(MemoryErrorCode::NamespaceDenied);
    }
    if proposal.sensitivity() == MemorySensitivity::Restricted
        && grant.restricted_grant_digest.is_none()
    {
        return Err(MemoryErrorCode::SensitivityDenied);
    }
    verify_memory_evidence(ledger, &grant.prefixes, proposal)
}

/// Verifies query namespace, scope subset and exact restricted-grant binding.
pub fn authorize_memory_query(
    grant: &MemoryAccessGrant,
    query: &MemoryQuery,
) -> Result<(), MemoryErrorCode> {
    if query.namespace_id() != grant.namespace_id
        || query
            .allowed_scopes()
            .iter()
            .any(|scope| !grant.allowed_scopes.contains(scope))
    {
        return Err(MemoryErrorCode::NamespaceDenied);
    }
    if query.include_restricted()
        && query.restricted_grant_digest() != grant.restricted_grant_digest.as_deref()
    {
        return Err(MemoryErrorCode::SensitivityDenied);
    }
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
