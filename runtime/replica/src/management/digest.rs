use serde::Serialize;
use sha2::{Digest, Sha256};

/// Domain identifier stamped into every canonicalized management-config digest.
///
/// Any party that reads or replays a digest MUST first check this contract id,
/// refusing to interpret the digest if the contract differs.
#[allow(dead_code)] // consumed by commit-3 HTTP layer + tests
pub const MANAGEMENT_CONFIG_CONTRACT: &str = "garive.management-config.v1";

/// Envelope field set hashed to derive `configuration_digest` on commit.
///
/// Fields are intentionally ordered alphabetically; the [`serde_jcs`] encoder
/// emits canonical JSON in key order, and alphabetical order is what existing
/// digest contracts in the engine/tools crate use.
#[derive(Debug, Serialize)]
pub(crate) struct ConfigurationDigestEnvelope<'a> {
    pub(crate) api_key: &'a str,
    pub(crate) definition_id: &'a str,
    pub(crate) deployment_id: &'a str,
    pub(crate) endpoint_override: Option<&'a str>,
    pub(crate) model_id: &'a str,
    pub(crate) model_target_id: &'a str,
    pub(crate) profile_id: &'a str,
    pub(crate) runtime_id: &'a str,
}

/// Envelope field set hashed to derive `receipt_digest` after a successful
/// commit.
#[derive(Debug, Serialize)]
pub(crate) struct ReceiptDigestEnvelope<'a> {
    pub(crate) configuration_digest: &'a str,
    pub(crate) configuration_revision: u64,
    pub(crate) restart_required: bool,
}

/// Canonical SHA-256 hex digest of a management-configuration commit body.
///
/// Bound to the [`MANAGEMENT_CONFIG_CONTRACT`] constant; rolling the constant
/// is a deliberately explicit version bump rather than a hidden field change.
pub fn configuration_digest(envelope: &ConfigurationDigestEnvelope<'_>) -> String {
    let canonical = serde_jcs::to_vec(envelope).expect("canonical JSON encoding is infallible");
    format!("{:x}", Sha256::digest(canonical))
}

/// Canonical SHA-256 hex digest of a management-configuration commit receipt.
pub fn receipt_digest(envelope: &ReceiptDigestEnvelope<'_>) -> String {
    let canonical = serde_jcs::to_vec(envelope).expect("canonical JSON encoding is infallible");
    format!("{:x}", Sha256::digest(canonical))
}
