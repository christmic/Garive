//! Thin official SWE benchmark loader, driver, adapters and evaluator boundary.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod adapters;
mod case;
mod command_ports;
mod error;
mod official;
mod runner;
mod tracking;
mod unique_json;

pub use adapters::{ExactSweIntake, UnifiedDiffPatchAdapter};
pub use case::{parse_cases, CaseLoadLimits, SweCase, SweDataset};
pub use command_ports::{CommandAgentDriver, CommandEnvironmentPool, CommandPortConfig};
pub use error::{BenchError, BenchErrorCode};
pub use official::{
    OfficialEvaluatorConfig, OfficialInvocation, OfficialProcess, OfficialProcessOutput,
    StdOfficialProcess, SweBenchOfficialEvaluator,
};
pub use runner::{
    run_benchmark, AgentDriver, AgentInput, AgentOutput, BenchFuture, BenchmarkMode,
    BenchmarkRunConfig, EnvironmentPool, EvaluationVerdict, IntakeAdapter, OfficialEvaluator,
    PatchAdapter, ResultSink, RunnerPorts, WorkspaceLease,
};
pub use tracking::{JsonlResultSink, TrackingCompletion, TrackingDescriptor};
