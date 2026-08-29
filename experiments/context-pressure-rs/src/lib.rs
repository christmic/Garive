//! Reproducible C7-A corpus loading and context-pressure evidence tooling.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod command_counter;
mod corpus;
mod counter;
mod error;
mod http_exchange;
mod provider_counter;
mod publication_counter;
mod runner;
mod system_credential;

pub use command_counter::{CommandTokenCounter, CommandTokenCounterConfig};
pub use corpus::{load_corpus, ContextPressureCase, ContextPressureCorpus};
pub use counter::{TokenCounter, TokenCounterDescriptor, TokenCounterFailure};
pub use error::{ContextPressureError, ContextPressureErrorCode};
pub use garive_experiment_evidence::{
    attest_clean_revision, reserve_evidence_file, EvidenceFileError, EvidenceFileReservation,
    GitAttestationConfig, GitAttestationDescriptor, GitAttestationFailure,
};
pub use http_exchange::{ReqwestTokenCountExchangePort, TokenCountHttpLimits};
pub use provider_counter::{
    AnthropicProviderCounter, AnthropicProviderCounterConfig, TokenCountExchangePort,
};
pub use publication_counter::{
    build_publication_provider_counter, CredentialReferenceResolver, CredentialResolutionFailure,
    ProviderCounterBuildError, ProviderCounterRunConfig, PublicationProviderCounter,
};
pub use runner::{measure_context_pressure, ContextPressureRun};
pub use system_credential::{SystemCredentialReferenceResolver, PRESSURE_CREDENTIAL_SERVICE};
