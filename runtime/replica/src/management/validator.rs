use std::sync::Arc;

use super::types::{ManagementCommitBody, ManagementConfigError};

/// Pluggable semantic validation for a [`ManagementCommitBody`].
///
/// Lives below the per-field wire validation enforced by
/// [`super::store::ManagementConfigStore::commit`]; the validator's job is to
/// bind values to the host's catalogue (e.g. known Provider profile ids,
/// known Agent definition ids) and to enforce any cross-field rules the
/// Runtime contract requires.
pub trait ManagementValidator: Send + Sync {
    /// Validates the body; `Ok(())` accepts, an `Err` rejects with a
    /// stable wire code via [`ManagementConfigError::wire_code`].
    fn validate(&self, body: &ManagementCommitBody) -> Result<(), ManagementConfigError>;
}

impl<T> ManagementValidator for Arc<T>
where
    T: ManagementValidator + ?Sized,
{
    fn validate(&self, body: &ManagementCommitBody) -> Result<(), ManagementConfigError> {
        (**self).validate(body)
    }
}

/// Permissive validator that accepts every well-formed body. Useful for
/// tests and for hosts that do not yet bundle a Registry-backed catalogue.
pub struct AllowAllValidator;

impl ManagementValidator for AllowAllValidator {
    fn validate(&self, _body: &ManagementCommitBody) -> Result<(), ManagementConfigError> {
        Ok(())
    }
}
