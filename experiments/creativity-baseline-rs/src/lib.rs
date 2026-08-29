//! Reproducible paired evidence tooling for the CR-A creativity prerequisite.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod command_ports;
mod command_process;
mod corpus;
mod error;
mod port;
mod runner;

pub use command_ports::{CommandCreativityEvaluator, CommandCreativityGenerator};
pub use command_process::CommandPortConfig;
pub use corpus::{load_creativity_corpus, CreativityCorpus, CreativityTask};
pub use error::{CreativityBaselineError, CreativityBaselineErrorCode};
pub use port::{
    CandidateVerdict, CreativityEvaluatorPort, CreativityGeneratorPort, EvaluatorRequest,
    ExperimentPortDescriptor, GeneratedArm, GeneratedCandidate, GeneratorRequest,
};
pub use runner::{run_creativity_baseline, CreativityBaselineRun};
