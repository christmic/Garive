//! Shared bounded provenance support for evidence-only experiment runners.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod git_attestation;

pub use git_attestation::{
    attest_clean_revision, GitAttestationConfig, GitAttestationDescriptor, GitAttestationFailure,
};
