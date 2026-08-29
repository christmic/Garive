use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
};

use futures::{stream, StreamExt};
use garive_eval::{EvaluationCaseOutcome, EvaluationCaseResult};

use crate::{BenchError, BenchErrorCode, SweCase};

const MAX_JOBS: usize = 64;

/// Boxed asynchronous benchmark-port operation.
pub type BenchFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BenchError>> + Send + 'a>>;

/// Whether a run may produce published evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BenchmarkMode {
    /// Local smoke/development evidence, never publishable.
    Development,
    /// Official evaluator and warm pool suitable for publication.
    OfficialPublished,
}

/// Explicit bounded driver configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BenchmarkRunConfig {
    /// Maximum concurrently active cases.
    pub jobs: usize,
    /// Publication classification.
    pub mode: BenchmarkMode,
}

/// Exact per-case workspace ownership returned by the environment pool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceLease {
    /// Opaque environment-owned handle.
    pub handle: String,
    /// Official case bound to this workspace.
    pub case_id: String,
    /// Exact repository commit checked out in the workspace.
    pub base_commit: String,
}

/// Gold-free input translated for one Agent driver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentInput {
    /// Exact official problem statement or agent-native encoding of it.
    pub payload: String,
    /// Exact public repository identity.
    pub repository: String,
    /// Exact base commit.
    pub base_commit: String,
    /// Opaque workspace handle.
    pub workspace_handle: String,
}

/// Raw terminal Agent output and optional usage evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentOutput {
    /// Adapter-owned raw output.
    pub raw: String,
    /// Measured case duration.
    pub duration_ms: u64,
    /// Known model input tokens, or unknown.
    pub input_tokens: Option<u64>,
    /// Known model output tokens, or unknown.
    pub output_tokens: Option<u64>,
}

/// Official evaluator's only Agent verdicts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationVerdict {
    /// Official tests resolved the case.
    Passed,
    /// Official tests did not resolve the case.
    Failed,
}

/// Warm, bounded per-case environment pool.
pub trait EnvironmentPool: Sync {
    /// Number of workspaces that can be active without setup-time pulling.
    fn warm_capacity(&self) -> usize;
    /// Acquires one isolated workspace for the exact case.
    fn acquire<'a>(&'a self, case: &'a SweCase) -> BenchFuture<'a, WorkspaceLease>;
    /// Releases exactly one successfully acquired workspace.
    fn release<'a>(&'a self, lease: WorkspaceLease) -> BenchFuture<'a, ()>;
}

/// Mandatory case-to-Agent intake translation.
pub trait IntakeAdapter: Sync {
    /// Produces gold-free Agent input for a bound workspace.
    fn translate<'a>(
        &'a self,
        case: &'a SweCase,
        workspace: &'a WorkspaceLease,
    ) -> BenchFuture<'a, AgentInput>;
}

/// Injected Agent under evaluation.
pub trait AgentDriver: Sync {
    /// Runs the Agent only inside the acquired workspace.
    fn run<'a>(
        &'a self,
        input: AgentInput,
        workspace: &'a WorkspaceLease,
    ) -> BenchFuture<'a, AgentOutput>;
}

/// Mandatory Agent-output-to-patch translation.
pub trait PatchAdapter: Sync {
    /// Produces one bounded canonical unified diff.
    fn translate<'a>(
        &'a self,
        output: &'a AgentOutput,
        case: &'a SweCase,
    ) -> BenchFuture<'a, String>;
}

/// Independent official SWE-bench evaluator boundary.
pub trait OfficialEvaluator: Sync {
    /// Evaluates one canonical patch without access to the Agent workspace.
    fn evaluate<'a>(
        &'a self,
        case: &'a SweCase,
        patch: &'a str,
    ) -> BenchFuture<'a, EvaluationVerdict>;
}

/// Ordered durable/result output boundary.
pub trait ResultSink: Sync {
    /// Appends one terminal result in original source order.
    fn append<'a>(
        &'a self,
        source_index: usize,
        result: &'a EvaluationCaseResult,
    ) -> BenchFuture<'a, ()>;
}

/// Complete injected port set for the only B0 execution path.
#[derive(Clone, Copy)]
pub struct RunnerPorts<'a> {
    /// Environment pool.
    pub environments: &'a dyn EnvironmentPool,
    /// Intake adapter.
    pub intake: &'a dyn IntakeAdapter,
    /// Agent driver.
    pub agent: &'a dyn AgentDriver,
    /// Patch adapter.
    pub patch: &'a dyn PatchAdapter,
    /// Official evaluator.
    pub evaluator: &'a dyn OfficialEvaluator,
    /// Ordered result sink.
    pub results: &'a dyn ResultSink,
}

/// Runs every case through acquire → intake → Agent → patch → evaluator → release.
pub async fn run_benchmark(
    cases: &[SweCase],
    config: BenchmarkRunConfig,
    ports: RunnerPorts<'_>,
) -> Result<Vec<EvaluationCaseResult>, BenchError> {
    validate_run(cases, config, ports.environments.warm_capacity())?;
    let mut active = stream::iter(cases.iter().cloned().enumerate())
        .map(|(index, case)| async move { (index, run_case(case, ports).await) })
        .buffer_unordered(config.jobs);
    let mut pending = BTreeMap::new();
    let mut ordered = Vec::with_capacity(cases.len());
    let mut next = 0;
    while let Some((index, result)) = active.next().await {
        pending.insert(index, result);
        while let Some(result) = pending.remove(&next) {
            ports.results.append(next, &result).await?;
            ordered.push(result);
            next += 1;
        }
    }
    Ok(ordered)
}

async fn run_case(case: SweCase, ports: RunnerPorts<'_>) -> EvaluationCaseResult {
    let lease = match ports.environments.acquire(&case).await {
        Ok(value) if valid_lease(&case, &value) => value,
        _ => return infrastructure(&case, None),
    };
    let mut usage = None;
    let verdict = match ports.intake.translate(&case, &lease).await {
        Err(_) => None,
        Ok(input) if !valid_input(&case, &lease, &input) => None,
        Ok(input) => match ports.agent.run(input, &lease).await {
            Err(_) => None,
            Ok(output) => {
                usage = Some((
                    output.duration_ms,
                    output.input_tokens,
                    output.output_tokens,
                ));
                match ports.patch.translate(&output, &case).await {
                    Err(_) => None,
                    Ok(patch) => ports.evaluator.evaluate(&case, &patch).await.ok(),
                }
            }
        },
    };
    if ports.environments.release(lease).await.is_err() {
        return infrastructure(&case, usage);
    }
    let Some(verdict) = verdict else {
        return infrastructure(&case, usage);
    };
    let (duration_ms, input_tokens, output_tokens) = usage.unwrap_or((0, None, None));
    EvaluationCaseResult {
        case_id: case.instance_id,
        outcome: match verdict {
            EvaluationVerdict::Passed => EvaluationCaseOutcome::Passed,
            EvaluationVerdict::Failed => EvaluationCaseOutcome::Failed,
        },
        duration_ms,
        input_tokens,
        output_tokens,
    }
}

fn validate_run(
    cases: &[SweCase],
    config: BenchmarkRunConfig,
    warm_capacity: usize,
) -> Result<(), BenchError> {
    if cases.is_empty() || !(1..=MAX_JOBS).contains(&config.jobs) {
        return Err(BenchError::new(BenchErrorCode::InvalidLimits));
    }
    let mut identities = BTreeSet::new();
    if cases
        .iter()
        .any(|case| !identities.insert(case.instance_id.as_str()))
    {
        return Err(BenchError::new(BenchErrorCode::DuplicateCase));
    }
    if config.mode == BenchmarkMode::OfficialPublished
        && (config.jobs == 1 || warm_capacity < config.jobs)
    {
        return Err(BenchError::new(BenchErrorCode::InvalidLimits));
    }
    Ok(())
}

fn valid_lease(case: &SweCase, lease: &WorkspaceLease) -> bool {
    !lease.handle.is_empty()
        && lease.case_id == case.instance_id.as_str()
        && lease.base_commit == case.base_commit
}

fn valid_input(case: &SweCase, lease: &WorkspaceLease, input: &AgentInput) -> bool {
    !input.payload.is_empty()
        && input.repository == case.repository
        && input.base_commit == case.base_commit
        && input.workspace_handle == lease.handle
}

fn infrastructure(
    case: &SweCase,
    usage: Option<(u64, Option<u64>, Option<u64>)>,
) -> EvaluationCaseResult {
    let (duration_ms, input_tokens, output_tokens) = usage.unwrap_or((0, None, None));
    EvaluationCaseResult {
        case_id: case.instance_id.clone(),
        outcome: EvaluationCaseOutcome::InfrastructureFailure,
        duration_ms,
        input_tokens,
        output_tokens,
    }
}
