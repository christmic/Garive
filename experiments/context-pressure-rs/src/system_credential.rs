use garive_provider_profile::SecretValue;

use crate::{CredentialReferenceResolver, CredentialResolutionFailure};

/// OS credential-store service reserved for context-pressure publication.
pub const PRESSURE_CREDENTIAL_SERVICE: &str = "com.garive.context-pressure";

/// Shipping resolver backed only by the operating-system credential store.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemCredentialReferenceResolver;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
impl CredentialReferenceResolver for SystemCredentialReferenceResolver {
    fn resolve(&self, credential_ref: &str) -> Result<SecretValue, CredentialResolutionFailure> {
        let entry = keyring::Entry::new(PRESSURE_CREDENTIAL_SERVICE, credential_ref)
            .map_err(|_| CredentialResolutionFailure)?;
        let credential = entry
            .get_password()
            .map_err(|_| CredentialResolutionFailure)?;
        SecretValue::new(credential).map_err(|_| CredentialResolutionFailure)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
impl CredentialReferenceResolver for SystemCredentialReferenceResolver {
    fn resolve(&self, _: &str) -> Result<SecretValue, CredentialResolutionFailure> {
        Err(CredentialResolutionFailure)
    }
}
