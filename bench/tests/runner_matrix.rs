use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

use bench::{
    run_benchmark, AgentDriver, AgentInput, AgentOutput, BenchError, BenchErrorCode, BenchmarkMode,
    BenchmarkRunConfig, EnvironmentPool, EvaluationVerdict, IntakeAdapter, OfficialEvaluator,
    PatchAdapter, ResultSink, RunnerPorts, SweCase, WorkspaceLease,
};
use futures::{executor::block_on, future::poll_fn};
use garive_eval::{EvaluationCaseId, EvaluationCaseOutcome, EvaluationCaseResult};

#[derive(Clone, Copy, Eq, PartialEq)]
enum FailureStage {
    Acquire,
    Intake,
    Agent,
    Patch,
    Evaluate,
    Release,
    None,
}

struct Harness {
    failure: FailureStage,
    capacity: usize,
    active: AtomicUsize,
    maximum_active: AtomicUsize,
    releases: AtomicUsize,
    appended: Mutex<Vec<usize>>,
}

impl Harness {
    fn new(failure: FailureStage, capacity: usize) -> Self {
        Self {
            failure,
            capacity,
            active: AtomicUsize::new(0),
            maximum_active: AtomicUsize::new(0),
            releases: AtomicUsize::new(0),
            appended: Mutex::new(Vec::new()),
        }
    }

    fn ports(&self) -> RunnerPorts<'_> {
        RunnerPorts {
            environments: self,
            intake: self,
            agent: self,
            patch: self,
            evaluator: self,
            results: self,
        }
    }
}

impl EnvironmentPool for Harness {
    fn warm_capacity(&self) -> usize {
        self.capacity
    }

    fn acquire<'a>(&'a self, case: &'a SweCase) -> bench::BenchFuture<'a, WorkspaceLease> {
        Box::pin(async move {
            if self.failure == FailureStage::Acquire {
                return Err(failure());
            }
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            update_maximum(&self.maximum_active, active);
            Ok(WorkspaceLease {
                handle: format!("workspace-{}", case.instance_id.as_str()),
                case_id: case.instance_id.as_str().into(),
                base_commit: case.base_commit.clone(),
            })
        })
    }

    fn release<'a>(&'a self, _: WorkspaceLease) -> bench::BenchFuture<'a, ()> {
        Box::pin(async move {
            self.releases.fetch_add(1, Ordering::SeqCst);
            self.active.fetch_sub(1, Ordering::SeqCst);
            if self.failure == FailureStage::Release {
                Err(failure())
            } else {
                Ok(())
            }
        })
    }
}

impl IntakeAdapter for Harness {
    fn translate<'a>(
        &'a self,
        case: &'a SweCase,
        workspace: &'a WorkspaceLease,
    ) -> bench::BenchFuture<'a, AgentInput> {
        Box::pin(async move {
            if self.failure == FailureStage::Intake {
                return Err(failure());
            }
            Ok(AgentInput {
                payload: case.problem_statement.clone(),
                repository: case.repository.clone(),
                base_commit: case.base_commit.clone(),
                workspace_handle: workspace.handle.clone(),
            })
        })
    }
}

impl AgentDriver for Harness {
    fn run<'a>(
        &'a self,
        input: AgentInput,
        workspace: &'a WorkspaceLease,
    ) -> bench::BenchFuture<'a, AgentOutput> {
        Box::pin(async move {
            yield_times(workspace.case_id.bytes().last().unwrap_or(b'0') as usize % 4).await;
            if self.failure == FailureStage::Agent {
                return Err(failure());
            }
            Ok(AgentOutput {
                raw: format!("{}:{}", input.payload, workspace.handle),
                duration_ms: 7,
                input_tokens: Some(3),
                output_tokens: Some(2),
            })
        })
    }
}

impl PatchAdapter for Harness {
    fn translate<'a>(
        &'a self,
        _: &'a AgentOutput,
        case: &'a SweCase,
    ) -> bench::BenchFuture<'a, String> {
        Box::pin(async move {
            if self.failure == FailureStage::Patch {
                return Err(failure());
            }
            Ok(format!(
                "diff --git a/{}.txt b/{}.txt\n",
                case.instance_id.as_str(),
                case.instance_id.as_str()
            ))
        })
    }
}

impl OfficialEvaluator for Harness {
    fn evaluate<'a>(
        &'a self,
        case: &'a SweCase,
        _: &'a str,
    ) -> bench::BenchFuture<'a, EvaluationVerdict> {
        Box::pin(async move {
            if self.failure == FailureStage::Evaluate {
                return Err(failure());
            }
            if case.instance_id.as_str().ends_with('3') {
                Ok(EvaluationVerdict::Failed)
            } else {
                Ok(EvaluationVerdict::Passed)
            }
        })
    }
}

impl ResultSink for Harness {
    fn append<'a>(
        &'a self,
        source_index: usize,
        _: &'a EvaluationCaseResult,
    ) -> bench::BenchFuture<'a, ()> {
        Box::pin(async move {
            self.appended.lock().unwrap().push(source_index);
            Ok(())
        })
    }
}

#[test]
fn bounded_parallel_runner_emits_source_order_and_agent_failures_are_verdicts() {
    let harness = Harness::new(FailureStage::None, 4);
    let results = block_on(run_benchmark(
        &(0..4).map(case).collect::<Vec<_>>(),
        BenchmarkRunConfig {
            jobs: 2,
            mode: BenchmarkMode::Development,
        },
        harness.ports(),
    ))
    .unwrap();
    assert_eq!(*harness.appended.lock().unwrap(), [0, 1, 2, 3]);
    assert_eq!(harness.releases.load(Ordering::SeqCst), 4);
    assert_eq!(harness.maximum_active.load(Ordering::SeqCst), 2);
    assert_eq!(results[3].outcome, EvaluationCaseOutcome::Failed);
    assert_eq!(
        results
            .iter()
            .filter(|item| item.outcome == EvaluationCaseOutcome::InfrastructureFailure)
            .count(),
        0
    );
}

#[test]
fn every_failure_boundary_is_infrastructure_and_release_is_exact() {
    for stage in [
        FailureStage::Acquire,
        FailureStage::Intake,
        FailureStage::Agent,
        FailureStage::Patch,
        FailureStage::Evaluate,
        FailureStage::Release,
    ] {
        let harness = Harness::new(stage, 2);
        let results = block_on(run_benchmark(
            &[case(0)],
            BenchmarkRunConfig {
                jobs: 1,
                mode: BenchmarkMode::Development,
            },
            harness.ports(),
        ))
        .unwrap();
        assert_eq!(
            results[0].outcome,
            EvaluationCaseOutcome::InfrastructureFailure
        );
        let expected_releases = usize::from(stage != FailureStage::Acquire);
        assert_eq!(harness.releases.load(Ordering::SeqCst), expected_releases);
    }
}

#[test]
fn published_runs_require_parallel_warm_capacity_and_unique_cases() {
    let cases = vec![case(0), case(1)];
    let sequential = Harness::new(FailureStage::None, 2);
    assert_eq!(
        block_on(run_benchmark(
            &cases,
            BenchmarkRunConfig {
                jobs: 1,
                mode: BenchmarkMode::OfficialPublished
            },
            sequential.ports()
        ))
        .unwrap_err()
        .code(),
        BenchErrorCode::InvalidLimits
    );
    let cold = Harness::new(FailureStage::None, 1);
    assert_eq!(
        block_on(run_benchmark(
            &cases,
            BenchmarkRunConfig {
                jobs: 2,
                mode: BenchmarkMode::OfficialPublished
            },
            cold.ports()
        ))
        .unwrap_err()
        .code(),
        BenchErrorCode::InvalidLimits
    );
    let duplicate = Harness::new(FailureStage::None, 2);
    assert_eq!(
        block_on(run_benchmark(
            &[case(0), case(0)],
            BenchmarkRunConfig {
                jobs: 2,
                mode: BenchmarkMode::Development
            },
            duplicate.ports()
        ))
        .unwrap_err()
        .code(),
        BenchErrorCode::DuplicateCase
    );
}

fn case(index: usize) -> SweCase {
    SweCase {
        instance_id: EvaluationCaseId::new(format!("case-{index}")).unwrap(),
        repository: "owner/repo".into(),
        base_commit: "a".repeat(40),
        problem_statement: format!("problem-{index}"),
        version: "1".into(),
        fail_to_pass: vec![format!("test-{index}")],
        pass_to_pass: vec![],
    }
}

fn failure() -> BenchError {
    BenchError::from_port(BenchErrorCode::InfrastructureFailure)
}

fn update_maximum(maximum: &AtomicUsize, value: usize) {
    let mut current = maximum.load(Ordering::SeqCst);
    while value > current {
        match maximum.compare_exchange(current, value, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}

async fn yield_times(times: usize) {
    for _ in 0..times {
        let mut yielded = false;
        poll_fn(move |context| {
            if yielded {
                std::task::Poll::Ready(())
            } else {
                yielded = true;
                context.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        })
        .await;
    }
}
