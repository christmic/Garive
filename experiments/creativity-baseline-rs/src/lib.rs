//! Reproducible paired evidence tooling for the CR-A creativity prerequisite.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod command_ports;
mod command_process;
mod corpus;
mod error;
mod model_ports;
mod port;
mod publication_model;
mod runner;
mod system_credential;

pub use command_ports::{CommandCreativityEvaluator, CommandCreativityGenerator};
pub use command_process::CommandPortConfig;
pub use corpus::{load_creativity_corpus, CreativityCorpus, CreativityTask};
pub use error::{CreativityBaselineError, CreativityBaselineErrorCode};
pub use model_ports::{
    ModelCreativityConfig, ModelCreativityEvaluator, ModelCreativityGenerator,
    EVALUATOR_TEMPLATE_REVISION, GENERATOR_TEMPLATE_REVISION,
};
pub use port::{
    CandidateVerdict, CreativityEvaluatorPort, CreativityGeneratorPort, EvaluatorRequest,
    ExperimentPortDescriptor, GeneratedArm, GeneratedCandidate, GeneratorRequest,
};
pub use publication_model::{
    build_publication_evaluator, build_publication_generator, model_endpoint_publication_eligible,
    CredentialReferenceResolver, CredentialResolutionFailure, ModelEndpointConfig, ModelProtocol,
    NonSecretHeader, PublicationModelCoordinate,
};
pub use runner::{run_creativity_baseline, CreativityBaselineRun};
pub use system_credential::{SystemCredentialReferenceResolver, CREATIVITY_CREDENTIAL_SERVICE};
