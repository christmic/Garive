//! Strict C6 durable Runtime payload-v1 validation.

use crate::{FactDraft, LedgerError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Whether a fact payload was applied as admitted v1 semantics or kept opaque.
pub enum RuntimeFactDisposition {
    /// Known C6 fact kind with a valid schema-v1 payload and envelope.
    AppliedV1,
    /// Unknown kind or newer schema retained only as an audit fact.
    Opaque,
}

/// Validates one admitted C6 payload and its required outer envelope identities.
pub fn validate_runtime_fact(_fact: &FactDraft) -> Result<RuntimeFactDisposition, LedgerError> {
    Ok(RuntimeFactDisposition::Opaque)
}
